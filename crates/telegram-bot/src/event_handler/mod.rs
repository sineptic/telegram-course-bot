use std::{collections::BTreeMap, error::Error, str::FromStr, sync::LazyLock};

use chrono::{DateTime, Local};
use course_graph::{graph::CourseGraph, progress_store::TaskProgress};
use dashmap::DashMap;
use progress_store::UserProgress;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide::{Bot, prelude::Requester, types::UserId};

use super::{Event, EventReceiver};
use crate::{
    handlers::{send_interactions, set_task_for_user},
    interaction_types::{telegram_interaction::QuestionElement, *},
    utils::{Immutable, ResultExt},
};

mod progress_store;

static PROGRESS_STORE: LazyLock<DashMap<UserId, UserProgress>> = LazyLock::new(DashMap::new);
static COURSE_GRAPH: LazyLock<Immutable<CourseGraph>> = LazyLock::new(|| {
    CourseGraph::from_str(&std::fs::read_to_string("graph").unwrap())
        .unwrap_or_else(|err| {
            println!("{err}");
            panic!("graph parsing error");
        })
        .into()
});
static DEFAULT_USER_PROGRESS: LazyLock<Immutable<UserProgress>> = LazyLock::new(|| {
    let mut user_progress = UserProgress::default();
    COURSE_GRAPH.init_store(&mut user_progress);
    user_progress.into()
});
static DEQUE: LazyLock<Immutable<BTreeMap<String, BTreeMap<u16, Task>>>> = LazyLock::new(|| {
    let deque = deque::from_str(&std::fs::read_to_string("cards.md").unwrap(), true).unwrap();
    let mut errors = Vec::new();
    COURSE_GRAPH
        .cards()
        .keys()
        .filter(|&id| !deque.contains_key(id))
        .map(|id| format!("Graph has '{id}' card, but deque(cards.md) doesn't."))
        .for_each(|item| {
            errors.push(item);
        });
    deque
        .keys()
        .filter(|x| !COURSE_GRAPH.cards().contains_key(*x))
        .map(|err| format!("Deque(cards.md) has '{err}', but graph doesn't."))
        .for_each(|item| {
            errors.push(item);
        });
    if !errors.is_empty() {
        panic!(
            "Cards in deque(cards.md) and graph(graph) are different.\n{}",
            errors.join("\n")
        );
    }
    deque.into()
});

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

fn now() -> DateTime<Local> {
    static START_TIME: LazyLock<Immutable<DateTime<Local>>> = LazyLock::new(|| Local::now().into());
    let now = Local::now();
    let diff = now - **START_TIME;
    **START_TIME + diff * 3600
}

pub(crate) async fn event_handler(bot: Bot, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        tokio::spawn(handle_event(bot.clone(), event));
    }
}

async fn handle_event(bot: Bot, event: Event) {
    match event {
        Event::ReviseCard { user_id, card_name } => {
            syncronize(user_id);
            if matches!(
                PROGRESS_STORE
                    .entry(user_id)
                    .or_insert_with(|| DEFAULT_USER_PROGRESS.clone())[&card_name],
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
                return;
            }

            if let Some(rcx) = handle_revise(&card_name, bot, user_id).await {
                PROGRESS_STORE
                    .entry(user_id)
                    .or_insert_with(|| DEFAULT_USER_PROGRESS.clone())
                    .repetition(&card_name, rcx);
            }
        }

        Event::ViewGraph { user_id } => {
            syncronize(user_id);
            let graph = course_graph::generate_graph(
                COURSE_GRAPH.generate_graph(),
                &*PROGRESS_STORE
                    .entry(user_id)
                    .or_insert_with(|| DEFAULT_USER_PROGRESS.clone()),
            );

            let graph_image = tokio::task::spawn_blocking(move || course_graph::print_graph(graph))
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
        }
        Event::Revise { user_id } => {
            syncronize(user_id);
            let a = PROGRESS_STORE
                .entry(user_id)
                .or_insert_with(|| DEFAULT_USER_PROGRESS.clone())
                .revise(async |id| handle_revise(id, bot.clone(), user_id).await.unwrap())
                .await;
            if a.is_none() {
                bot.send_message(user_id, "You don't have card to revise.")
                    .await
                    .log_err();
            }
        }
        Event::Clear { user_id } => {
            PROGRESS_STORE.insert(user_id, DEFAULT_USER_PROGRESS.clone());

            send_interactions(bot, user_id, vec!["Progress cleared.".into()])
                .await
                .log_err();
        }
    }
}

fn syncronize(user_id: UserId) {
    let mut user_progress = PROGRESS_STORE
        .entry(user_id)
        .or_insert_with(|| DEFAULT_USER_PROGRESS.clone());
    user_progress.syncronize(now().into());
    COURSE_GRAPH.detect_recursive_fails(&mut *user_progress);
}

async fn handle_revise(id: &String, bot: Bot, user_id: UserId) -> Option<RepetitionContext> {
    let Task {
        question,
        options,
        answer,
        explanation,
    } = {
        card::random_task(
            {
                if let Some(x) = DEQUE.get(id) {
                    x
                } else {
                    send_interactions(bot, user_id, vec!["Card with this name not found".into()])
                        .await
                        .log_err();
                    return None;
                }
            },
            rand::rng(),
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
        review_time: now(),
    })
}
