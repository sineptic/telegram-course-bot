use std::collections::BTreeMap;

use super::{Task, task::TaskParseError};
use crate::check;

const USAGE: &str = "Card should follow this syntax:
<name>
name
<task 1>
task syntax
<task 2>
task syntax
...
";
#[derive(Debug, thiserror::Error)]
pub enum CardParseError {
    #[error("{USAGE}. Card should start with <name> token")]
    NameTokenMissing,
    #[error("{USAGE}. Card should have name")]
    NameMissing,
    #[error("{USAGE}. Card shouldn't be empty")]
    EmptyInput,
    #[error(transparent)]
    TaskParseError(#[from] TaskParseError),
    #[error("{USAGE}. Card should have at least 1 task")]
    NoTasks,
    #[error(
        "{USAGE}. Task token should have <task ID> syntax, where ID is unique(for card) number"
    )]
    IncorrectTaskToken,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Card {
    pub name: String,
    pub tasks: BTreeMap<u16, Task>,
}

impl Card {
    pub fn from_str(
        input: impl AsRef<str>,
        multiline_messages: bool,
    ) -> Result<Self, CardParseError> {
        let input = input.as_ref().trim();
        check!(!input.is_empty(), CardParseError::EmptyInput);
        let mut lines = input.lines().map(|x| x.trim());
        check!(
            lines.next().unwrap() == "<name>",
            CardParseError::NameTokenMissing
        );
        let name = lines.next().ok_or(CardParseError::NameMissing)?.to_owned();

        let mut tasks = BTreeMap::new();

        let mut number = parse_task_token(lines.next().ok_or(CardParseError::IncorrectTaskToken)?)
            .ok_or(CardParseError::IncorrectTaskToken)?
            .ok_or(CardParseError::IncorrectTaskToken)?;
        let mut task_text = String::new();
        for line in lines {
            let new_number = if let Some(nmbr) = parse_task_token(line) {
                Some(nmbr.ok_or(CardParseError::IncorrectTaskToken)?)
            } else {
                None
            };
            match new_number {
                Some(nmbr) => {
                    let prev = tasks.insert(number, Task::from_str(task_text, multiline_messages)?);
                    check!(prev.is_none(), CardParseError::IncorrectTaskToken);
                    number = nmbr;
                    task_text = String::new();
                }
                None => {
                    task_text.push_str(line);
                    task_text.push('\n');
                }
            }
        }
        {
            let prev = tasks.insert(number, Task::from_str(task_text, multiline_messages)?);
            check!(prev.is_none(), CardParseError::IncorrectTaskToken);
        }
        check!(!tasks.is_empty(), CardParseError::NoTasks);
        Ok(Self { name, tasks })
    }
}

/// is this a task token.
/// is this a valid task token.
/// if yes, what line it have.
fn parse_task_token(input: &str) -> Option<Option<u16>> {
    input.strip_prefix("<task ").map(|tail| {
        tail.strip_suffix(">")
            .and_then(|number| number.parse::<u16>().ok())
    })
}
