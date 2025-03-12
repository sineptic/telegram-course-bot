use std::{collections::HashSet, sync::Arc};

use telegram_interactions::TelegramInteraction;
use teloxide::{Bot, prelude::Requester};
use tokio::sync::{Mutex, oneshot};

use super::{Event, EventReceiver};
use crate::utils::ResultExt;

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) -> ! {
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
                let interactions = vec![
                    TelegramInteraction::Image("assets/gruvbox-nix.png".into()),
                    TelegramInteraction::Text("2 * 3 = ".into()),
                    TelegramInteraction::OneOf(vec![5.to_string(), 6.to_string(), 7.to_string()]),
                    TelegramInteraction::Text("7 - 5 = ".into()),
                    TelegramInteraction::UserInput,
                ];
                let correct_answer: Vec<String> =
                    vec!["".into(), "".into(), "1".into(), "".into(), "2".into()];
                let (tx, rx) = oneshot::channel();
                {
                    let bot = bot.clone();
                    let completed = completed.clone();
                    tokio::spawn(async move {
                        let result = rx.await.unwrap();
                        if result == correct_answer {
                            completed.lock().await.insert(user_id);
                            bot.send_message(user_id, "correct").await.log_err();
                            log::debug!("user {user_id} answer correctly");
                        } else {
                            bot.send_message(user_id, "wrong").await.log_err();
                            log::debug!("user {user_id} answer wrong");
                        }
                    });
                }
                crate::handlers::set_task_for_user(bot.clone(), user_id, interactions, tx)
                    .await
                    .log_err();
            }
        }
    }
    unreachable!()
}
