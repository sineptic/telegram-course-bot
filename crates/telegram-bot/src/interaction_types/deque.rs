use std::collections::BTreeMap;

use super::{Card, Task};
use crate::check;

#[derive(Debug, Clone)]
pub struct Deque {
    pub source: String,
    pub tasks: BTreeMap<String, BTreeMap<u16, Task>>,
}

const USAGE: &str = "Deque should follow this syntax:
card syntax
-----
other card
-----
...
";
#[derive(Debug, thiserror::Error)]
pub enum DequeParseError {
    #[error("{USAGE}. Deque should have at lease 1 card")]
    NoCards,
    #[error(transparent)]
    CardParseError(#[from] super::card::CardParseError),
    #[error("{USAGE}. Each card should have unique name")]
    CardNameRepeated,
}

pub fn from_str(input: &str, multiline_messages: bool) -> Result<Deque, DequeParseError> {
    let lines = input.lines().collect::<Vec<_>>();
    let cards_input = lines
        .split(|line| line.starts_with("-----"))
        .map(|input| input.join("\n"));
    let cards = cards_input.map(|x| Card::from_str(x, multiline_messages));
    let mut deque = Deque {
        source: input.to_owned(),
        tasks: BTreeMap::new(),
    };
    for card in cards {
        let Card { name, tasks } = card?;
        let prev = deque.tasks.insert(name.to_lowercase(), tasks);
        check!(prev.is_none(), DequeParseError::CardNameRepeated);
    }
    check!(!deque.tasks.is_empty(), DequeParseError::NoCards);
    Ok(deque)
}
