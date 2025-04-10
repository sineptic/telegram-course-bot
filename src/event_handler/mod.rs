use teloxide::{Bot, prelude::Requester};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    handlers::set_task_for_user,
    interaction_types::{Task, TelegramInteraction},
    utils::ResultExt,
};

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    let task = Task::from_str(
        std::fs::read_to_string("tasks/france_capital.md").unwrap(),
        true,
    )
    .unwrap();
    while let Some(event) = rx.recv().await {
        match event {
            Event::StartInteraction(user_id) => {
                let (tx, rx) = oneshot::channel();
                {
                    let bot = bot.clone();
                    let correct = task.correct_answer().to_owned();
                    let explanation = task.explanation.clone();
                    tokio::spawn(async move {
                        let result: Vec<String> = rx.await.unwrap();
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
