use std::sync::Arc;

use teloxide::types::MessageId;
use tokio::sync::{Mutex, oneshot};

use super::*;

#[derive(Clone, Default)]
pub enum State {
    #[default]
    General,
    UserEvent {
        interactions: Vec<TelegramInteraction>,
        current: usize,
        current_id: u64,
        current_message: Option<MessageId>,
        answers: Vec<String>,
        channel: Arc<Mutex<Option<oneshot::Sender<Vec<String>>>>>,
    },
}
