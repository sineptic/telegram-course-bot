use std::{error::Error, str::FromStr, sync::LazyLock};

use chrono::{DateTime, Local};
use course::Course;
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

mod course;
static COURSES_STORE: LazyLock<DashMap<UserId, Course>> = LazyLock::new(DashMap::new);
fn get_course<'a>(user_id: UserId) -> dashmap::mapref::one::RefMut<'a, UserId, Course> {
    COURSES_STORE.entry(user_id).or_default()
}

static PROGRESS_STORE: LazyLock<DashMap<UserId, UserProgress>> = LazyLock::new(DashMap::new);
fn get_progress<'a>(user_id: UserId) -> dashmap::mapref::one::RefMut<'a, UserId, UserProgress> {
    PROGRESS_STORE
        .entry(user_id)
        .or_insert_with(|| get_course(user_id).default_user_progress())
}

async fn get_user_answer(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = QuestionElement>,
    answers: Vec<String>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let answer = get_user_answer_raw(
        bot,
        user_id,
        interactions
            .into_iter()
            .map(|x| x.into())
            .chain([TelegramInteraction::OneOf(answers)]),
    )
    .await?;
    Ok(answer.map(|mut x| x.pop().unwrap()))
}
async fn get_user_answer_raw(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = TelegramInteraction>,
) -> Result<Option<Vec<String>>, Box<dyn Error + Send + Sync>> {
    let interactions = interactions.into_iter().collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    set_task_for_user(bot, user_id, interactions, tx).await?;
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
            log::info!("user {user_id} trigger ReviseCard event");
            syncronize(user_id);
            if matches!(
                get_progress(user_id)[&card_name],
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
                get_progress(user_id).repetition(&card_name, rcx);
            }
        }
        Event::PreviewCard { user_id, card_name } => {
            log::info!("user {user_id} trigger PreviewCard event");
            handle_revise(&card_name, bot, user_id).await;
        }

        Event::ViewGraph { user_id } => {
            log::info!("user {user_id} trigger ViewGraph event");
            syncronize(user_id);
            let graph = course_graph::generate_graph(
                get_course(user_id).get_course_graph().generate_graph(),
                &*get_progress(user_id),
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
            log::info!("user {user_id} trigger Revise event");
            syncronize(user_id);
            let a = get_progress(user_id)
                .revise(async |id| handle_revise(id, bot.clone(), user_id).await.unwrap())
                .await;
            if a.is_none() {
                bot.send_message(user_id, "You don't have card to revise.")
                    .await
                    .log_err();
            }
        }
        Event::Clear { user_id } => {
            log::info!("user {user_id} trigger Clear event");
            let new_course = Course::default();
            let new_progress = new_course.default_user_progress();
            *get_course(user_id) = new_course;
            *get_progress(user_id) = new_progress;

            send_interactions(bot, user_id, vec!["Progress cleared.".into()])
                .await
                .log_err();
        }
        Event::ChangeCourseGraph { user_id } => {
            log::info!("user {user_id} trigger ChangeCourseGraph event");
            let (source, printed_graph) = {
                let course = get_course(user_id);
                let course_graph = course.get_course_graph();
                let source = course_graph.get_source().to_owned();
                let generated_graph = course_graph.generate_graph();
                drop(course);
                let printed_graph =
                    tokio::task::spawn_blocking(move || course_graph::print_graph(generated_graph))
                        .await
                        .log_err()
                        .unwrap();
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
            )
            .await
            .log_err()
            .unwrap()
            {
                assert_eq!(answer.len(), 6);
                #[allow(clippy::needless_range_loop)]
                for i in 0..answer.len() - 1 {
                    assert!(answer[i].is_empty());
                }
                let answer = answer.last().unwrap();

                match CourseGraph::from_str(answer) {
                    Ok(new_course_graph) => {
                        get_course(user_id).set_course_graph(new_course_graph);
                        *get_progress(user_id) = get_course(user_id).default_user_progress();
                        send_interactions(bot, user_id, vec!["Course graph changed".into()])
                            .await
                            .log_err();
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
                        )
                        .await
                        .log_err();
                    }
                }
            }
        }
        Event::ChangeDeque { user_id } => {
            log::info!("user {user_id} trigger ChangeDeque event");
            let source = get_course(user_id).get_deque().source.clone();

            if let Some(answer) = get_user_answer_raw(
                bot.clone(),
                user_id,
                vec![
                    "Current source:".into(),
                    format!("```\n{source}\n```").into(),
                    "Print new source:".into(),
                    TelegramInteraction::UserInput,
                ],
            )
            .await
            .log_err()
            .unwrap()
            {
                assert_eq!(answer.len(), 4);
                #[allow(clippy::needless_range_loop)]
                for i in 0..answer.len() - 1 {
                    assert!(answer[i].is_empty());
                }
                let answer = answer.last().unwrap();

                match deque::from_str(answer, true) {
                    Ok(new_deque) => {
                        get_course(user_id).set_deque(new_deque);
                        let default_user_progress = get_course(user_id).default_user_progress();
                        *get_progress(user_id) = default_user_progress;
                        send_interactions(bot, user_id, vec!["Deque changed".into()])
                            .await
                            .log_err();
                    }
                    Err(err) => {
                        send_interactions(
                            bot,
                            user_id,
                            vec![
                                "Your deque has this errors:".into(),
                                format!("```\n{err}\n```").into(),
                            ],
                        )
                        .await
                        .log_err();
                    }
                }
            }
        }
        Event::ViewCourseGraphSource { user_id } => {
            log::info!("user {user_id} trigger ViewCourseGraphSource event");
            let source = get_course(user_id)
                .get_course_graph()
                .get_source()
                .to_owned();
            send_interactions(
                bot,
                user_id,
                vec![
                    "Course graph source:".into(),
                    format!("```\n{source}\n```").into(),
                ],
            )
            .await
            .log_err();
        }
        Event::ViewDequeSource { user_id } => {
            log::info!("user {user_id} trigger ViewDequeSource event");
            let source = get_course(user_id).get_deque().source.to_owned();
            send_interactions(
                bot,
                user_id,
                vec!["Deque source:".into(), format!("```\n{source}\n```").into()],
            )
            .await
            .log_err();
        }
        Event::ViewCourseErrors { user_id } => {
            log::info!("user {user_id} trigger ViewCourseErrors event");
            if let Some(errors) = get_course(user_id).get_errors() {
                let mut msgs = Vec::new();
                msgs.push("Errors:".into());
                for error in errors {
                    msgs.push(error.into());
                }
                send_interactions(bot, user_id, msgs).await.log_err();
            } else {
                send_interactions(bot, user_id, vec!["No errors!".into()])
                    .await
                    .log_err();
            }
        }
    }
}

fn syncronize(user_id: UserId) {
    let mut user_progress = get_progress(user_id);
    user_progress.syncronize(now().into());
    get_course(user_id)
        .get_course_graph()
        .detect_recursive_fails(&mut *user_progress);
}

async fn handle_revise(id: &String, bot: Bot, user_id: UserId) -> Option<RepetitionContext> {
    let Task {
        question,
        options,
        answer,
        explanation,
    } = {
        let course = get_course(user_id);
        card::random_task(
            {
                if let Some(x) = course.get_deque().tasks.get(id) {
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
            .log_err()
            .unwrap()
    {
        if user_answer == options[answer] {
            correct = true;
            send_interactions(bot.clone(), user_id, vec!["Correct!".into()])
                .await
                .log_err();
        } else {
            send_interactions(
                bot.clone(),
                user_id,
                vec![format!("Wrong. Answer is {}", options[answer]).into()],
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
