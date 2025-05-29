use super::telegram_interaction::{QuestionElement, TelegramInteraction};
use crate::check;

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub question: Vec<QuestionElement>,
    pub options: Vec<String>,
    pub answer: usize,
    pub explanation: Option<Vec<QuestionElement>>,
}

impl Task {
    #[allow(unused)]
    pub fn correct_answer(&self) -> &str {
        &self.options[self.answer]
    }
    #[allow(unused)]
    pub fn interactions(&self) -> Vec<TelegramInteraction> {
        let mut interactions = Vec::new();
        for element in &self.question {
            interactions.push(element.clone().into());
        }
        interactions.push(TelegramInteraction::OneOf(self.options.clone()));
        interactions
    }
}

pub(crate) const ERROR_MSG: &str = "Task should follow this syntax:
...
'question':
text
![link_to_image]
...
            <- empty line
* correct 'option'
- options
...
            <- empty line
'explanation'
in format of 'question'
...
";

#[derive(Debug, thiserror::Error, PartialEq)]
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
    #[error("Image should have this syntax: ![link_to_image]")]
    InvalidImageSyntax,
    #[error("Image should be valid link. Error: {0}")]
    ImageShouldBeLink(#[from] url::ParseError),
    #[error("{ERROR_MSG}. Task should not have anything after explanation")]
    ContentAfterExplanation,
}

impl Task {
    pub fn from_str(
        input: impl AsRef<str>,
        multiline_messages: bool,
    ) -> Result<Self, TaskParseError> {
        let input = input.as_ref().trim();
        check!(!input.is_empty(), TaskParseError::EmptyInput);
        let lines = input.lines().map(|x| x.trim());

        let (question, remainder) = parse_messages(lines, multiline_messages)?;
        let (options, remainder) = parse_options(remainder)?;
        let explanation = parse_explanation(multiline_messages, remainder)?;

        Ok(Task {
            question,
            options,
            answer: 0,
            explanation,
        })
    }
}

pub(crate) fn parse_explanation<'a>(
    multiline_messages: bool,
    remainder: impl Iterator<Item = &'a str>,
) -> Result<Option<Vec<QuestionElement>>, TaskParseError> {
    let (explanation, tail) = parse_messages(remainder, multiline_messages)?;
    check!(tail.count() == 0, TaskParseError::ContentAfterExplanation);
    if explanation.is_empty() {
        Ok(None)
    } else {
        Ok(Some(explanation))
    }
}

pub(crate) fn parse_options<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<(Vec<String>, impl Iterator<Item = &'a str>), TaskParseError> {
    let mut options = Vec::new();
    let Some(first_line) = lines.next() else {
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
    for line in &mut lines {
        if line.is_empty() {
            check!(options.len() > 1, TaskParseError::NoIncorrectOption);
            return Ok((options, lines));
        }
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
    Ok((options, lines))
}

pub(crate) fn is_option_string_prefix_valid(line: &str) -> bool {
    line.starts_with("* ") || line.starts_with("- ")
}

pub(crate) fn merge_messages(question: Vec<QuestionElement>) -> Vec<QuestionElement> {
    let mut new_question = Vec::new();
    let mut prev: Option<String> = None;
    for question_part in question {
        match question_part {
            QuestionElement::Text(text) => {
                if let Some(prev) = &mut prev {
                    prev.push('\n');
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

pub(crate) fn parse_messages<'a>(
    mut lines: impl Iterator<Item = &'a str>,
    multiline_messages: bool,
) -> Result<(Vec<QuestionElement>, impl Iterator<Item = &'a str>), TaskParseError> {
    let mut question = Vec::new();
    for line in &mut lines {
        if line.is_empty() {
            break;
        }
        question.push(QuestionElement::from_str(line)?);
    }
    if multiline_messages {
        question = merge_messages(question);
    }
    Ok((question, lines))
}
