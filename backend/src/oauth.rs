//! OAuth 2.1 authorization server for MCP clients.
//!
//! Implements what the MCP authorization spec expects: protected resource and
//! authorization server metadata, dynamic client registration (plus URL-based
//! client ID metadata documents), authorization code + PKCE (S256 only),
//! rotating refresh tokens, and revocation. All clients are public clients.
//! The consent screen itself lives in the SPA at `/connect`.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::{hash_token, now_iso, random_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::models::User;
use crate::AppState;

pub const SCOPES: [&str; 3] = ["read", "write", "delete"];
const ACCESS_TOKEN_SECS: i64 = 3600;
const REFRESH_TOKEN_DAYS: i64 = 90;
const REQUEST_SECS: i64 = 15 * 60;
const CODE_SECS: i64 = 5 * 60;

fn iso_in(d: Duration) -> String {
    (Utc::now() + d)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Public origin of this server, e.g. `https://app.toodue.com`. `PUBLIC_URL` wins;
/// otherwise it is derived from the (possibly proxied) request.
pub fn base_url(headers: &HeaderMap) -> String {
    if let Some(url) = std::env::var("PUBLIC_URL").ok().filter(|s| !s.is_empty()) {
        return url.trim_end_matches('/').to_string();
    }
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let host = get("x-forwarded-host")
        .or_else(|| get("host"))
        .unwrap_or_else(|| "localhost:8080".into());
    let proto = get("x-forwarded-proto").unwrap_or_else(|| {
        if is_loopback_host(host.split(':').next().unwrap_or("")) {
            "http".into()
        } else {
            "https".into()
        }
    });
    format!("{proto}://{host}")
}

pub fn mcp_url(headers: &HeaderMap) -> String {
    format!("{}/mcp", base_url(headers))
}

pub fn resource_metadata_url(headers: &HeaderMap) -> String {
    format!(
        "{}/.well-known/oauth-protected-resource/mcp",
        base_url(headers)
    )
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// Normalizes a space-separated scope string to known scopes in canonical order.
pub fn parse_scope(scope: &str) -> Vec<&'static str> {
    let wanted: Vec<&str> = scope.split_whitespace().collect();
    SCOPES
        .iter()
        .copied()
        .filter(|s| wanted.contains(s))
        .collect()
}

/* ---------- metadata ---------- */

pub async fn protected_resource_metadata(headers: HeaderMap) -> Json<Value> {
    let base = base_url(&headers);
    Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "scopes_supported": SCOPES,
        "bearer_methods_supported": ["header"],
        "resource_name": "TooDue",
        "resource_documentation": "https://docs.toodue.com/mcp.html",
    }))
}

pub async fn authorization_server_metadata(headers: HeaderMap) -> Json<Value> {
    let base = base_url(&headers);
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "scopes_supported": SCOPES,
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true,
        "service_documentation": "https://docs.toodue.com/mcp.html",
    }))
}

/* ---------- errors ---------- */

