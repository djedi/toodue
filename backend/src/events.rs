use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::AuthUser;
use crate::AppState;

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<OutEvent>,
}

#[derive(Clone)]
pub struct OutEvent {
    pub recipients: Arc<Vec<i64>>,
    pub payload: Arc<String>,
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, recipients: Vec<i64>, kind: &str, data: serde_json::Value) {
        let payload = serde_json::json!({ "type": kind, "data": data }).to_string();
        let _ = self.tx.send(OutEvent {
            recipients: Arc::new(recipients),
            payload: Arc::new(payload),
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OutEvent> {
        self.tx.subscribe()
    }
}

pub async fn sse_handler(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = user.id;
    let stream = BroadcastStream::new(st.hub.subscribe()).filter_map(move |msg| {
        let out = match msg {
            Ok(ev) if ev.recipients.contains(&user_id) => {
                Some(Ok(Event::default().data(ev.payload.as_str())))
            }
            _ => None,
        };
        futures::future::ready(out)
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("ping"),
    )
}

pub async fn project_recipients(db: &sqlx::AnyPool, project_id: i64) -> Vec<i64> {
    sqlx::query_as::<_, (i64,)>(&*crate::db::sql(
        "SELECT user_id FROM project_members WHERE project_id = ?",
    ))
    .bind(project_id)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| r.0)
    .collect()
}
