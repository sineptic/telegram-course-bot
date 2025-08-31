use std::{str::FromStr, sync::LazyLock};

use course_graph::graph::CourseGraph;

use super::progress_store::UserProgress;
use crate::{
    interaction_types::deque::{self, Deque},
    utils::Immutable,
};

static DEFAULT_COURSE_GRAPH: LazyLock<Immutable<CourseGraph>> = LazyLock::new(|| {
    CourseGraph::from_str(include_str!("../../../../graph"))
        .unwrap_or_else(|err| {
            println!("{err}");
            panic!("graph parsing error");
        })
        .into()
});

static DEFAULT_DEQUE: LazyLock<Immutable<Deque>> = LazyLock::new(|| {
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

#[derive(Default, Debug)]
pub struct Course {
    course_graph: Option<Immutable<CourseGraph>>,
    deque: Option<Immutable<Deque>>,
    has_errors: bool,
}
impl Course {
    pub fn get_course_graph(&self) -> &CourseGraph {
        if let Some(ref course_graph) = self.course_graph
            && !self.has_errors
        {
            course_graph
        } else {
            &DEFAULT_COURSE_GRAPH
        }
    }
    pub fn get_deque(&self) -> &Deque {
        if let Some(ref deque) = self.deque
            && !self.has_errors
        {
            deque
        } else {
            &DEFAULT_DEQUE
        }
    }
    pub fn get_errors(&self) -> Option<Vec<String>> {
        let deque = self.get_deque();
        let course_graph = self.get_course_graph();
        let mut errors = Vec::new();

        course_graph
            .cards()
            .keys()
            .filter(|&id| !deque.tasks.contains_key(id))
            .map(|id| format!("Graph has '{id}' card, but deque(cards.md) doesn't."))
            .for_each(|item| errors.push(item));
        deque
            .tasks
            .keys()
            .filter(|x| !DEFAULT_COURSE_GRAPH.cards().contains_key(*x))
            .map(|err| format!("Deque(cards.md) has '{err}', but graph doesn't."))
            .for_each(|item| {
                errors.push(item);
            });

        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }
    pub fn set_course_graph(&mut self, course_graph: CourseGraph) {
        self.course_graph = Some(course_graph.into());
        self.has_errors = self.get_errors().is_some();
    }
    pub fn set_deque(&mut self, deque: Deque) {
        self.deque = Some(deque.into());
        self.has_errors = self.get_errors().is_some();
    }
    pub fn default_user_progress(&self) -> UserProgress {
        let mut user_progress = UserProgress::default();
        self.get_course_graph().init_store(&mut user_progress);
        user_progress
    }
}
