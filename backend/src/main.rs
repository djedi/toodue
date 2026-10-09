mod auth;
mod db;
mod error;
mod events;
mod gcal;
mod ics;
mod mcp;
mod models;
mod oauth;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: db::Database,
    pub hub: events::Hub,
    pub data_dir: PathBuf,
    pub http: reqwest::Client,
    pub oauth_states: gcal::OauthStates,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "toodue=info,tower_http=info".into()),
        )
        .init();

    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into()));
    std::fs::create_dir_all(data_dir.join("attachments")).expect("create data dir");

    let db = db::connect(&data_dir).await.expect("open database");
    let state = AppState {
        db,
        hub: events::Hub::new(),
        data_dir,
        http: reqwest::Client::new(),
        oauth_states: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };

    // Keep Google Calendar watch channels alive and sync tokens primed.
    {
        let st = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
            loop {
                interval.tick().await;
                gcal::renew_channels(&st).await;
            }
        });
    }

    // Expire stale OAuth requests, codes, and tokens.
    {
        let st = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                oauth::cleanup(&st).await;
            }
        });
    }

    let api = Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/me", get(auth::me))
        .route(
            "/api-keys",
            get(routes::api_keys::list).post(routes::api_keys::create),
        )
        .route("/api-keys/{id}", delete(routes::api_keys::remove))
        .route("/mcp/info", get(oauth::server_info))
        .route("/oauth/connections", get(oauth::list_connections))
        .route("/oauth/connections/{id}", delete(oauth::remove_connection))
        .route(
            "/oauth/requests/{id}",
            get(oauth::request_info).post(oauth::decide),
        )
        .route("/ai/me", get(routes::ai::me))
        .route(
            "/ai/projects",
            get(routes::ai::list_projects).post(routes::ai::create_project),
        )
        .route("/ai/projects/{id}", get(routes::ai::project_detail))
        .route(
            "/ai/tasks",
            get(routes::ai::list_tasks).post(routes::ai::create_task),
        )
        .route("/ai/tasks/search", get(routes::ai::search_tasks))
        .route(
            "/ai/tasks/{id}",
            get(routes::ai::task_detail)
                .patch(routes::ai::update_task)
                .delete(routes::ai::delete_task),
        )
        .route(
            "/projects",
            get(routes::projects::list).post(routes::projects::create),
        )
        .route(
            "/projects/{id}",
            axum::routing::patch(routes::projects::update).delete(routes::projects::remove),
        )
        .route("/projects/reorder", post(routes::projects::reorder))
        .route("/projects/share-bulk", post(routes::projects::share_bulk))
        .route("/projects/{id}/share", post(routes::projects::share))
        .route(
            "/templates",
            get(routes::templates::list).post(routes::templates::create),
        )
        .route(
            "/templates/{id}/import",
            post(routes::templates::import_template),
        )
        .route("/templates/{id}", delete(routes::templates::remove))
        .route(
            "/projects/{id}/members/{user_id}",
            delete(routes::projects::remove_member),
        )
        .route(
            "/tasks",
            get(routes::tasks::list).post(routes::tasks::create),
        )
        .route("/tasks/search", get(routes::tasks::search))
        .route(
            "/tasks/{id}",
            get(routes::tasks::detail)
                .patch(routes::tasks::update)
                .delete(routes::tasks::remove),
        )
        .route("/tasks/{id}/comments", post(routes::comments::create))
        .route("/comments/{id}", delete(routes::comments::remove))
        .route(
            "/tasks/{id}/attachments",
            post(routes::attachments::upload).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route(
            "/attachments/{id}",
            get(routes::attachments::download).delete(routes::attachments::remove),
        )
        .route(
            "/import/backup",
            post(routes::import::backup).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/events", get(events::sse_handler))
        .route("/calendar/{token}", get(ics::feed))
        .route("/me/calendar", get(ics::my_url).post(ics::rotate))
        .route("/google/status", get(gcal::status))
        .route("/google/connect", get(gcal::connect))
        .route("/google/callback", get(gcal::callback))
        .route("/google/disconnect", post(gcal::disconnect))
        .route("/google/webhook", post(gcal::webhook));

    let static_dir =
        PathBuf::from(std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()));
    let spa = ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html")));

    // Endpoints MCP clients call directly. Browser-based clients (e.g. the MCP
    // Inspector) need CORS; auth is bearer-only, so no credentials are allowed.
    let mcp_routes = Router::new()
        .route("/mcp", get(mcp::handle_get).post(mcp::handle_post))
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(oauth::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::authorization_server_metadata),
        )
        .route("/oauth/register", post(oauth::register))
        .route("/oauth/token", post(oauth::token))
        .route("/oauth/revoke", post(oauth::revoke))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers([
                    axum::http::header::WWW_AUTHENTICATE,
                    axum::http::HeaderName::from_static("mcp-session-id"),
                ]),
        );

    let app = Router::new()
        .nest("/api", api)
        .merge(mcp_routes)
        .route("/oauth/authorize", get(oauth::authorize))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("TooDue listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server");
}
