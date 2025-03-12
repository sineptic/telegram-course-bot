use std::path::PathBuf;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TelegramInteraction {
    OneOf(Vec<String>),
    Text(String),
    UserInput,
    Image(PathBuf),
}

#[derive(Debug, Clone)]
pub enum QuestionElement {
    Text(String),
    Image(PathBuf),
}
impl From<QuestionElement> for TelegramInteraction {
    fn from(element: QuestionElement) -> Self {
        match element {
            QuestionElement::Text(text) => TelegramInteraction::Text(text),
            QuestionElement::Image(image) => TelegramInteraction::Image(image),
        }
    }
}

pub fn one_of<C, S>(options: C) -> Vec<String>
where
    C: IntoIterator<Item = S>,
    S: Into<String>,
{
    options.into_iter().map(|s| s.into()).collect()
}

#[derive(Debug, Clone)]
pub struct Task {
    pub question: Vec<QuestionElement>,
    pub options: Vec<String>,
    pub answer: usize,
}
impl Task {
    pub fn correct_answer(&self) -> &str {
        &self.options[self.answer]
    }
    pub fn interactions(&self) -> Vec<TelegramInteraction> {
        let mut interactions = Vec::new();
        for element in &self.question {
            interactions.push(element.clone().into());
        }
        interactions.push(TelegramInteraction::OneOf(self.options.clone()));
        interactions
    }
}
