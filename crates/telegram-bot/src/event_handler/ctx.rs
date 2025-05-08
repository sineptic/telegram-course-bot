use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
};

use course_graph::{graph::CourseGraph, progress_store::TaskProgress};
use graphviz_rust::dot_structures::Graph;
use rand::{SeedableRng, rngs::StdRng};
use teloxide::Bot;

use crate::{
    interaction_types::{Task, deque},
    utils::Immutable,
};

pub struct BotCtx {
    pub course_graph: Immutable<CourseGraph>,
    pub progress_store: HashMap<String, TaskProgress>,
    base_graph: Graph,
    pub deque: BTreeMap<String, BTreeMap<u16, Task>>,
    pub rng: StdRng,
    bot: Bot,
}

impl BotCtx {
    pub fn load(bot: Bot) -> Self {
        let course_graph = CourseGraph::from_str(&std::fs::read_to_string("graph").unwrap())
            .unwrap_or_else(|err| {
                println!("{err}");
                panic!("graph parsing error");
            });
        let mut progress_store = HashMap::new();
        course_graph.init_store(&mut progress_store);
        let base_graph = course_graph.generate_graph();

        let deque = deque::from_str(&std::fs::read_to_string("cards.md").unwrap(), true).unwrap();
        let rng = StdRng::from_os_rng();

        check_cards_consistency(&progress_store, &deque);

        Self {
            course_graph: course_graph.into(),
            progress_store,
            base_graph,
            deque,
            rng,
            bot,
        }
    }
    pub fn base_graph(&self) -> Graph {
        self.base_graph.clone()
    }
    pub fn bot(&self) -> Bot {
        self.bot.clone()
    }
}

fn check_cards_consistency(
    progress_store: &HashMap<String, TaskProgress>,
    deque: &BTreeMap<String, BTreeMap<u16, Task>>,
) {
    let mut errors = Vec::new();
    progress_store
        .keys()
        .filter(|x| !deque.contains_key(*x))
        .map(|err| format!("Graph has '{err}' card, but deque(cards.md) doesn't."))
        .for_each(|item| {
            errors.push(item);
        });
    deque
        .keys()
        .filter(|x| !progress_store.contains_key(*x))
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
