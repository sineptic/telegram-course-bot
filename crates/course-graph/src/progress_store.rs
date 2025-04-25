use std::collections::HashMap;

use dot_structures::{Node, Stmt};
use graphviz_rust::attributes::{NodeAttributes, color_name};

use crate::card::Card;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskProgress {
    NotStarted { could_be_learned: bool },
    Good,
    Failed,
    RecursiveFailed,
}

pub trait TaskProgressStore {
    fn get_progress(&self, id: u64) -> TaskProgress;
    fn init(&mut self, id: u64);
    fn update_recursive_failed(&mut self, id: u64);
    fn iter(&self) -> impl Iterator<Item = (u64, TaskProgress)>;
}

impl TaskProgressStore for HashMap<u64, TaskProgress> {
    fn get_progress(&self, id: u64) -> TaskProgress {
        *self
            .get(&id)
            .expect("task progress store should have all tasks before querying")
    }

    fn init(&mut self, id: u64) {
        self.entry(id).or_insert(TaskProgress::NotStarted {
            could_be_learned: true,
        });
    }

    fn update_recursive_failed(&mut self, id: u64) {
        let x = self
            .get_mut(&id)
            .expect("task progress store should have all tasks before querying");
        match x {
            TaskProgress::NotStarted { .. } => {
                *x = TaskProgress::NotStarted {
                    could_be_learned: false,
                }
            }
            TaskProgress::Good => *x = TaskProgress::RecursiveFailed,
            _ => {}
        }
    }

    fn iter(&self) -> impl Iterator<Item = (u64, TaskProgress)> {
        self.iter().map(|(id, progress)| (*id, *progress))
    }
}

fn propagate_fail(card: &Card, store: &mut impl TaskProgressStore) {
    store.update_recursive_failed(card.id);
    card.dependents
        .iter()
        .map(|x| x.upgrade().unwrap())
        .for_each(|x| propagate_fail(&x.borrow(), store));
}

fn detect_recursive_fail(card: &Card, store: &mut impl TaskProgressStore) {
    if let TaskProgress::Failed = store.get_progress(card.id) {
        propagate_fail(card, store);
    } else {
        card.dependencies
            .iter()
            .map(|x| x.borrow())
            .for_each(|x| detect_recursive_fail(&x, store));
    }
}

use crate::{deque::Deque, utils::*};

pub trait TaskProgressStoreExt {
    fn generate_stmts(&self) -> Vec<Stmt>;
    fn detect_recursive_fails(&mut self, deque: &Deque);
}

impl<T> TaskProgressStoreExt for T
where
    T: TaskProgressStore,
{
    fn generate_stmts(&self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        for (id, progress) in self.iter() {
            let color = match progress {
                TaskProgress::Good => color_name::green,
                TaskProgress::Failed => color_name::red,
                TaskProgress::RecursiveFailed => color_name::yellow,
                TaskProgress::NotStarted { .. } => color_name::white,
            };
            stmts.push(Stmt::Node(Node {
                id: NodeId(id_from_id(id), None),
                attributes: vec![
                    NodeAttributes::style("filled".into()),
                    NodeAttributes::fillcolor(color),
                ],
            }));
            if let TaskProgress::NotStarted {
                could_be_learned: true,
            } = progress
            {
                stmts.push(Stmt::Node(Node {
                    id: NodeId(id_from_id(id), None),
                    attributes: vec![NodeAttributes::penwidth(3.)],
                }));
            }
        }
        stmts
    }

    fn detect_recursive_fails(&mut self, deque: &Deque) {
        deque
            .top_level_cards
            .iter()
            .for_each(|card| detect_recursive_fail(&card.borrow(), self));
    }
}
