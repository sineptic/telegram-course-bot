use course_graph::progress_store::{TaskProgress, TaskProgressStore};
use ctx::BotCtx;
use teloxide::{Bot, prelude::Requester, types::UserId};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::*,
    utils::ResultExt,
};

pub(crate) mod ctx;

pub(crate) async fn event_handler(mut ctx: BotCtx, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        match event {
            Event::ReviseCard { user_id, card_name } => {
                let Some(tasks) = ctx.deque.get(&card_name.to_lowercase()) else {
                    send_interactions(
                        ctx.bot(),
                        user_id,
                        vec![TelegramInteraction::Text(
                            "Card with this name not found".into(),
                        )],
                    )
                    .await
                    .log_err();
                    continue;
                };
                let task = card::random_task(tasks, &mut ctx.rng).clone();
                let (tx, rx) = oneshot::channel();
                tokio::spawn(event_end_handler(
                    ctx.bot(),
                    rx,
                    user_id,
                    task.correct_answer().to_owned(),
                    task.explanation.clone(),
                ));
                set_task_for_user(ctx.bot(), user_id, task.interactions(), tx)
                    .await
                    .log_err();
            }

            Event::ViewGraph { user_id } => {
                let graph_image =
                    course_graph::generate_graph_chart(ctx.base_graph(), &ctx.progress_store);
                send_interactions(
                    ctx.bot(),
                    user_id,
                    vec![TelegramInteraction::PersonalImage(graph_image)],
                )
                .await
                .log_err();
            }

            Event::SetCardProgress {
                user_id,
                card_name,
                progress,
            } => match progress {
                TaskProgress::Good | TaskProgress::Failed => {
                    let Some(card_node) = ctx.course_graph.cards().get(&card_name) else {
                        send_interactions(
                            ctx.bot(),
                            user_id,
                            vec![TelegramInteraction::Text(format!(
                                "There is no '{card_name}' card."
                            ))],
                        )
                        .await
                        .log_err();
                        continue;
                    };
                    if card_node.dependencies.iter().any(|dependencie| {
                        matches!(
                            ctx.progress_store.get_progress(dependencie),
                            TaskProgress::NotStarted {
                                could_be_learned: _
                            }
                        )
                    }) {
                        send_interactions(
                            ctx.bot(),
                            user_id,
                            vec![TelegramInteraction::Text(format!(
                                "All '{card_name}' dependencies should be started."
                            ))],
                        )
                        .await
                        .log_err();
                        continue;
                    }
                    *ctx.progress_store.get_mut(&card_name).unwrap() = progress;
                    ctx.course_graph
                        .detect_recursive_fails(&mut ctx.progress_store);
                }
                TaskProgress::NotStarted {
                    could_be_learned: true,
                } => {}
                _ => unreachable!("should not receive set event with this task progress"),
            },
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
