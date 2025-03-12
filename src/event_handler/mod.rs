use teloxide::{Bot, prelude::Requester};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    interaction_types::{QuestionElement, Task, one_of},
    utils::ResultExt,
};

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        match event {
            Event::StartInteraction(user_id) => {
                let task = Task {
                    question: vec![
                        QuestionElement::Image("assets/france-paris.jpg".into()),
                        QuestionElement::Text("What is the capital of France?".into()),
                    ],
                    options: one_of(["Paris", "London", "Berlin"]),
                    answer: 0,
                };
                let (tx, rx) = oneshot::channel();
                {
                    let bot = bot.clone();
                    let correct = task.correct_answer().to_owned();
                    tokio::spawn(async move {
                        let result: Vec<String> = rx.await.unwrap();
                        let user_answer = result.last().unwrap().clone();
                        if user_answer == correct {
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
