use std::collections::HashMap;

use dot_structures::{Node, Stmt};
use graphviz_rust::attributes::{NodeAttributes, color_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskProgress {
    NotStarted { could_be_learned: bool },
    Good,
    Failed,
    RecursiveFailed,
}

pub trait TaskProgressStore {
    type Id: PartialEq;
    fn get_progress(&self, id: &Self::Id) -> TaskProgress;
    fn init(&mut self, id: &Self::Id);
    fn update_recursive_failed(&mut self, id: &Self::Id);
    fn iter(&self) -> impl Iterator<Item = (&Self::Id, TaskProgress)>;
}

impl TaskProgressStore for HashMap<String, TaskProgress> {
    type Id = String;
    fn get_progress(&self, id: &Self::Id) -> TaskProgress {
        *self
            .get(id)
            .expect("task progress store should have all tasks before querying")
    }

    fn init(&mut self, id: &Self::Id) {
        self.entry(id.clone()).or_insert(TaskProgress::NotStarted {
            could_be_learned: true,
        });
    }

    fn update_recursive_failed(&mut self, id: &Self::Id) {
        let x = self
            .get_mut(id)
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

    fn iter(&self) -> impl Iterator<Item = (&Self::Id, TaskProgress)> {
        self.iter().map(|(id, progress)| (id, *progress))
    }
}

use crate::utils::*;

pub trait TaskProgressStoreExt {
    fn generate_stmts(&self) -> Vec<Stmt>;
}

impl<T> TaskProgressStoreExt for T
where
    T: TaskProgressStore<Id = String>,
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
                id: NodeId(id_from_string(id), None),
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
                    id: NodeId(id_from_string(id), None),
                    attributes: vec![NodeAttributes::penwidth(3.)],
                }));
            }
        }
        stmts
    }
}
