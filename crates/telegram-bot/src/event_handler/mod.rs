use std::{str::FromStr, sync::LazyLock};

use anyhow::Context;
use chrono::{DateTime, Local};
use course_graph::graph::CourseGraph;
use dashmap::DashMap;
use rand::seq::SliceRandom;
use ssr_algorithms::fsrs::level::{Quality, RepetitionContext};
use teloxide_core::{Bot, prelude::Requester, types::UserId};

use crate::{
    database::*,
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
    user_state: MutUserState<'_>,
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
    .await
    .context("failed to get user answer raw")?;
    Ok(answer.map(|mut x| x.pop().unwrap()))
}
async fn get_user_answer_raw(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = TelegramInteraction>,
    user_state: MutUserState<'_>,
) -> anyhow::Result<Option<Vec<String>>> {
    let interactions = interactions.into_iter().collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    set_task_for_user(bot, user_id, interactions, tx, user_state)
        .await
        .context("failed to set task for user")?;
    let Ok(answer) = rx.await else {
        return Ok(None);
    };
    Ok(Some(answer))
}

const I_DONT_KNOW_MESSAGE: &str = "I don't know";

async fn get_card_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    mut answers: Vec<String>,
    user_state: MutUserState<'_>,
) -> anyhow::Result<Option<String>> {
    answers.shuffle(&mut rand::rng());
    answers.push(I_DONT_KNOW_MESSAGE.into());

    get_user_answer(bot, user_id, interactions, answers, user_state).await
}

fn now() -> DateTime<Local> {
    static START_TIME: LazyLock<Immutable<DateTime<Local>>> = LazyLock::new(|| Local::now().into());
    let now = Local::now();
    let diff = now - **START_TIME;
    **START_TIME + diff * 1 // No speedup
}

pub async fn handle_changing_course_graph(
    bot: Bot,
    user_state: MutUserState<'_>,
    user_id: UserId,
    course_id: CourseId,
) -> anyhow::Result<()> {
    let (source, printed_graph) = {
        let Some(course) = db_get_course(course_id) else {
            bot.send_message(
                user_id,
                format!("Course with id {} not found.", course_id.0),
            )
            .await
            .context("failed to notify user, that there is no course with this id")?;
            return Ok(());
        };
        if course.owner_id != user_id {
            bot.send_message(user_id, "It's not your course.")
                .await
                .context("failed to warn user, that he can change only his own courses")?;
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
    .await
    .context("failed to display current graph")?
    {
        assert_eq!(answer.len(), 6);
        #[allow(clippy::needless_range_loop)]
        for i in 0..answer.len() - 1 {
            assert!(answer[i].is_empty());
        }
        let answer = answer.last().unwrap();

        match CourseGraph::from_str(answer) {
            Ok(new_course_graph) => {
                let mut new_course = db_get_course(course_id).unwrap();
                new_course.structure = new_course_graph;
                db_set_course(course_id, new_course);
                bot.send_message(user_id, "Course graph changed.")
                    .await
                    .context("failed to confirm course graph change")?;
            }
            Err(err) => {
                let err = strip_ansi_escapes::strip_str(err);
                bot.send_message(
                    user_id,
                    format!("Your course graph has this errors:\n```\n{err}\n```"),
                )
                .await
                .context("failed to notify that course graph has errors")?;
            }
        }
    }
    Ok(())
}
pub async fn handle_changing_deque(
    bot: Bot,
    user_state: MutUserState<'_>,
    user_id: UserId,
    course_id: CourseId,
) -> anyhow::Result<()> {
    let Some(course) = db_get_course(course_id) else {
        bot.send_message(
            user_id,
            format!("Course with id {} not found.", course_id.0),
        )
        .await
        .context("failed to responde to user, that course not found")?;
        return Ok(());
    };
    if course.owner_id != user_id {
        bot.send_message(user_id, "It's not your course.")
            .await
            .context("failed to warn user, that hi can change only his courses")?;
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
    .await
    .context("failed to send current course tasks")?
    {
        assert_eq!(answer.len(), 4);
        #[allow(clippy::needless_range_loop)]
        for i in 0..answer.len() - 1 {
            assert!(answer[i].is_empty());
        }
        let answer = answer.last().unwrap();

        match deque::from_str(answer, true) {
            Ok(new_deque) => {
                let mut new_course = course;
                new_course.tasks = new_deque;
                db_set_course(course_id, new_course);
                bot.send_message(user_id, "Deque changed.")
                    .await
                    .context("failed to confirm, that deque is changed")?;
            }
            Err(err) => {
                bot.send_message(
                    user_id,
                    format!("Your deque has this errors:\n```\n{err}\n```"),
                )
                .await
                .context("failed to notify user, that deque has errors")?;
            }
        }
    }
    Ok(())
}

pub fn syncronize(user_id: UserId, course_id: CourseId) {
    let mut progress = db_get_progress(user_id, course_id);
    progress.syncronize(now().into());
    db_get_course(course_id)
        .unwrap()
        .structure
        .detect_recursive_fails(&mut progress);
    db_set_course_progress(user_id, course_id, progress);
}

pub async fn complete_card(
    bot: Bot,
    user_id: UserId,
    Task {
        question,
        options,
        answer,
        explanation,
    }: Task,
    user_state: MutUserState<'_>,
    user_states: &DashMap<UserId, UserState>,
) -> (RepetitionContext, bool) {
    let Some(user_answer) = get_card_answer(
        bot.clone(),
        user_id,
        question.clone(),
        options.clone(),
        user_state,
    )
    .await
    .log_err()
    .unwrap() else {
        return (
            RepetitionContext {
                quality: Quality::Again,
                review_time: now(),
            },
            false,
        );
    };
    if user_answer == options[answer] {
        bot.send_message(user_id, "Correct!").await.log_err();
        (
            RepetitionContext {
                quality: Quality::Good,
                review_time: now(),
            },
            true,
        )
    } else {
        let mut messages = Vec::new();
        messages.push(TelegramInteraction::Text(
            if user_answer == I_DONT_KNOW_MESSAGE {
                format!("Answer is {}", options[answer])
            } else {
                format!("Wrong. Answer is {}", options[answer])
            },
        ));
        if let Some(explanation) = explanation {
            messages.extend(explanation.iter().cloned().map(TelegramInteraction::from));
        }
        let user_state = user_states.get_mut(&user_id).unwrap();
        send_interactions(bot.clone(), user_id, messages, user_state)
            .await
            .log_err();
        (
            RepetitionContext {
                quality: Quality::Again,
                review_time: now(),
            },
            true,
        )
    }
}
