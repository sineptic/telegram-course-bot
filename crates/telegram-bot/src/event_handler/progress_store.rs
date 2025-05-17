use std::{collections::HashMap, time::SystemTime};

use course_graph::progress_store::{TaskProgress, TaskProgressStore};
use fsrs::FSRS;
use serde::{Deserialize, Serialize};
use ssr_algorithms::fsrs::{level::RepetitionContext, weights::Weights};

type Level = ssr_algorithms::fsrs::level::Level;

type Id = String;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Task {
    progress: TaskProgress,
    level: Level,
}
impl Task {
    fn syncronize(&mut self, fsrs: &FSRS, retrievability_goal: f32, now: SystemTime) {
        let time_to_repeat = self.level.next_repetition(fsrs, retrievability_goal as f64) < now;
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
                if !self.level.failed() {
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
    fn add_repetition(&mut self, repetition: RepetitionContext) -> Result<(), ()> {
        match self.progress {
            TaskProgress::NotStarted {
                could_be_learned: false,
            }
            | TaskProgress::Good => Err(()),
            TaskProgress::Failed
            | TaskProgress::NotStarted {
                could_be_learned: true,
            }
            | TaskProgress::RecursiveFailed => {
                self.level.add_repetition(repetition);
                Ok(())
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserProgress {
    weights: Weights,
    desired_retention: f32,
    tasks: HashMap<Id, Task>,
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
    pub fn syncronize(&mut self) {
        let fsrs = self.weights.fsrs();
        self.tasks.values_mut().for_each(|t| {
            t.syncronize(&fsrs, self.desired_retention, SystemTime::now());
        });
    }
    pub async fn repetition(
        &mut self,
        id: &Id,
        check_knowledge: impl AsyncFnOnce(&Id) -> RepetitionContext,
    ) {
        self.tasks
            .get_mut(id)
            .unwrap()
            .add_repetition(check_knowledge(id).await)
            .expect("HINT: repeated task can't be already good")
    }
    pub async fn revise(
        &mut self,
        check_knowledge: impl AsyncFnOnce(&Id) -> RepetitionContext,
    ) -> Option<()> {
        if let Some((id, task)) = self
            .tasks
            .iter_mut()
            .find(|(_id, task)| task.progress == TaskProgress::Failed)
        {
            task.add_repetition(check_knowledge(id).await).unwrap();
            Some(())
        } else {
            None
        }
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
