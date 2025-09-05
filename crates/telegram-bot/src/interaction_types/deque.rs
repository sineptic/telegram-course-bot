use std::collections::BTreeMap;

use course_graph::graph::CourseGraph;
use serde::{
    Deserialize, Serialize,
    de::{Error, Visitor},
};

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

impl Default for Deque {
    fn default() -> Self {
        let deque = from_str(include_str!("../../../../cards.md"), true).unwrap();
        let mut errors = Vec::new();
        CourseGraph::default()
            .cards()
            .keys()
            .filter(|&id| !deque.tasks.contains_key(id))
            .map(|id| format!("Graph has '{id}' card, but deque(cards.md) doesn't."))
            .for_each(|item| {
                errors.push(item);
            });
        deque
            .tasks
            .keys()
            .filter(|x| !CourseGraph::default().cards().contains_key(*x))
            .map(|err| format!("Deque(cards.md) has '{err}', but graph doesn't."))
            .for_each(|item| {
                errors.push(item);
            });
        if !errors.is_empty() {
            panic!(
                "Cards in deque(cards.md) and graph(graph) are different.\n{}",
                errors.join("\n")
            );
        }
        deque
    }
}

impl Serialize for Deque {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.source)
    }
}
struct DequeVisitor;
impl Visitor<'_> for DequeVisitor {
    type Value = Deque;

    fn expecting(&self, _formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        todo!()
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        from_str(v, true).map_err(Error::custom)
    }
}
impl<'de> Deserialize<'de> for Deque {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(DequeVisitor)
    }
}
