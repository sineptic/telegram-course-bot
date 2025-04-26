use std::{cell::RefCell, ops::Deref, rc::Rc};

use dot_structures::{Graph, Node, Stmt};
use graphviz_rust::attributes::NodeAttributes;

use crate::card::Card;

#[derive(Clone)]
pub struct Deque {
    pub top_level_cards: Vec<Rc<RefCell<Card>>>,
}
impl Deque {
    pub fn for_each_repeated(&self, callback: &mut impl FnMut(&Rc<RefCell<Card>>)) {
        fn card_for_each(card: &Rc<RefCell<Card>>, callback: &mut impl FnMut(&Rc<RefCell<Card>>)) {
            callback(card);
            card.borrow().dependencies.iter().for_each(|dependencie| {
                card_for_each(dependencie, callback);
            });
        }
        self.top_level_cards.iter().for_each(|x| {
            card_for_each(x, callback);
        });
    }
    pub fn new(top_level_cards: impl IntoIterator<Item = Rc<RefCell<Card>>>) -> Self {
        let top_level_cards = top_level_cards.into_iter().collect::<Vec<_>>();
        top_level_cards.iter().for_each(|card| {
        let name = card.borrow().name.clone();
        if !card.borrow().dependents.is_empty() && Rc::weak_count(card) == 0 {
            let dependents = card
                .borrow()
                .dependents
                .iter()
                .map(|dep| dep.upgrade().unwrap().borrow().name.clone())
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ");
            panic!("Top level card '{name}' shouldn't be dependencie for anything, but it is dependencie for {dependents}")
        }
    });
        Deque {
            top_level_cards: top_level_cards.into_iter().collect(),
        }
    }
    pub fn generate_stmts(&self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        for top_level_card in &self.top_level_cards {
            let id = id_from_string(&top_level_card.borrow().name);
            stmts.push(Stmt::Edge(edge_from_ids(
                id_from_string("Finish"),
                id.clone(),
            )));
            stmts.push(Stmt::Node(Node {
                id: NodeId(id.clone(), None),
                attributes: vec![NodeAttributes::label(top_level_card.borrow().name.clone())],
            }));
        }
        stmts.extend(
            self.top_level_cards
                .iter()
                .map(|x| x.borrow())
                .flat_map(|x| x.deref().generate_stmts()),
        );
        stmts
    }
    pub fn generate_graph(&self) -> Graph {
        Graph::DiGraph {
            id: id_from_string("G"),
            strict: true,
            stmts: self.generate_stmts(),
        }
    }
}

use crate::utils::*;