/// RFC 6749 style error body (`{"error": "...", "error_description": "..."}`).
pub struct OAuthError(StatusCode, &'static str, String);

impl OAuthError {
    fn bad(code: &'static str, desc: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, code, desc.into())
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        (
            self.0,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({ "error": self.1, "error_description": self.2 })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for OAuthError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("database error: {e}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error".into(),
        )
    }
}

/* ---------- clients ---------- */

/// Redirect URIs must be https, http on a loopback host (native apps), or a
/// private-use app scheme such as `cursor://`. Fragments are never allowed.
pub fn valid_redirect_uri(uri: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    if url.fragment().is_some() {
        return false;
    }
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => url.host_str().is_some_and(is_loopback_host),
        "javascript" | "data" | "file" | "vbscript" | "blob" | "about" | "ftp" | "ws" | "wss" => {
            false
        }
        _ => true,
    }
}

/// Exact match, except loopback redirects may use any port (RFC 8252 §7.3).
pub fn redirect_matches(registered: &str, requested: &str) -> bool {
    if registered == requested {
        return true;
    }
    let (Ok(a), Ok(b)) = (Url::parse(registered), Url::parse(requested)) else {
        return false;
    };
    a.scheme() == "http"
        && b.scheme() == "http"
        && a.host_str().is_some_and(is_loopback_host)
        && a.host_str() == b.host_str()
        && a.path() == b.path()
        && a.query() == b.query()
}

#[derive(Deserialize)]
pub struct RegisterBody {
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    client_uri: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
}

pub async fn register(
    State(st): State<AppState>,
    Json(b): Json<RegisterBody>,
) -> Result<(StatusCode, Json<Value>), OAuthError> {
    if b.redirect_uris.is_empty() || b.redirect_uris.len() > 10 {
        return Err(OAuthError::bad(
            "invalid_redirect_uri",
            "between 1 and 10 redirect_uris are required",
        ));
    }
    if let Some(bad) = b.redirect_uris.iter().find(|u| !valid_redirect_uri(u)) {
        return Err(OAuthError::bad(
            "invalid_redirect_uri",
            format!("redirect URI not allowed: {bad}"),
        ));
    }
    let name = clean_name(b.client_name.as_deref());
    let client_uri = b.client_uri.filter(|u| u.starts_with("https://"));
    let client_id = format!("tdc_{}", &random_token()[..32]);
    sqlx::query(&*crate::db::sql(
        "INSERT INTO oauth_clients (id, name, client_uri, redirect_uris) VALUES (?, ?, ?, ?)",
    ))
    .bind(&client_id)
    .bind(&name)
    .bind(&client_uri)
    .bind(serde_json::to_string(&b.redirect_uris).unwrap_or_default())
    .execute(&st.db.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "client_id": client_id,
            "client_id_issued_at": Utc::now().timestamp(),
            "client_name": name,
            "client_uri": client_uri,
            "redirect_uris": b.redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "scope": SCOPES.join(" "),
        })),
    ))
}

fn clean_name(name: Option<&str>) -> String {
    let name: String = name
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Unnamed MCP client".into()
    } else {
        name.to_string()
    }
}

struct Client {
    id: String,
    redirect_uris: Vec<String>,
}

async fn load_client(st: &AppState, client_id: &str) -> Result<Option<Client>, String> {
    if client_id.starts_with("https://") {
        return fetch_metadata_document(st, client_id).await.map(Some);
    }
    let row: Option<(String, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT id, redirect_uris FROM oauth_clients WHERE id = ?",
    ))
    .bind(client_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|(id, uris)| Client {
        id,
        redirect_uris: serde_json::from_str(&uris).unwrap_or_default(),
    }))
}

