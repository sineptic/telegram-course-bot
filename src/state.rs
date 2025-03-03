use teloxide::types::MessageId;
use tokio::sync::oneshot;

use super::*;

#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    UserEvent {
        interactions: Vec<TelegramInteraction>,
        current: usize,
        current_id: u64,
        current_message: Option<MessageId>,
        answers: Vec<String>,
        channel: Option<oneshot::Sender<Vec<String>>>,
    },
}
