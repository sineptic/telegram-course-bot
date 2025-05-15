use course_graph::progress_store::TaskProgress;
use ctx::BotCtx;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide::{Bot, prelude::Requester, types::UserId};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::*,
    utils::ResultExt,
};

pub(crate) mod ctx;
mod progress_store;

pub(crate) async fn event_handler(mut ctx: BotCtx, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        handle_event(&mut ctx, event).await;
    }
}

async fn handle_event(ctx: &mut BotCtx, event: Event) {
    match event {
        Event::ReviseCard { user_id, card_name } => {
            let Some(tasks) = ctx.deque.get(&card_name.to_lowercase()) else {
                send_interactions(
                    ctx.bot(),
                    user_id,
                    vec!["Card with this name not found".into()],
                )
                .await
                .log_err();
                return;
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
        } => {
            match progress {
                TaskProgress::Good | TaskProgress::Failed => {
                    if matches!(
                        ctx.progress_store[&card_name],
                        TaskProgress::NotStarted {
                            could_be_learned: false
                        }
                    ) {
                        send_interactions(
                            ctx.bot(),
                            user_id,
                            vec![
                                format!("'{card_name}' dependencies should be good to start learning new.").into(),
                            ],
                        )
                        .await
                        .log_err();
                        return;
                    }
                    ctx.progress_store
                        .repetition(&card_name, |_| RepetitionContext {
                            quality: match progress {
                                TaskProgress::Good => Quality::Good,
                                TaskProgress::Failed => Quality::Again,
                                _ => unreachable!(),
                            },
                            review_time: chrono::Local::now(),
                        });
                }
                _ => unreachable!("should not receive set event with this task progress"),
            };
            ctx.progress_store.syncronize();
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);

            Box::pin(handle_event(ctx, Event::ViewGraph { user_id })).await;
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
    let Ok(result) = rx.await else {
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