/// Client ID Metadata Documents: the client_id is an https URL serving the
/// client's registration JSON. The fetched copy is cached in oauth_clients so
/// grants can reference it.
async fn fetch_metadata_document(st: &AppState, client_id: &str) -> Result<Client, String> {
    let url = Url::parse(client_id).map_err(|_| "invalid client_id URL".to_string())?;
    if url.scheme() != "https" || url.path() == "/" || url.fragment().is_some() {
        return Err("client_id URL must be https with a path".into());
    }
    let res = st
        .http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("could not fetch client metadata: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("client metadata returned {}", res.status()));
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("client metadata document is too large".into());
    }
    let doc: Value = serde_json::from_slice(&bytes).map_err(|_| "client metadata is not JSON")?;
    if doc["client_id"].as_str() != Some(client_id) {
        return Err("client metadata client_id does not match".into());
    }
    let redirect_uris: Vec<String> = doc["redirect_uris"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if redirect_uris.is_empty() || !redirect_uris.iter().all(|u| valid_redirect_uri(u)) {
        return Err("client metadata has invalid redirect_uris".into());
    }
    let name = clean_name(doc["client_name"].as_str());
    let client_uri = doc["client_uri"]
        .as_str()
        .filter(|u| u.starts_with("https://"))
        .map(String::from);
    let uris_json = serde_json::to_string(&redirect_uris).unwrap_or_default();
    sqlx::query(&*crate::db::sql(
        "INSERT INTO oauth_clients (id, name, client_uri, redirect_uris) VALUES (?, ?, ?, ?) \
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, client_uri = excluded.client_uri, \
         redirect_uris = excluded.redirect_uris",
    ))
    .bind(client_id)
    .bind(&name)
    .bind(&client_uri)
    .bind(&uris_json)
    .execute(&st.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(Client {
        id: client_id.to_string(),
        redirect_uris,
    })
}

/* ---------- authorization endpoint ---------- */

fn with_query(redirect_uri: &str, params: &[(&str, &str)]) -> String {
    match Url::parse(redirect_uri) {
        Ok(mut url) => {
            {
                let mut q = url.query_pairs_mut();
                for (k, v) in params {
                    q.append_pair(k, v);
                }
            }
            url.to_string()
        }
        Err(_) => redirect_uri.to_string(),
    }
}

fn error_page(message: &str) -> Response {
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        format!(
            "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\">\
             <title>Authorization error · TooDue</title>\
             <body style=\"font:16px/1.5 system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\">\
             <h1 style=\"font-size:1.25rem\">This connection request can't be completed</h1>\
             <p>{escaped}</p><p>Go back to your AI agent and try connecting again.</p>"
        ),
    )
        .into_response()
}

pub async fn authorize(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let get = |k: &str| q.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let Some(client_id) = get("client_id") else {
        return error_page("The request is missing client_id.");
    };
    let client = match load_client(&st, client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return error_page("This app isn't registered with TooDue."),
        Err(e) => return error_page(&e),
    };
    let redirect_uri = match get("redirect_uri") {
        Some(r) => r,
        None if client.redirect_uris.len() == 1 => client.redirect_uris[0].as_str(),
        None => return error_page("The request is missing redirect_uri."),
    };
    if !client
        .redirect_uris
        .iter()
        .any(|r| redirect_matches(r, redirect_uri))
    {
        return error_page("The redirect URI doesn't match this app's registration.");
    }

    // From here on errors go back to the client.
    let state = get("state");
    let fail = |code: &str, desc: &str| {
        let mut params = vec![("error", code), ("error_description", desc)];
        if let Some(s) = state {
            params.push(("state", s));
        }
        Redirect::to(&with_query(redirect_uri, &params)).into_response()
    };
    if get("response_type") != Some("code") {
        return fail("unsupported_response_type", "response_type must be code");
    }
    let Some(challenge) = get("code_challenge") else {
        return fail("invalid_request", "PKCE code_challenge is required");
    };
    if get("code_challenge_method") != Some("S256") {
        return fail("invalid_request", "code_challenge_method must be S256");
    }
    if let Some(resource) = get("resource") {
        if resource.trim_end_matches('/') != mcp_url(&headers) {
            return fail("invalid_target", "unknown resource");
        }
    }
    let mut scopes = match get("scope") {
        Some(s) => parse_scope(s),
        None => SCOPES.to_vec(),
    };
    if scopes.is_empty() {
        return fail("invalid_scope", "no supported scopes requested");
    }
    if !scopes.contains(&"read") {
        scopes.insert(0, "read");
    }

    let id = random_token();
    let res = sqlx::query(&*crate::db::sql(
        "INSERT INTO oauth_requests (id, client_id, redirect_uri, scope, state, code_challenge, resource, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    ))
    .bind(&id)
    .bind(&client.id)
    .bind(redirect_uri)
    .bind(scopes.join(" "))
    .bind(state)
    .bind(challenge)
    .bind(get("resource"))
    .bind(iso_in(Duration::seconds(REQUEST_SECS)))
    .execute(&st.db.pool)
    .await;
    if let Err(e) = res {
        tracing::error!("database error: {e}");
        return fail("server_error", "internal error");
    }
    Redirect::to(&format!("/connect?request={id}")).into_response()
}

/* ---------- consent (called by the SPA with the session cookie) ---------- */

#[derive(sqlx::FromRow)]
struct PendingRequest {
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: Option<String>,
    code_challenge: String,
    name: String,
    client_uri: Option<String>,
}

async fn pending_request(st: &AppState, id: &str) -> ApiResult<PendingRequest> {
    sqlx::query_as::<_, PendingRequest>(&*crate::db::sql(
        "SELECT r.client_id, r.redirect_uri, r.scope, r.state, r.code_challenge, c.name, c.client_uri \
         FROM oauth_requests r JOIN oauth_clients c ON c.id = r.client_id \
         WHERE r.id = ? AND r.expires_at > ?",
    ))
    .bind(id)
    .bind(now_iso())
    .fetch_optional(&st.db.pool)
    .await?
    .ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            "This connection request has expired. Start again from your AI agent.".into(),
        )
    })
}

/// Host (or scheme for app redirects) the user is sending the code to.
fn redirect_label(uri: &str) -> String {
    match Url::parse(uri) {
        Ok(u) => match u.host_str() {
            Some(h) if u.scheme() == "https" || u.scheme() == "http" => match u.port() {
                Some(p) => format!("{h}:{p}"),
                None => h.to_string(),
            },
            _ => format!("{}://", u.scheme()),
        },
        Err(_) => uri.to_string(),
    }
}

