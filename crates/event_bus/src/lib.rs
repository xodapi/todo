use protocol::*;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum AppEvent {
    WindowsActivityRecorded(WindowsActivity),
    InputMetricsRecorded(InputMetrics),
    SystemMessage(String),
}

pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
