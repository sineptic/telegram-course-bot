use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TelegramInteraction {
    OneOf(Vec<String>),
    Text(String),
    UserInput,
    Image(PathBuf),
}