pub async fn request_info(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let req = pending_request(&st, &id).await?;
    let existing: Option<(String,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT scope FROM oauth_grants WHERE user_id = ? AND client_id = ?",
    ))
    .bind(user.id)
    .bind(&req.client_id)
    .fetch_optional(&st.db.pool)
    .await?;
    Ok(Json(json!({
        "client_name": req.name,
        "client_uri": req.client_uri,
        "redirect": redirect_label(&req.redirect_uri),
        "scopes": parse_scope(&req.scope),
        "connected_scopes": existing.map(|e| parse_scope(&e.0)),
    })))
}

#[derive(Deserialize)]
pub struct Decision {
    approve: bool,
    #[serde(default)]
    scopes: Vec<String>,
}

pub async fn decide(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(b): Json<Decision>,
) -> ApiResult<Json<Value>> {
    let req = pending_request(&st, &id).await?;
    // Single use: whichever way the user decides, the request is consumed.
    sqlx::query(&*crate::db::sql("DELETE FROM oauth_requests WHERE id = ?"))
        .bind(&id)
        .execute(&st.db.pool)
        .await?;
    let mut params: Vec<(&str, String)> = Vec::new();
    if b.approve {
        let requested = parse_scope(&req.scope);
        let chosen: Vec<&str> = requested
            .iter()
            .copied()
            .filter(|s| *s == "read" || b.scopes.iter().any(|c| c == s))
            .collect();
        let scope = chosen.join(" ");
        let (grant_id,): (i64,) = sqlx::query_as(&*crate::db::sql(
            "INSERT INTO oauth_grants (user_id, client_id, scope) VALUES (?, ?, ?) \
             ON CONFLICT (user_id, client_id) DO UPDATE SET scope = excluded.scope RETURNING id",
        ))
        .bind(user.id)
        .bind(&req.client_id)
        .bind(&scope)
        .fetch_one(&st.db.pool)
        .await?;
        let code = random_token();
        sqlx::query(&*crate::db::sql(
            "INSERT INTO oauth_codes (code_hash, grant_id, redirect_uri, code_challenge, scope, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        ))
        .bind(hash_token(&code))
        .bind(grant_id)
        .bind(&req.redirect_uri)
        .bind(&req.code_challenge)
        .bind(&scope)
        .bind(iso_in(Duration::seconds(CODE_SECS)))
        .execute(&st.db.pool)
        .await?;
        params.push(("code", code));
    } else {
        params.push(("error", "access_denied".into()));
        params.push(("error_description", "The user denied access".into()));
    }
    if let Some(s) = req.state {
        params.push(("state", s));
    }
    let pairs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(Json(json!({
        "redirect_to": with_query(&req.redirect_uri, &pairs),
    })))
}

/* ---------- token endpoint ---------- */

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    refresh_token: String,
    scope: String,
}

async fn issue_tokens(st: &AppState, grant_id: i64, scope: &str) -> Result<Response, OAuthError> {
    let access = format!("tdue_at_{}", random_token());
    let refresh = format!("tdue_rt_{}", random_token());
    for (token, kind, expires) in [
        (
            &access,
            "access",
            iso_in(Duration::seconds(ACCESS_TOKEN_SECS)),
        ),
        (
            &refresh,
            "refresh",
            iso_in(Duration::days(REFRESH_TOKEN_DAYS)),
        ),
    ] {
        sqlx::query(&*crate::db::sql(
            "INSERT INTO oauth_tokens (token_hash, grant_id, kind, scope, expires_at) VALUES (?, ?, ?, ?, ?)",
        ))
        .bind(hash_token(token))
        .bind(grant_id)
        .bind(kind)
        .bind(scope)
        .bind(expires)
        .execute(&st.db.pool)
        .await?;
    }
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(TokenResponse {
            access_token: access,
            token_type: "Bearer",
            expires_in: ACCESS_TOKEN_SECS,
            refresh_token: refresh,
            scope: scope.to_string(),
        }),
    )
        .into_response())
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn token(
    State(st): State<AppState>,
    Form(f): Form<HashMap<String, String>>,
) -> Result<Response, OAuthError> {
    let get = |k: &str| f.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let client_id = get("client_id")
        .ok_or_else(|| OAuthError::bad("invalid_request", "client_id is required"))?;
    match get("grant_type") {
        Some("authorization_code") => {
            let code = get("code")
                .ok_or_else(|| OAuthError::bad("invalid_request", "code is required"))?;
            let verifier = get("code_verifier")
                .ok_or_else(|| OAuthError::bad("invalid_request", "code_verifier is required"))?;
            let code_hash = hash_token(code);
            let row: Option<(i64, String, String, String, String)> =
                sqlx::query_as(&*crate::db::sql(
                    "SELECT c.grant_id, c.redirect_uri, c.code_challenge, c.scope, g.client_id \
                     FROM oauth_codes c JOIN oauth_grants g ON g.id = c.grant_id \
                     WHERE c.code_hash = ? AND c.expires_at > ?",
                ))
                .bind(&code_hash)
                .bind(now_iso())
                .fetch_optional(&st.db.pool)
                .await?;
            // Codes are single use, valid or not.
            sqlx::query(&*crate::db::sql(
                "DELETE FROM oauth_codes WHERE code_hash = ?",
            ))
            .bind(&code_hash)
            .execute(&st.db.pool)
            .await?;
            let (grant_id, redirect_uri, challenge, scope, grant_client) =
                row.ok_or_else(|| {
                    OAuthError::bad("invalid_grant", "authorization code is invalid or expired")
                })?;
            if grant_client != client_id {
                return Err(OAuthError::bad(
                    "invalid_grant",
                    "code was issued to another client",
                ));
            }
            if let Some(r) = get("redirect_uri") {
                if r != redirect_uri {
                    return Err(OAuthError::bad("invalid_grant", "redirect_uri mismatch"));
                }
            }
            if pkce_challenge(verifier) != challenge {
                return Err(OAuthError::bad("invalid_grant", "PKCE verification failed"));
            }
            issue_tokens(&st, grant_id, &scope).await
        }
        Some("refresh_token") => {
            let refresh = get("refresh_token")
                .ok_or_else(|| OAuthError::bad("invalid_request", "refresh_token is required"))?;
            let token_hash = hash_token(refresh);
            let row: Option<(i64, String, String)> = sqlx::query_as(&*crate::db::sql(
                "SELECT t.grant_id, t.scope, g.client_id FROM oauth_tokens t \
                 JOIN oauth_grants g ON g.id = t.grant_id \
                 WHERE t.token_hash = ? AND t.kind = 'refresh' AND t.expires_at > ?",
            ))
            .bind(&token_hash)
            .bind(now_iso())
            .fetch_optional(&st.db.pool)
            .await?;
            let (grant_id, scope, grant_client) = row.ok_or_else(|| {
                OAuthError::bad("invalid_grant", "refresh token is invalid or expired")
            })?;
            if grant_client != client_id {
                return Err(OAuthError::bad(
                    "invalid_grant",
                    "token was issued to another client",
                ));
            }
            // A narrower scope may be requested; never a wider one.
            let scope = match get("scope") {
                Some(s) => {
                    let current = parse_scope(&scope);
                    let wanted = parse_scope(s);
                    if wanted.is_empty() || !wanted.iter().all(|w| current.contains(w)) {
                        return Err(OAuthError::bad("invalid_scope", "scope exceeds the grant"));
                    }
                    wanted.join(" ")
                }
                None => scope,
            };
            // Rotate: the old refresh token stops working.
            sqlx::query(&*crate::db::sql(
                "DELETE FROM oauth_tokens WHERE token_hash = ?",
            ))
            .bind(&token_hash)
            .execute(&st.db.pool)
            .await?;
            issue_tokens(&st, grant_id, &scope).await
        }
        _ => Err(OAuthError::bad(
            "unsupported_grant_type",
            "grant_type must be authorization_code or refresh_token",
        )),
    }
}

