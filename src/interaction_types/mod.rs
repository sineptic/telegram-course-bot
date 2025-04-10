use std::path::PathBuf;

use crate::check;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TelegramInteraction {
    OneOf(Vec<String>),
    Text(String),
    UserInput,
    Image(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
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
impl QuestionElement {
    pub fn from_str(input: &str) -> Result<Self, TaskParseError> {
        let input = input.trim();
        assert!(input.lines().count() == 1);
        assert!(!input.is_empty());

        match input.as_bytes()[0] {
            b'!' => {
                let path = input
                    .strip_prefix("![")
                    .ok_or(TaskParseError::InvalidImageSyntax)?
                    .strip_suffix("]")
                    .ok_or(TaskParseError::InvalidImageSyntax)?;
                Ok(QuestionElement::Image(PathBuf::from(path)))
            }
            _ => Ok(QuestionElement::Text(input.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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

const ERROR_MSG: &str = "Task should follow this syntax:
...
input
...
           <- empty line
* correct option
- options
...
";
#[derive(Debug, thiserror::Error)]
pub enum TaskParseError {
    #[error("{ERROR_MSG}. Input shouldn't be empty")]
    EmptyInput,
    // NoQuestion,
    #[error("{ERROR_MSG}. No 'options' provided")]
    NoOptions,
    #[error("{ERROR_MSG}. First 'option' should be correct and line should start with '* '")]
    NoCorrectOption,
    #[error(
        "{ERROR_MSG}. After correct 'option' required at least one incorrect, so line should start with '- '"
    )]
    NoIncorrectOption,
    #[error("{ERROR_MSG}. Correct option should start with '* ' and incorrect with '- '")]
    InvalidOptionPrefix,
    #[error("{ERROR_MSG}. Each option should contain non empty text")]
    EmptyOptionText,
    #[error("mage should have this syntax: ![path_to_image]")]
    InvalidImageSyntax,
}

impl Task {
    pub fn from_str(
        input: impl AsRef<str>,
        multiline_messages: bool,
    ) -> Result<Self, TaskParseError> {
        let input = input.as_ref().trim();
        if input.is_empty() {
            return Err(TaskParseError::EmptyInput);
        }
        let lines = input.lines().map(|x| x.trim());
        let (mut question, remainder) = parse_question(lines)?;
        if multiline_messages {
            question = merge_messages(question);
        }
        let options = parse_options(remainder)?;

        Ok(Task {
            question,
            options,
            answer: 0,
        })
    }
}

fn parse_options<'a>(
    mut remainder: impl Iterator<Item = &'a str>,
) -> Result<Vec<String>, TaskParseError> {
    let mut options = Vec::new();
    let Some(first_line) = remainder.next() else {
        return Err(TaskParseError::NoOptions);
    };
    check!(
        is_option_string_prefix_valid(first_line),
        TaskParseError::InvalidOptionPrefix
    );
    let first_line = first_line
        .strip_prefix("* ")
        .ok_or(TaskParseError::NoCorrectOption)?
        .trim();
    check!(!first_line.is_empty(), TaskParseError::EmptyOptionText);
    options.push(first_line.to_owned());
    for line in remainder {
        check!(
            is_option_string_prefix_valid(line),
            TaskParseError::InvalidOptionPrefix
        );
        let line = line
            .strip_prefix("- ")
            .ok_or(TaskParseError::NoIncorrectOption)?
            .trim();
        check!(!line.is_empty(), TaskParseError::EmptyOptionText);
        options.push(line.to_owned());
    }
    check!(options.len() > 1, TaskParseError::NoIncorrectOption);
    Ok(options)
}

fn is_option_string_prefix_valid(line: &str) -> bool {
    line.starts_with("* ") || line.starts_with("- ")
}

fn merge_messages(question: Vec<QuestionElement>) -> Vec<QuestionElement> {
    let mut new_question = Vec::new();
    let mut prev: Option<String> = None;
    for question_part in question {
        match question_part {
            QuestionElement::Text(text) => {
                if let Some(prev) = &mut prev {
                    prev.push_str(&text);
                } else {
                    prev = Some(text);
                }
            }
            QuestionElement::Image(_) => {
                if let Some(prev) = prev.take() {
                    new_question.push(QuestionElement::Text(prev));
                }
                new_question.push(question_part);
            }
        }
    }
    if let Some(prev) = prev.take() {
        new_question.push(QuestionElement::Text(prev));
    }
    new_question
}

fn parse_question<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<(Vec<QuestionElement>, impl Iterator<Item = &'a str>), TaskParseError> {
    let mut question = Vec::new();
    for line in &mut lines {
        if line.is_empty() {
            return Ok((question, lines));
        }
        question.push(QuestionElement::from_str(line)?);
    }
    Err(TaskParseError::NoOptions)
}
