use std::{collections::HashMap, str::FromStr};

use dot_structures::{Graph, Node, Stmt};
use graphviz_rust::attributes::NodeAttributes;

use crate::card::CardNode;

#[derive(Clone, Debug)]
#[allow(clippy::manual_non_exhaustive)]
pub struct CourseGraph {
    pub(crate) text: String,
    pub(crate) cards: HashMap<String, CardNode>,
}
impl CourseGraph {
    pub fn init_store(&self, store: &mut impl TaskProgressStore<Id = String>) {
        self.cards.keys().for_each(|id| {
            store.init(id);
        });
    }
    fn generate_card_stmts(&self, name: &String) -> impl Iterator<Item = Stmt> {
        self.cards[name]
            .dependencies
            .iter()
            .flat_map(|dependency| generate_edge_stmts(name, dependency))
    }
    pub fn generate_stmts(&self) -> impl Iterator<Item = Stmt> {
        self.cards
            .keys()
            .flat_map(|name| self.generate_card_stmts(name))
            .chain(
                self.cards
                    .iter()
                    .filter(|(_, card)| card.dependents.is_empty())
                    .map(|(x, _)| x)
                    .flat_map(|top_level_dependencie| {
                        generate_edge_stmts("Finish", top_level_dependencie)
                    }),
            )
    }
    pub fn generate_structure_graph(&self) -> Graph {
        Graph::Graph {
            id: id_from_string("G"),
            strict: true,
            stmts: self.generate_stmts().collect(),
        }
    }
    pub fn cards(&self) -> &HashMap<String, CardNode> {
        &self.cards
    }
    pub fn get_source(&self) -> &str {
        &self.text
    }
}

fn generate_edge_stmts(first: &str, second: &str) -> impl Iterator<Item = Stmt> {
    [
        node_stmt(first),
        node_stmt(second),
        edge_stmt_from_strings(first, second),
    ]
    .into_iter()
}

fn node_stmt(name: &str) -> Stmt {
    Stmt::Node(Node {
        id: NodeId(id_from_string(name), None),
        attributes: vec![NodeAttributes::label(name.to_owned())],
    })
}

use crate::{
    progress_store::{TaskProgress, TaskProgressStore},
    utils::*,
};

impl CourseGraph {
    fn propagate_fail(&self, name: &String, store: &mut impl TaskProgressStore<Id = String>) {
        store.update_recursive_failed(name);
        self.cards[name]
            .dependents
            .iter()
            .for_each(|x| self.propagate_fail(x, store));
    }

    fn propagate_no_fail(&self, name: &String, store: &mut impl TaskProgressStore<Id = String>) {
        if self.cards[name]
            .dependencies
            .iter()
            .any(|x| store[x] != TaskProgress::Good)
        {
            return;
        }
        store.update_no_recursive_failed(name);
        self.cards[name]
            .dependents
            .iter()
            .for_each(|x| self.propagate_no_fail(x, store));
    }

    pub fn detect_recursive_fails(&self, store: &mut impl TaskProgressStore<Id = String>) {
        self.cards.keys().for_each(|name| {
            if store[name] == TaskProgress::Failed {
                self.propagate_fail(name, store);
            }
        });
        self.cards.keys().for_each(|name| {
            self.propagate_no_fail(name, store);
        });
    }
}

impl Default for CourseGraph {
    fn default() -> Self {
        CourseGraph::from_str(include_str!("../../../graph")).unwrap_or_else(|err| {
            println!("{err}");
            panic!("graph parsing error");
        })
    }
}
