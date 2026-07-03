mod auth;
mod db;
mod error;
mod events;
mod ics;
mod models;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub hub: events::Hub,
    pub data_dir: PathBuf,
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
    };

    let api = Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route(
            "/projects",
            get(routes::projects::list).post(routes::projects::create),
        )
        .route(
            "/projects/{id}",
            axum::routing::patch(routes::projects::update).delete(routes::projects::remove),
        )
        .route("/projects/reorder", post(routes::projects::reorder))
        .route("/projects/{id}/share", post(routes::projects::share))
        .route(
            "/projects/{id}/members/{user_id}",
            delete(routes::projects::remove_member),
        )
        .route(
            "/tasks",
            get(routes::tasks::list).post(routes::tasks::create),
        )
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
            "/import/todoist",
            post(routes::import::todoist).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/events", get(events::sse_handler))
        .route("/calendar/{token}", get(ics::feed))
        .route("/me/calendar", get(ics::my_url).post(ics::rotate));

    let static_dir =
        PathBuf::from(std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()));
    let spa = ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html")));

    let app = Router::new()
        .nest("/api", api)
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
