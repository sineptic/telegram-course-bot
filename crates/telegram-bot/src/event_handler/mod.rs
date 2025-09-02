use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use anyhow::Context;
use chrono::{DateTime, Local};
use course_graph::{graph::CourseGraph, progress_store::TaskProgress};
use dashmap::DashMap;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide_core::{Bot, prelude::Requester, types::UserId};

use crate::{
    STORAGE,
    database::CourseId,
    handlers::{send_interactions, set_task_for_user},
    interaction_types::{telegram_interaction::QuestionElement, *},
    state::{MutUserState, UserState},
    utils::{Immutable, ResultExt},
};

pub mod progress_store;

async fn get_user_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    answers: Vec<String>,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<Option<String>> {
    let answer = get_user_answer_raw(
        bot,
        user_id,
        interactions
            .into_iter()
            .map(|x| x.into())
            .chain([TelegramInteraction::OneOf(answers)]),
        user_state,
    )
    .await?;
    Ok(answer.map(|mut x| x.pop().unwrap()))
}
async fn get_user_answer_raw(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = TelegramInteraction>,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<Option<Vec<String>>> {
    let interactions = interactions.into_iter().collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    set_task_for_user(bot, user_id, interactions, tx, user_state).await?;
    let Ok(answer) = rx.await else {
        return Ok(None);
    };
    Ok(Some(answer))
}

async fn get_card_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    answers: Vec<String>,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<Option<String>> {
    // TODO: add 'I dont know' option
    get_user_answer(bot, user_id, interactions, answers, user_state).await
}

fn now() -> DateTime<Local> {
    static START_TIME: LazyLock<Immutable<DateTime<Local>>> = LazyLock::new(|| Local::now().into());
    let now = Local::now();
    let diff = now - **START_TIME;
    **START_TIME + diff * 3600
}

pub async fn handle_revise(
    bot: Bot,
    mut user_state: MutUserState<'_, '_>,
    user_id: UserId,
    course_id: CourseId,
) {
    todo!();
    // syncronize(user_id, course_id);
    //
    // let a = get_progress(user_id)
    //     .revise(async |id| handle_revise(id, bot.clone(), user_id).await.unwrap())
    //     .await;
    // if a.is_none() {
    //     bot.send_message(user_id, "You don't have card to revise.")
    //         .await?;
    // }
}

pub async fn handle_changing_course_graph(
    bot: Bot,
    mut user_state: MutUserState<'_, '_>,
    user_id: UserId,
    course_id: CourseId,
) -> anyhow::Result<()> {
    let (source, printed_graph) = {
        let Some(course) = STORAGE.get_course(course_id) else {
            bot.send_message(
                user_id,
                format!("Course with id {} not found.", course_id.0),
            )
            .await?;
            return Ok(());
        };
        if course.owner_id != user_id {
            bot.send_message(user_id, "It's not your course.").await?;
            return Ok(());
        }
        let course_graph = &course.structure;
        let source = course_graph.get_source().to_owned();
        let graph = course_graph.generate_structure_graph();
        let printed_graph = tokio::task::spawn_blocking(move || {
            graphviz_rust::exec(
                graph,
                &mut graphviz_rust::printer::PrinterContext::default(),
                vec![graphviz_rust::cmd::Format::Jpeg.into()],
            )
            .context("Failed to run 'dot'")
        })
        .await
        .unwrap()?;
        (source, printed_graph)
    };

    if let Some(answer) = get_user_answer_raw(
        bot.clone(),
        user_id,
        vec![
            "Current graph:".into(),
            TelegramInteraction::PersonalImage(printed_graph),
            "Courrent source:".into(),
            format!("```\n{source}\n```").into(),
            "Print new source:".into(),
            TelegramInteraction::UserInput,
        ],
        user_state,
    )
    .await?
    {
        assert_eq!(answer.len(), 6);
        #[allow(clippy::needless_range_loop)]
        for i in 0..answer.len() - 1 {
            assert!(answer[i].is_empty());
        }
        let answer = answer.last().unwrap();

        match CourseGraph::from_str(answer) {
            Ok(new_course_graph) => {
                let mut new_course = arc_deep_clone(STORAGE.get_course(course_id).unwrap());
                new_course.structure = new_course_graph;
                send_interactions(
                    bot,
                    user_id,
                    vec!["Course graph changed".into()],
                    user_state,
                )
                .await?;
            }
            Err(err) => {
                let err = strip_ansi_escapes::strip_str(err);
                send_interactions(
                    bot,
                    user_id,
                    vec![
                        "Your course graph has this errors:".into(),
                        format!("```\n{err}\n```").into(),
                    ],
                    user_state,
                )
                .await?;
            }
        }
    }
    Ok(())
}
pub async fn handle_changing_deque(
    bot: Bot,
    mut user_state: MutUserState<'_, '_>,
    user_id: UserId,
    course_id: CourseId,
) -> anyhow::Result<()> {
    let Some(course) = STORAGE.get_course(course_id) else {
        bot.send_message(
            user_id,
            format!("Course with id {} not found.", course_id.0),
        )
        .await?;
        return Ok(());
    };
    if course.owner_id != user_id {
        bot.send_message(user_id, "It's not your course.").await?;
        return Ok(());
    }
    let source = course.tasks.source.clone();

    if let Some(answer) = get_user_answer_raw(
        bot.clone(),
        user_id,
        vec![
            "Current source:".into(),
            format!("```\n{source}\n```").into(),
            "Print new source:".into(),
            TelegramInteraction::UserInput,
        ],
        user_state,
    )
    .await?
    {
        assert_eq!(answer.len(), 4);
        #[allow(clippy::needless_range_loop)]
        for i in 0..answer.len() - 1 {
            assert!(answer[i].is_empty());
        }
        let answer = answer.last().unwrap();

        match deque::from_str(answer, true) {
            Ok(new_deque) => {
                let mut new_course = arc_deep_clone(course);
                new_course.tasks = new_deque;
                STORAGE.set_course(course_id, new_course);
                send_interactions(bot, user_id, vec!["Deque changed".into()], user_state).await?;
            }
            Err(err) => {
                send_interactions(
                    bot,
                    user_id,
                    vec![
                        "Your deque has this errors:".into(),
                        format!("```\n{err}\n```").into(),
                    ],
                    user_state,
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn learn_card(
    bot: Bot,
    user_id: UserId,
    course_id: CourseId,
    user_state: MutUserState<'_, '_>,
    card_name: String,
) -> Result<(), anyhow::Error> {
    syncronize(user_id, course_id);
    if matches!(
        STORAGE.get_progress(user_id, course_id)[&card_name],
        TaskProgress::NotStarted {
            could_be_learned: false
        }
    ) {
        send_interactions(
            bot.clone(),
            user_id,
            vec!["You should learn all dependencies before starting new card.".into()],
            user_state,
        )
        .await?;
        return Ok(());
    }
    if let Some(rcx) = complete_card(bot, user_id, course_id, user_state, &card_name).await {
        let mut progress = arc_deep_clone(STORAGE.get_progress(user_id, course_id));
        progress.repetition(&card_name, rcx);
        STORAGE.set_course_progress(user_id, course_id, progress);
    }
    Ok(())
}

fn arc_deep_clone<T: Clone>(arc: Arc<T>) -> T {
    let mut new_value = arc.clone();
    Arc::make_mut(&mut new_value);
    Arc::into_inner(new_value).unwrap()
}

pub fn syncronize(user_id: UserId, course_id: CourseId) {
    return;
    todo!();
    // let mut user_progress = get_progress(course_id, user_id);
    // user_progress.syncronize(now().into());
    // COURSES_STORAGE.get_course(course_id).unwrap().structure;
    // get_course(user_id)
    //     .get_course_graph()
    //     .detect_recursive_fails(&mut *user_progress);
}

pub async fn complete_card(
    bot: Bot,
    user_id: UserId,
    course_id: CourseId,
    mut user_state: MutUserState<'_, '_>,
    card_name: &str,
) -> Option<RepetitionContext> {
    let Task {
        question,
        options,
        answer,
        explanation,
    } = {
        let course = STORAGE.get_course(course_id).unwrap();
        card::random_task(
            {
                if let Some(x) = course.tasks.tasks.get(card_name) {
                    x
                } else {
                    send_interactions(
                        bot,
                        user_id,
                        vec!["Card with this name not found".into()],
                        user_state,
                    )
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
    if let Some(user_answer) = get_card_answer(
        bot.clone(),
        user_id,
        question.clone(),
        options.clone(),
        user_state,
    )
    .await
    .log_err()
    .unwrap()
    {
        if user_answer == options[answer] {
            correct = true;
            send_interactions(bot.clone(), user_id, vec!["Correct!".into()], user_state)
                .await
                .log_err();
        } else {
            send_interactions(
                bot.clone(),
                user_id,
                vec![format!("Wrong. Answer is {}", options[answer]).into()],
                user_state,
            )
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
                    user_state,
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
    } else {
        Some(RepetitionContext {
            quality: Quality::Again,
            review_time: now(),
        })
    }
}
