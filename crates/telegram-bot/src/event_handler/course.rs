use std::{str::FromStr, sync::LazyLock};

use course_graph::graph::CourseGraph;

use crate::{
    interaction_types::deque::{self, Deque},
    utils::Immutable,
};

pub static DEFAULT_COURSE_GRAPH: LazyLock<Immutable<CourseGraph>> = LazyLock::new(|| {
    CourseGraph::from_str(include_str!("../../../../graph"))
        .unwrap_or_else(|err| {
            println!("{err}");
            panic!("graph parsing error");
        })
        .into()
});

pub static DEFAULT_DEQUE: LazyLock<Immutable<Deque>> = LazyLock::new(|| {
    let deque = deque::from_str(include_str!("../../../../cards.md"), true).unwrap();
    let mut errors = Vec::new();
    DEFAULT_COURSE_GRAPH
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
        .filter(|x| !DEFAULT_COURSE_GRAPH.cards().contains_key(*x))
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
    deque.into()
});
