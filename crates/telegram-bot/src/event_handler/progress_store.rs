use std::{collections::HashMap, error::Error, time::SystemTime};

use course_graph::progress_store::{TaskProgress, TaskProgressStore};
use fsrs::FSRS;
use ssr_algorithms::fsrs::{level::RepetitionContext, weights::Weights};
use teloxide::{Bot, types::UserId};

use crate::{handlers::set_task_for_user, interaction_types::TelegramInteraction};

type Response = Vec<String>;
type Level = ssr_algorithms::fsrs::level::Level;

async fn get_user_answer(
    bot: Bot,
    user_id: UserId,
    interactions: Vec<TelegramInteraction>,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    set_task_for_user(bot, user_id, interactions, tx).await?;
    Ok(rx.await.unwrap())
}

type Id = String;

#[derive(Default, Debug)]
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

#[derive(Debug)]
pub struct UserProgress {
    weights: Weights,
    desired_retention: f32,
    tasks: HashMap<Id, Task>,
}
impl Default for UserProgress {
    fn default() -> Self {
        Self {
            weights: Weights::default(),
            desired_retention: 0.99999,
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
    pub fn repetition(&mut self, id: &Id, check_knowledge: impl FnOnce(&Id) -> RepetitionContext) {
        self.tasks
            .get_mut(id)
            .unwrap()
            .add_repetition(check_knowledge(id))
            .expect("HINT: repeated task can't be already good")
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
