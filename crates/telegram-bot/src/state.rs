use teloxide_core::types::MessageId;
use tokio::sync::oneshot;

use crate::{database::CourseId, interaction_types::TelegramInteraction};

#[derive(Default)]
pub struct UserState {
    pub current_screen: Screen,
    pub current_interaction: Option<UserInteraction>,
}

#[derive(Default)]
pub enum Screen {
    #[default]
    Main,
    Course(CourseId),
}

pub struct UserInteraction {
    pub interactions: Vec<TelegramInteraction>,
    pub current: usize,
    pub current_id: u64,
    pub current_message: Option<MessageId>,
    pub answers: Vec<String>,
    pub channel: Option<oneshot::Sender<Vec<String>>>,
}
