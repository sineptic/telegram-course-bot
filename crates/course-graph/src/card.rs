use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    str::FromStr,
};

pub struct Card {
    pub name: String,
    pub id: u64,
    pub dependencies: Vec<Rc<RefCell<Card>>>,
    pub dependents: Vec<Weak<RefCell<Card>>>,
}

impl Card {
    // FIXME: check for cycles
    pub fn new(
        name: impl ToString,
        id: u64,
        dependencies: impl IntoIterator<Item = Rc<RefCell<Card>>>,
    ) -> Rc<RefCell<Card>> {
        let name = name.to_string();
        assert!(!name.contains('"'));
        let card = Rc::new(RefCell::new(Card {
            name,
            id,
            dependencies: Vec::new(),
            dependents: Vec::new(),
        }));
        dependencies.into_iter().for_each(|dep| {
            dep.borrow_mut().dependents.push(Rc::downgrade(&card));
            card.borrow_mut().dependencies.push(dep);
        });
        card
    }
    pub fn generate_stmts(&self) -> Vec<Stmt> {
        use graphviz_rust::attributes::NodeAttributes;

        let mut stmts = Vec::new();
        for dependency in &self.dependencies {
            let id1 = id_from_id(self.id);
            let id2 = id_from_id(dependency.borrow().id);
            stmts.push(Stmt::Node(Node {
                id: NodeId(id1.clone(), None),
                attributes: vec![NodeAttributes::label(self.name.clone())],
            }));
            stmts.push(Stmt::Node(Node {
                id: NodeId(id2.clone(), None),
                attributes: vec![NodeAttributes::label(dependency.borrow().name.clone())],
            }));
            stmts.push(Stmt::Edge(edge_from_ids(id1, id2)));
            stmts.extend(dependency.borrow().generate_stmts());
        }
        stmts
    }
}

use crate::utils::*;

impl FromStr for Card {
    type Err = anyhow::Error;
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        todo!("use chumsky for parsing")
    }
}
