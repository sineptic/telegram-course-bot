use std::{
    error::Error,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use chrono::{DateTime, Local};
use course_graph::progress_store::TaskProgress;
use ctx::BotCtx;
use progress_store::UserProgress;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide::{Bot, prelude::Requester, types::UserId};
use tokio::sync::Mutex;

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::{telegram_interaction::QuestionElement, *},
    utils::ResultExt,
};

pub(crate) mod ctx;
mod progress_store;

type Ctx = Arc<Mutex<BotCtx>>;

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

pub(crate) async fn event_handler(ctx: BotCtx, bot: Bot, mut rx: EventReceiver) {
    let start = chrono::Local::now();
    let ctx = Arc::new(Mutex::new(ctx));
    while let Some(event) = rx.recv().await {
        handle_event(start, bot.clone(), ctx.clone(), event).await;
    }
}

async fn handle_event(start_time: DateTime<Local>, bot: Bot, ctx: Ctx, event: Event) {
    match event {
        Event::ReviseCard { user_id, card_name } => {
            syncronize(start_time, ctx.clone()).await;

            if let Some(rcx) =
                handle_revise(&card_name, bot, user_id, start_time, ctx.clone()).await
            {
                let mut ctx = ctx.lock().await;
                let ctx = ctx.deref_mut();
                let mut progress_store = ctx.progress_store.lock().await;
                let progress_store = progress_store.deref_mut();
                progress_store.repetition(&card_name, rcx);
            }
        }

        Event::ViewGraph { user_id } => {
            syncronize(start_time, ctx.clone()).await;
            let graph = {
                let mut ctx = ctx.lock().await;
                let ctx = ctx.deref_mut();
                course_graph::generate_graph(
                    ctx.base_graph(),
                    ctx.progress_store.lock().await.deref(),
                )
            };

            tokio::spawn(async move {
                let graph_image =
                    tokio::task::spawn_blocking(move || course_graph::print_graph(graph))
                        .await
                        .log_err()
                        .unwrap();
                send_interactions(
                    bot,
                    user_id,
                    vec![TelegramInteraction::PersonalImage(graph_image)],
                )
                .await
                .log_err();
            });
        }
        Event::Revise { user_id } => {
            // FIXME: 2 users can't revise at the same time
            syncronize(start_time, ctx.clone()).await;
            let progress_store = ctx.lock().await.progress_store.clone();
            let a = progress_store
                .lock()
                .await
                .revise(async |id| {
                    handle_revise(id, bot.clone(), user_id, start_time, ctx)
                        .await
                        .unwrap()
                })
                .await;
            if a.is_none() {
                bot.send_message(user_id, "You don't have card to revise.")
                    .await
                    .log_err();
            }
        }
        Event::Clear { user_id } => {
            let mut ctx = ctx.lock().await;
            let ctx = ctx.deref_mut();
            let mut progress_store = ctx.progress_store.lock().await;
            let progress_store = progress_store.deref_mut();
            *progress_store = UserProgress::default();
            ctx.course_graph.init_store(progress_store);

            tokio::spawn(async move {
                send_interactions(bot, user_id, vec!["Progress cleared.".into()])
                    .await
                    .log_err();
            });
        }
    }
}

async fn syncronize(start_time: DateTime<Local>, ctx: Arc<Mutex<BotCtx>>) {
    let mut ctx = ctx.lock().await;
    let ctx = ctx.deref_mut();
    let mut progress_store = ctx.progress_store.lock().await;
    let progress_store = progress_store.deref_mut();
    progress_store.syncronize(now(start_time).into());
    ctx.course_graph.detect_recursive_fails(progress_store);
}

async fn handle_revise(
    id: &String,
    bot: Bot,
    user_id: UserId,
    start_time: DateTime<Local>,
    ctx: Ctx,
) -> Option<RepetitionContext> {
    let Task {
        question,
        options,
        answer,
        explanation,
    } = {
        let mut ctx = ctx.lock().await;
        let ctx = ctx.deref_mut();
        let progress_store = ctx.progress_store.lock().await;
        if matches!(
            progress_store[id],
            TaskProgress::NotStarted {
                could_be_learned: false
            }
        ) {
            send_interactions(
                bot.clone(),
                user_id,
                vec!["You should learn all dependencies before starting new card.".into()],
            )
            .await
            .log_err();
            return None;
        }
        card::random_task(
            {
                if let Some(x) = ctx.deque.get(id) {
                    x
                } else {
                    send_interactions(bot, user_id, vec!["Card with this name not found".into()])
                        .await
                        .log_err();
                    return None;
                }
            },
            &mut ctx.rng,
        )
        .clone()
    };

    let mut correct = false;
    if let Some(user_answer) =
        get_card_answer(bot.clone(), user_id, question.clone(), options.clone())
            .await
            .unwrap()
        && user_answer == options[answer]
    {
        correct = true;
        bot.send_message(user_id, "Correct!").await.log_err();
    }
    if !correct {
        bot.send_message(user_id, format!("Wrong. Answer is {}", options[answer]))
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
    Some(RepetitionContext {
        quality,
        review_time: now(start_time),
    })
}
