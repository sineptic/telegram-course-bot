use url::Url;

use super::task::TaskParseError;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TelegramInteraction {
    OneOf(Vec<String>),
    Text(String),
    UserInput,
    Image(Url),
    PersonalImage(Vec<u8>),
}
impl<T> From<T> for TelegramInteraction
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        let text = value.into();
        let escaped = text.replace(".", "\\.").replace("!", "\\!");
        TelegramInteraction::Text(escaped)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionElement {
    Text(String),
    Image(Url),
}

impl From<QuestionElement> for TelegramInteraction {
    fn from(element: QuestionElement) -> Self {
        match element {
            QuestionElement::Text(text) => text.into(),
            QuestionElement::Image(image) => TelegramInteraction::Image(image),
        }
    }
}

impl QuestionElement {
    pub fn from_str(input: &str) -> Result<Self, TaskParseError> {
        let input = input.trim();
        assert!(input.lines().count() == 1);
        assert!(!input.is_empty());

        match input.as_bytes()[0] {
            b'!' => {
                let link = input
                    .strip_prefix("![")
                    .ok_or(TaskParseError::InvalidImageSyntax)?
                    .strip_suffix("]")
                    .ok_or(TaskParseError::InvalidImageSyntax)?;
                Ok(QuestionElement::Image(link.parse()?))
            }
            _ => Ok(QuestionElement::Text(input.to_string())),
        }
    }
}
