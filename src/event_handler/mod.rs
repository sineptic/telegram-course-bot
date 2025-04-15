use rand::{Rng, SeedableRng};
use teloxide::{Bot, prelude::Requester};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{handlers::set_task_for_user, interaction_types::*, utils::ResultExt};

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    let card = Card::from_str(std::fs::read_to_string("tasks.md").unwrap(), true).unwrap();
    let mut rng = rand::rngs::StdRng::from_os_rng();
    while let Some(event) = rx.recv().await {
        match event {
            Event::StartInteraction(user_id) => {
                let len = card.tasks.len();
                let task = card
                    .tasks
                    .values()
                    .nth(rng.random_range(0..len))
                    .unwrap()
                    .clone();
                let (tx, rx) = oneshot::channel();
                {
                    let bot = bot.clone();
                    let correct = task.correct_answer().to_owned();
                    let explanation = task.explanation.clone();
                    tokio::spawn(async move {
                        // FIXME
                        let Ok(result): Result<Vec<String>, _> = rx.await else {
                            log::warn!("todo: handle user input cancellation");
                            return;
                        };
                        let user_answer = result.last().unwrap().clone();
                        if user_answer == correct {
                            bot.send_message(user_id, "Correct!").await.log_err();
                            log::debug!("user {user_id} answer correctly");
                        } else {
                            bot.send_message(user_id, format!("Wrong. Answer is {correct}"))
                                .await
                                .log_err();
                            if let Some(explanation) = explanation {
                                let messages = explanation
                                    .into_iter()
                                    .map(|x| x.into())
                                    .collect::<Vec<TelegramInteraction>>();
                                let (tx, rx) = oneshot::channel();
                                set_task_for_user(bot, user_id, messages, tx)
                                    .await
                                    .log_err();
                                rx.await.unwrap();
                            }
                            log::debug!("user {user_id} answer wrong");
                        }
                    });
                }
                set_task_for_user(bot.clone(), user_id, task.interactions(), tx)
                    .await
                    .log_err();
            }
        }
    }
}