pub async fn revoke(
    State(st): State<AppState>,
    Form(f): Form<HashMap<String, String>>,
) -> Result<StatusCode, OAuthError> {
    if let Some(token) = f.get("token") {
        sqlx::query(&*crate::db::sql(
            "DELETE FROM oauth_tokens WHERE token_hash = ?",
        ))
        .bind(hash_token(token))
        .execute(&st.db.pool)
        .await?;
    }
    Ok(StatusCode::OK)
}

/* ---------- bearer resolution for /mcp ---------- */

pub struct OAuthAccess {
    pub user: User,
    pub scopes: Vec<&'static str>,
}

pub async fn access_from_token(st: &AppState, token: &str) -> ApiResult<Option<OAuthAccess>> {
    let row: Option<(i64, String, String, i64, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT u.id, u.email, u.name, g.id, t.scope FROM oauth_tokens t \
         JOIN oauth_grants g ON g.id = t.grant_id JOIN users u ON u.id = g.user_id \
         WHERE t.token_hash = ? AND t.kind = 'access' AND t.expires_at > ?",
    ))
    .bind(hash_token(token))
    .bind(now_iso())
    .fetch_optional(&st.db.pool)
    .await?;
    let Some((id, email, name, grant_id, scope)) = row else {
        return Ok(None);
    };
    sqlx::query(&*crate::db::sql(
        "UPDATE oauth_grants SET last_used_at = ? WHERE id = ?",
    ))
    .bind(now_iso())
    .bind(grant_id)
    .execute(&st.db.pool)
    .await?;
    Ok(Some(OAuthAccess {
        user: User { id, email, name },
        scopes: parse_scope(&scope),
    }))
}

