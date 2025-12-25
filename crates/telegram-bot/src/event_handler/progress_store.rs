use std::{collections::HashMap, time::SystemTime};

use course_graph::progress_store::{TaskProgress, TaskProgressStore};
use fsrs::FSRS;
use serde::{Deserialize, Serialize};
use ssr_algorithms::fsrs::{level::RepetitionContext, weights::Weights};

type Level = ssr_algorithms::fsrs::level::Level;

type Id = String;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    progress: TaskProgress,
    level: Level,
    pub(crate) meaningful_repetitions: u32,
}
impl Task {
    fn synchronize(&mut self, fsrs: &FSRS, retrievability_goal: f32, now: SystemTime) {
        let next_repetition = self.level.next_repetition(fsrs, retrievability_goal as f64);
        let time_to_repeat = next_repetition < now;
        match self.progress {
            TaskProgress::NotStarted {
                could_be_learned: false,
            } => assert!(time_to_repeat),
            TaskProgress::Good | TaskProgress::RecursiveFailed => {
                if time_to_repeat {
                    self.progress = TaskProgress::Failed
                }
            }
            TaskProgress::Failed
            | TaskProgress::NotStarted {
                could_be_learned: true,
            } => {
                if !self.level.failed() && !time_to_repeat {
                    self.progress = TaskProgress::Good
                }
            }
        }
    }
    fn update_parents_info(&mut self, is_all_parents_correct: bool) {
        match self.progress {
            TaskProgress::NotStarted {
                could_be_learned: _,
            } => {
                self.progress = TaskProgress::NotStarted {
                    could_be_learned: is_all_parents_correct,
                };
            }
            TaskProgress::Good => {
                if !is_all_parents_correct {
                    self.progress = TaskProgress::RecursiveFailed;
                }
            }
            TaskProgress::Failed => (),
            TaskProgress::RecursiveFailed => {
                if is_all_parents_correct {
                    self.progress = TaskProgress::Good;
                }
            }
        }
    }
    fn add_repetition(
        &mut self,
        repetition: RepetitionContext,
        meaningful_repetition: bool,
    ) -> Result<(), ()> {
        match self.progress {
            TaskProgress::NotStarted {
                could_be_learned: false,
            } => Err(()),
            _ => {
                self.level.add_repetition(repetition);
                if meaningful_repetition {
                    self.meaningful_repetitions += 1;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProgress {
    weights: Weights,
    desired_retention: f32,
    pub(crate) tasks: HashMap<Id, Task>,
}
impl Default for UserProgress {
    fn default() -> Self {
        Self {
            weights: Weights::default(),
            desired_retention: 0.85,
            tasks: HashMap::new(),
        }
    }
}
impl UserProgress {
    pub fn synchronize(&mut self, now: SystemTime) {
        let fsrs = self.weights.fsrs();
        self.tasks.values_mut().for_each(|t| {
            t.synchronize(&fsrs, self.desired_retention, now);
        });
    }
    pub fn repetition(
        &mut self,
        id: &Id,
        repetition: RepetitionContext,
        meaningful_repetition: bool,
    ) {
        self.tasks
            .get_mut(id)
            .unwrap()
            .add_repetition(repetition, meaningful_repetition)
            .expect("HINT: you cant revice card that not started and have bad known(for user) dependencies")
    }
}
impl<'a> std::ops::Index<&'a Id> for UserProgress {
    type Output = TaskProgress;

    fn index(&self, index: &'a Id) -> &Self::Output {
        &self.tasks[index].progress
    }
}
impl TaskProgressStore for UserProgress {
    type Id = Id;

    fn init(&mut self, id: &Self::Id) {
        if let Some(_prev) = self.tasks.insert(id.to_owned(), Task::default()) {
            panic!("each task should be initialized once, but {id} doesn't.");
        }
    }
    fn contains(&self, id: &Self::Id) -> bool {
        self.tasks.contains_key(id)
    }

    fn update_recursive_failed(&mut self, id: &Self::Id) {
        self.tasks.get_mut(id).unwrap().update_parents_info(false);
    }

    fn update_no_recursive_failed(&mut self, id: &Self::Id) {
        self.tasks.get_mut(id).unwrap().update_parents_info(true);
    }

    fn iter(&self) -> impl Iterator<Item = (&Self::Id, TaskProgress)> {
        self.tasks.iter().map(|(id, t)| (id, t.progress))
    }
}
