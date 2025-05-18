use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use course_graph::{graph::CourseGraph, progress_store::TaskProgressStore};
use graphviz_rust::dot_structures::Graph;
use rand::{SeedableRng, rngs::StdRng};
use tokio::sync::Mutex;

use super::progress_store::UserProgress;
use crate::{
    interaction_types::{Task, deque},
    utils::Immutable,
};

pub struct BotCtx {
    pub course_graph: Immutable<CourseGraph>,
    pub progress_store: Arc<Mutex<UserProgress>>,
    base_graph: Graph,
    pub deque: BTreeMap<String, BTreeMap<u16, Task>>,
    pub rng: StdRng,
}

impl BotCtx {
    pub fn load() -> Self {
        let course_graph = CourseGraph::from_str(&std::fs::read_to_string("graph").unwrap())
            .unwrap_or_else(|err| {
                println!("{err}");
                panic!("graph parsing error");
            });
        let mut progress_store = UserProgress::default();
        course_graph.init_store(&mut progress_store);
        let base_graph = course_graph.generate_graph();

        let deque = deque::from_str(&std::fs::read_to_string("cards.md").unwrap(), true).unwrap();
        let rng = StdRng::from_os_rng();

        check_cards_consistency(&progress_store, &deque);
        course_graph.detect_recursive_fails(&mut progress_store);

        Self {
            course_graph: course_graph.into(),
            progress_store: Arc::new(Mutex::new(progress_store)),
            base_graph,
            deque,
            rng,
        }
    }
    pub fn base_graph(&self) -> Graph {
        self.base_graph.clone()
    }
}

fn check_cards_consistency<S>(progress_store: &S, deque: &BTreeMap<String, BTreeMap<u16, Task>>)
where
    S: TaskProgressStore<Id = String>,
{
    let mut errors = Vec::new();
    progress_store
        .iter()
        .map(|(id, _)| id)
        .filter(|&id| !deque.contains_key(id))
        .map(|id| format!("Graph has '{id}' card, but deque(cards.md) doesn't."))
        .for_each(|item| {
            errors.push(item);
        });
    deque
        .keys()
        .filter(|x| !progress_store.contains(*x))
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
}
