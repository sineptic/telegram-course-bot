use std::{collections::HashSet, sync::Arc};

use teloxide::{Bot, prelude::Requester};
use tokio::sync::{Mutex, oneshot};

use super::{Event, EventReceiver};
use crate::{
    interaction_types::{QuestionElement, Task, one_of},
    utils::ResultExt,
};

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    let completed = Arc::new(Mutex::new(HashSet::new()));
    while let Some(event) = rx.recv().await {
        match event {
            Event::StartInteraction(user_id) => {
                if completed.lock().await.contains(&user_id) {
                    log::debug!("user {user_id} already completed");
                    bot.send_message(user_id, "You've already completed the interaction.")
                        .await
                        .log_err();
                    continue;
                }
                let task = Task {
                    question: vec![QuestionElement::Text(
                        "What is the capital of France?".into(),
                    )],
                    options: one_of(["Paris", "London", "Berlin"]),
                    answer: 0,
                };
                let (tx, rx) = oneshot::channel();
                {
                    let bot = bot.clone();
                    let completed = completed.clone();
                    let correct = task.correct_answer().to_owned();
                    tokio::spawn(async move {
                        let result: Vec<String> = rx.await.unwrap();
                        let user_answer = result.last().unwrap().clone();
                        if user_answer == correct {
                            completed.lock().await.insert(user_id);
                            bot.send_message(user_id, "correct").await.log_err();
                            log::debug!("user {user_id} answer correctly");
                        } else {
                            bot.send_message(user_id, "wrong").await.log_err();
                            log::debug!("user {user_id} answer wrong");
                        }
                    });
                }
                crate::handlers::set_task_for_user(bot.clone(), user_id, task.interactions(), tx)
                    .await
                    .log_err();
            }
        }
    }
}
