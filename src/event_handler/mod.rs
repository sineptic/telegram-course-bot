use rand::SeedableRng;
use teloxide::{Bot, prelude::Requester, types::UserId};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::*,
    utils::ResultExt,
};

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    let deque = deque::from_str(&std::fs::read_to_string("cards.md").unwrap(), true).unwrap();
    let mut rng = rand::rngs::StdRng::from_os_rng();

    while let Some(event) = rx.recv().await {
        match event {
            Event::ReviseCard { user_id, card_name } => {
                let Some(tasks) = deque.get(&card_name.to_lowercase()) else {
                    send_interactions(
                        bot.clone(),
                        user_id,
                        vec![TelegramInteraction::Text(
                            "Card with this name not found".into(),
                        )],
                    )
                    .await
                    .log_err();
                    continue;
                };
                let task = card::random_task(tasks, &mut rng).clone();
                let (tx, rx) = oneshot::channel();
                tokio::spawn(event_end_handler(
                    bot.clone(),
                    rx,
                    user_id,
                    task.correct_answer().to_owned(),
                    task.explanation.clone(),
                ));
                set_task_for_user(bot.clone(), user_id, task.interactions(), tx)
                    .await
                    .log_err();
            }

            Event::ListCards { user_id } => {
                let names = deque
                    .keys()
                    .map(|x| format!("- {x}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let message = vec![TelegramInteraction::Text(format!("card names:\n{names}"))];
                send_interactions(bot.clone(), user_id, message)
                    .await
                    .log_err();
            }
        }
    }
}

async fn event_end_handler(
    bot: Bot,
    rx: oneshot::Receiver<Vec<String>>,
    user_id: UserId,
    correct: String,
    explanation: Option<Vec<telegram_interaction::QuestionElement>>,
) {
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
}
