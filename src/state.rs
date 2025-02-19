use super::*;

#[derive(Clone, Default)]
pub enum State {
    #[default]
    General,
    UserEvent {
        interactions: Vec<TelegramInteraction>,
        current: usize,
        current_id: u64,
        answers: Vec<String>,
    },
}

pub type MyDialogue = Dialogue<State, InMemStorage<State>>;
