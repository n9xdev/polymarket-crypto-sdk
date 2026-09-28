use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum EventKind {
    Decision,
    OrderPost,
    Fill,
    Heartbeat,
    SlotSample,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::OrderPost => "order_post",
            Self::Fill => "fill",
            Self::Heartbeat => "heartbeat",
            Self::SlotSample => "slot_sample",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub kind: EventKind,
    pub payload: Value,
}

pub struct EventLog {
    tx: mpsc::UnboundedSender<EventRecord>,
}

impl EventLog {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<EventRecord>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn append(&self, kind: EventKind, payload: Value) {
        let _ = self.tx.send(EventRecord {
            kind,
            payload: serde_json::json!({
                "ts": Utc::now().to_rfc3339(),
                "data": payload,
            }),
        });
    }
}

pub fn spawn_writer(
    mut rx: mpsc::UnboundedReceiver<EventRecord>,
    store: Arc<crate::postgres::PostgresStore>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(rec) = rx.recv().await {
            if let Err(e) = store.insert_event(rec.kind.as_str(), rec.payload).await {
                tracing::warn!(error = %e, "event log write failed");
            }
        }
    })
}
