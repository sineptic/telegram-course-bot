use std::{collections::HashMap, error::Error, time::SystemTime};

use course_graph::progress_store::{TaskProgress, TaskProgressStore};
use fsrs::FSRS;
use ssr_algorithms::fsrs::weights::Weights;
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

#[derive(Default)]
pub struct Task {
    progress: TaskProgress,
    level: Level,
}
impl Task {
    fn syncronize(&mut self, fsrs: &FSRS, retrievability_goal: f32, now: SystemTime) {
        match self.could_be_repeated(fsrs, retrievability_goal, now) {
            true => match self.progress {
                TaskProgress::NotStarted {
                    could_be_learned: _,
                } => (),
                _ => self.progress = TaskProgress::Failed,
            },
            false => match self.progress {
                TaskProgress::Good | TaskProgress::RecursiveFailed => (),
                _ => unreachable!(),
            },
        }
    }
    fn could_be_repeated(&self, fsrs: &FSRS, retrievability_goal: f32, now: SystemTime) -> bool {
        self.level.next_repetition(fsrs, retrievability_goal as f64) < now
    }
    fn should_be_repeated(&self, fsrs: &FSRS, retrievability_goal: f32, now: SystemTime) -> bool {
        self.could_be_repeated(fsrs, retrievability_goal, now)
            && !matches!(
                self.progress,
                TaskProgress::NotStarted {
                    could_be_learned: _
                }
            )
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
    fn is_correct(&self) -> bool {
        matches!(self.progress, TaskProgress::Good)
    }
}

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
    fn should_be_repeated(&mut self) -> impl Iterator<Item = &mut Task> {
        let fsrs = self.weights.fsrs();
        let now = SystemTime::now();
        let desired_retention = self.desired_retention;
        self.tasks
            .values_mut()
            .filter(move |t| t.should_be_repeated(&fsrs, desired_retention, now))
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