/* ---------- connected apps (settings screen) ---------- */

#[derive(Serialize, sqlx::FromRow)]
pub struct Connection {
    pub id: i64,
    pub client_name: String,
    pub client_uri: Option<String>,
    pub scope: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

pub async fn list_connections(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<Connection>>> {
    let rows = sqlx::query_as::<_, Connection>(&*crate::db::sql(
        "SELECT g.id, c.name AS client_name, c.client_uri, g.scope, g.last_used_at, g.created_at \
         FROM oauth_grants g JOIN oauth_clients c ON c.id = g.client_id \
         WHERE g.user_id = ? ORDER BY g.created_at DESC",
    ))
    .bind(user.id)
    .fetch_all(&st.db.pool)
    .await?;
    Ok(Json(rows))
}

pub async fn remove_connection(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    // Tokens and codes cascade with the grant.
    let result = sqlx::query(&*crate::db::sql(
        "DELETE FROM oauth_grants WHERE id = ? AND user_id = ?",
    ))
    .bind(id)
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn server_info(headers: HeaderMap) -> Json<Value> {
    Json(json!({ "mcp_url": mcp_url(&headers) }))
}

/// Drops expired requests, codes, and tokens.
pub async fn cleanup(st: &AppState) {
    let now = now_iso();
    for table in ["oauth_requests", "oauth_codes", "oauth_tokens"] {
        let sql = format!("DELETE FROM {table} WHERE expires_at <= ?");
        if let Err(e) = sqlx::query(&*crate::db::sql(&sql))
            .bind(&now)
            .execute(&st.db.pool)
            .await
        {
            tracing::warn!("oauth cleanup of {table} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn redirect_uri_rules() {
        assert!(valid_redirect_uri(
            "https://claude.ai/api/mcp/auth_callback"
        ));
        assert!(valid_redirect_uri("http://127.0.0.1:33418/callback"));
        assert!(valid_redirect_uri("http://localhost/callback"));
        assert!(valid_redirect_uri(
            "cursor://anysphere.cursor-mcp/oauth/callback"
        ));
        assert!(!valid_redirect_uri("http://evil.example/callback"));
        assert!(!valid_redirect_uri("javascript:alert(1)"));
        assert!(!valid_redirect_uri("https://example.com/cb#frag"));
        assert!(!valid_redirect_uri("not a url"));
    }

    #[test]
    fn loopback_redirects_ignore_port() {
        assert!(redirect_matches(
            "http://127.0.0.1:1234/callback",
            "http://127.0.0.1:5678/callback"
        ));
        assert!(!redirect_matches(
            "http://127.0.0.1:1234/callback",
            "http://127.0.0.1:5678/other"
        ));
        assert!(!redirect_matches(
            "https://example.com/cb",
            "https://example.com:8443/cb"
        ));
    }

    #[test]
    fn scopes_are_normalized() {
        assert_eq!(parse_scope("delete read bogus"), vec!["read", "delete"]);
        assert!(parse_scope("").is_empty());
    }

    #[test]
    fn query_params_are_appended() {
        assert_eq!(
            with_query(
                "https://a.example/cb?x=1",
                &[("code", "abc"), ("state", "s t")]
            ),
            "https://a.example/cb?x=1&code=abc&state=s+t"
        );
    }
}
