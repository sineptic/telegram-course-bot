use std::error::Error;

use chrono::{DateTime, Local};
use course_graph::progress_store::TaskProgress;
use ctx::BotCtx;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide::{Bot, prelude::Requester, types::UserId};
use tokio::sync::oneshot;

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::{telegram_interaction::QuestionElement, *},
    utils::ResultExt,
};

pub(crate) mod ctx;
mod progress_store;

pub(crate) async fn event_handler(mut ctx: BotCtx, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        handle_event(&mut ctx, event).await;
    }
}

async fn get_user_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    answers: Vec<String>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    set_task_for_user(
        bot,
        user_id,
        interactions
            .into_iter()
            .map(|x| x.into())
            .chain([TelegramInteraction::OneOf(answers)])
            .collect(),
        tx,
    )
    .await?;
    let Some(answer) = rx.await.map(|mut x| x.pop().unwrap()).ok() else {
        return Ok(None);
    };
    Ok(Some(answer))
}

async fn get_card_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    answers: Vec<String>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    // TODO: add 'I dont know' option
    get_user_answer(bot, user_id, interactions, answers).await
}

fn now(start: DateTime<Local>) -> DateTime<Local> {
    let now = Local::now();
    let diff = now - start;
    start + diff * 3600 * 24
}

async fn handle_event(ctx: &mut BotCtx, event: Event) {
    let start_time = ctx.start;
    match event {
        Event::ReviseCard { user_id, card_name } => {
            ctx.progress_store.syncronize(now(start_time).into());
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);
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
            let rcx = handle_revise(
                &card_name,
                ctx.bot(),
                user_id,
                start_time,
                &ctx.deque,
                &mut ctx.rng,
            )
            .await;
            ctx.progress_store
                .repetition(&card_name, async |_| rcx)
                .await;
        }

        Event::ViewGraph { user_id } => {
            ctx.progress_store.syncronize(now(start_time).into());
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);
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
            ctx.progress_store.syncronize(now(start_time).into());
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);
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
                        .repetition(&card_name, async |_| RepetitionContext {
                            quality: match progress {
                                TaskProgress::Good => Quality::Good,
                                TaskProgress::Failed => Quality::Again,
                                _ => unreachable!(),
                            },
                            review_time: now(start_time),
                        })
                        .await;
                }
                _ => unreachable!("should not receive set event with this task progress"),
            };
            ctx.progress_store.syncronize(now(start_time).into());
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);

            Box::pin(handle_event(ctx, Event::ViewGraph { user_id })).await;
        }
        Event::Revise { user_id } => {
            ctx.progress_store.syncronize(now(start_time).into());
            ctx.course_graph
                .detect_recursive_fails(&mut ctx.progress_store);
            let bot = ctx.bot();
            let a = ctx
                .progress_store
                .revise(async |id| {
                    handle_revise(
                        id,
                        bot.clone(),
                        user_id,
                        start_time,
                        &ctx.deque,
                        &mut ctx.rng,
                    )
                    .await
                })
                .await;
            if a.is_none() {
                bot.send_message(user_id, "You don't have card to revise.")
                    .await
                    .log_err();
            }
        }
    }
}
async fn handle_revise(
    id: &String,
    bot: Bot,
    user_id: UserId,
    start_time: DateTime<Local>,
    ctx_deque: &std::collections::BTreeMap<String, std::collections::BTreeMap<u16, Task>>,
    ctx_rng: &mut rand::prelude::StdRng,
) -> RepetitionContext {
    let Task {
        question,
        options,
        answer,
        explanation,
    } = card::random_task(&ctx_deque[id], ctx_rng);
    let mut correct = false;
    if let Some(user_answer) =
        get_card_answer(bot.clone(), user_id, question.clone(), options.clone())
            .await
            .unwrap()
    {
        if user_answer == options[*answer] {
            correct = true;
            bot.send_message(user_id, "Correct!").await.log_err();
        }
    }
    if !correct {
        bot.send_message(user_id, format!("Wrong. Answer is {}", options[*answer]))
            .await
            .log_err();
        if let Some(explanation) = explanation {
            send_interactions(
                bot.clone(),
                user_id,
                explanation
                    .iter()
                    .map(|x| x.clone().into())
                    .collect::<Vec<TelegramInteraction>>(),
            )
            .await
            .log_err();
        }
    }
    let quality = if correct {
        Quality::Good
    } else {
        Quality::Again
    };
    RepetitionContext {
        quality,
        review_time: now(start_time),
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
