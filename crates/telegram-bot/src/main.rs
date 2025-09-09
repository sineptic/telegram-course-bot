use std::cmp::max;

use anyhow::Context;
use course_graph::{
    graph::CourseGraph,
    progress_store::{TaskProgress, TaskProgressStoreExt},
};
use dashmap::DashMap;
use graphviz_rust::{cmd::Format, printer::PrinterContext};
use teloxide_core::{
    RequestError,
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Update, UpdateKind, User},
};

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use database::*;

use crate::{
    event_handler::{
        complete_card, handle_changing_course_graph, handle_changing_deque, syncronize,
    },
    handlers::{callback_handler, progress_on_user_event, send_interactions},
    interaction_types::{TelegramInteraction, deque::Deque},
    state::*,
    utils::ResultExt,
};
mod database;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("'TELOXIDE_TOKEN' variable should be specified in '.env' file");
    pretty_env_logger::init();
    let bot = Bot::from_env();
    let users_state: &DashMap<UserId, UserState> = Box::leak(Box::new(DashMap::new()));
    db_create_tables();

    log::info!("Bot started");

    let mut offset = 0;
    loop {
        let updates = bot
            .get_updates()
            .offset((offset + 1).try_into().unwrap())
            .timeout(30)
            .send()
            .await;
        let updates = match updates {
            Ok(x) => x,
            Err(err) => match err {
                RequestError::Network(error) if error.is_timeout() => {
                    log::trace!("Telegram connection timed out.");
                    continue;
                }
                other_error => {
                    log::error!(
                        "Error while connection to telegram to receive updates: {other_error}."
                    );
                    continue;
                }
            },
        };
        for update in updates {
            offset = max(offset, update.id.0);

            let bot = bot.clone();
            tokio::spawn(update_handler(bot, update, users_state));
        }
    }
}

async fn update_handler(bot: Bot, update: Update, user_states: &DashMap<UserId, UserState>) {
    match update.kind {
        UpdateKind::Message(message) => {
            let Some(ref user) = message.from else {
                log::warn!("Can't get user info from message {}", message.id);
                bot.send_message(message.chat.id, "Bot works only with users")
                    .await
                    .log_err();
                return;
            };
            let Some(text) = message.text() else {
                log::error!(
                    "Message should contain text. This message is from user {user:?} and has id {}",
                    message.id
                );
                return;
            };
            assert!(!text.is_empty());
            log::trace!("user {user:?} sends message '{text}'.");
            let user_state = user_states.entry(user.id).or_default();
            match user_state.current_screen {
                Screen::Main => {
                    handle_main_menu_interaction(bot, user, text, user_state)
                        .await
                        .log_err();
                }
                Screen::Course(course_id) => {
                    match db_get_course(course_id).unwrap().owner_id == user.id {
                        true => {
                            handle_owned_course_interaction(
                                bot,
                                user,
                                text,
                                course_id,
                                user_state,
                                user_states,
                            )
                            .await
                            .log_err();
                        }
                        false => {
                            handle_learned_course_interaction(
                                bot,
                                user,
                                text,
                                course_id,
                                user_state,
                                user_states,
                            )
                            .await
                            .log_err();
                        }
                    };
                }
            }
        }
        UpdateKind::CallbackQuery(callback_query) => {
            callback_handler(bot, callback_query, user_states)
                .await
                .log_err();
        }
        _ => todo!(),
    };
}

async fn send_help_message(
    bot: Bot,
    user: &User,
    user_state: &MutUserState<'_>,
) -> anyhow::Result<()> {
    let main_menu_help_message = "
/help - Display all commands

/create_course - Create new course and get it's ID
/list - List all your courses
/course COURSE_ID - Go to course menu
";
    let owned_course_help_message = "
/help — Display all commands
/exit - Go to main menu

/preview CARD_NAME — Try to complete card
/graph — View course structure
/change_course_graph
/change_deque
/view_course_graph_source
/view_deque_source
/view_course_errors
";
    let learned_course_help_message = "
/help — Display all commands
/exit - Go to main menu

/card CARD_NAME — Try to complete card
/graph — View course structure
";

    bot.send_message(
        user.id,
        match user_state.current_screen {
            Screen::Main => main_menu_help_message,
            Screen::Course(course_id) => {
                match db_get_course(course_id).unwrap().owner_id == user.id {
                    true => owned_course_help_message,
                    false => learned_course_help_message,
                }
            }
        },
    )
    .await
    .context("failed to send help message")?;
    Ok(())
}

fn log_user_command(user: &User, command_name: &str) {
    log::info!(
        "user {}({}) sends {command_name} command",
        user.username.clone().unwrap_or("unknown".into()),
        user.id
    );
}

async fn handle_main_menu_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    mut user_state: MutUserState<'_>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/start" => {
            log_user_command(user, "start");
            // TODO: onboarding
            bot.send_message(user.id, "TODO: onboarding").await?;

            send_help_message(bot, user, &user_state).await?;
        }
        "/create_course" => {
            log_user_command(user, "create_course");
            let course_id = db_insert(Course {
                owner_id: user.id,
                structure: CourseGraph::default(),
                tasks: Deque::default(),
            });
            bot.send_message(user.id, format!("Course created with id {}.", course_id.0))
                .await
                .context("failed to confirm, that course created")
                .log_err();
            user_state.current_screen = Screen::Course(course_id);
            bot.send_message(user.id, "You are now in course menu.")
                .await
                .context("failed to notify user, that he is now in course menu")?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/course" => {
            let Ok(course_id) = tail.parse() else {
                bot.send_message(
                    user.id,
                    format!("Can't parse course id from this string: '{tail}'."),
                )
                .await
                .context("failed to notify user about parsing error")?;
                return Ok(());
            };
            log::info!(
                "user {}({}) sends course '{course_id}' command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let course_id = CourseId(course_id);
            if db_get_course(course_id).is_none() {
                bot.send_message(user.id, "Can't find course with this id.")
                    .await
                    .context("failed to notify user, that course with this id doesn't exists")?;
                return Ok(());
            }
            user_state.current_screen = Screen::Course(course_id);
            db_add_course_to_user(user.id, course_id);
            bot.send_message(user.id, "You are now in course menu.")
                .await
                .context("failed to notify user, that he is now in course menu")?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/list" => {
            log_user_command(user, "list");
            let owned_courses = db_select_courses_by_owner(user.id);
            let learned_courses = db_list_user_learned_courses(user.id);
            let mut message = String::new();
            message.push_str("# Owned\n");
            for course in owned_courses {
                message.push_str(&course.0.to_string());
                message.push('\n');
            }
            message.push_str("# Learned\n");
            for course in learned_courses {
                message.push_str(&course.0.to_string());
                message.push('\n');
            }
            bot.send_message(user.id, message)
                .await
                .context("failed to send list of courses")?;
        }
        _ => {
            handle_no_command(bot, user, message, user_state)
                .await
                .context("failed to handle 'no command'")?;
        }
    }
    Ok(())
}

async fn handle_learned_course_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    course_id: CourseId,
    mut user_state: MutUserState<'_>,
    user_states: &DashMap<UserId, UserState>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/exit" => {
            log_user_command(user, "exit");
            user_state.current_screen = Screen::Main;
            bot.send_message(user.id, "You are now in main menu.")
                .await
                .context("failed to notify user, that he is now in main menu")?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/card" => {
            log_user_command(user, "card");
            if tail.contains(" ") {
                bot.send_message(user.id, "Error: Card name should not contain spaces.")
                    .await
                    .context("failed to send user, that card name should not contain spaces")?;
                return Ok(());
            }
            if tail.is_empty() {
                bot.send_message(
                    user.id,
                    "Error: You should provide card name, you want to learn.",
                )
                .await
                .context("failed to notify user, that card command should contain card name")?;
                return Ok(());
            }
            let card_name = tail;
            log::info!(
                "user {}({}) sends card '{card_name}' command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );

            syncronize(user.id, course_id);
            let task = {
                let course = db_get_course(course_id).unwrap();
                let Some(tasks) = course.tasks.tasks.get(card_name) else {
                    send_interactions(
                        bot,
                        user.id,
                        vec!["Card with this name not found".into()],
                        user_state,
                    )
                    .await
                    .context("failed to notify user, that card with this name not found")?;
                    return Ok(());
                };
                let tasks_list = tasks.values().collect::<Vec<_>>();
                let meaningful_repetitions = db_get_progress(user.id, course_id).tasks
                    [&card_name.to_owned()]
                    .meaningful_repetitions;
                if (meaningful_repetitions as usize) < tasks_list.len() {
                    tasks_list[((meaningful_repetitions as usize)
                        + usize::try_from(user.id.0).unwrap() % tasks_list.len())
                        % tasks_list.len()]
                    .clone()
                } else {
                    interaction_types::card::random_task(tasks, rand::rng()).clone()
                }
            };
            if matches!(
                db_get_progress(user.id, course_id)[&card_name.to_owned()],
                TaskProgress::NotStarted {
                    could_be_learned: false
                }
            ) {
                bot.send_message(
                    user.id,
                    "You should learn all dependencies before learning this card.",
                )
                .await.context("failed to notify user, that he should learn all dependencies before learning this card")?;
                return Ok(());
            }
            let (rcx, is_meaningful) =
                complete_card(bot, user.id, task, user_state, user_states).await;
            let mut progress = db_get_progress(user.id, course_id);
            progress.repetition(&card_name.to_owned(), rcx, is_meaningful);
            db_set_course_progress(user.id, course_id, progress);
        }
        "/graph" => {
            log_user_command(user, "graph");
            if !tail.is_empty() {
                bot.send_message(user.id, "graph command doesn't expect any arguments.")
                    .await
                    .context(
                        "failed to notify user, that graph command doesn't expect any arguments",
                    )?;
                return Ok(());
            }
            syncronize(user.id, course_id);

            let Some(course) = db_get_course(course_id) else {
                bot.send_message(
                    user.id,
                    format!("Course with id {} not found.", course_id.0),
                )
                .await
                .context("failed to notify user, that there is not course with this id")?;
                return Ok(());
            };
            let mut graph = course.structure.generate_structure_graph();

            db_get_progress(user.id, course_id)
                .generate_stmts()
                .into_iter()
                .for_each(|stmt| {
                    graph.add_stmt(stmt);
                });

            send_interactions(
                bot,
                user.id,
                [TelegramInteraction::PersonalImage(
                    tokio::task::spawn_blocking(move || {
                        graphviz_rust::exec(
                            graph,
                            &mut PrinterContext::default(),
                            Vec::from([Format::Jpeg.into()]),
                        )
                        .context("Failed to run 'dot'")
                    })
                    .await
                    .unwrap()?,
                )],
                user_state,
            )
            .await
            .context("failed to send graph image")?;
        }
        _ => {
            handle_no_command(bot, user, message, user_state)
                .await
                .context("failed to handle 'no command'")?;
        }
    }
    Ok(())
}

async fn handle_owned_course_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    course_id: CourseId,
    mut user_state: MutUserState<'_>,
    user_states: &DashMap<UserId, UserState>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/exit" => {
            log_user_command(user, "exit");
            user_state.current_screen = Screen::Main;
            bot.send_message(user.id, "You are now in main menu.")
                .await
                .context("failed to notify user, that he is now in main menu")?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/preview" => {
            log_user_command(user, "preview");
            if tail.contains(" ") {
                bot.send_message(user.id, "Error: Card name should not contain spaces.")
                    .await
                    .context("failed to notify user, that card name should not contain spaces")?;
                return Ok(());
            }
            if tail.is_empty() {
                bot.send_message(
                    user.id,
                    "Error: You should provide card name, you want to learn.",
                )
                .await
                .context(
                    "failed to notify user, that he should provide card name to preview command",
                )?;
                return Ok(());
            }
            log::info!(
                "user {}({}) sends card '{tail}' command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let task = {
                let course = db_get_course(course_id).unwrap();
                let Some(tasks) = course.tasks.tasks.get(tail) else {
                    send_interactions(
                        bot,
                        user.id,
                        vec!["Card with this name not found".into()],
                        user_state,
                    )
                    .await
                    .context("failed to notify user, that there is no card with this name")?;
                    return Ok(());
                };
                interaction_types::card::random_task(tasks, rand::rng()).clone()
            };
            complete_card(bot, user.id, task, user_state, user_states).await;
        }
        "/graph" => {
            log_user_command(user, "graph");
            if !tail.is_empty() {
                bot.send_message(user.id, "graph command doesn't expect any arguments.")
                    .await
                    .context(
                        "failed to notify user, that graph command doesn't have any arguments",
                    )?;
                return Ok(());
            }

            let Some(course) = db_get_course(course_id) else {
                bot.send_message(
                    user.id,
                    format!("Course with id {} not found.", course_id.0),
                )
                .await
                .context("failed to notify user, that there is no course with this id")?;
                return Ok(());
            };
            let graph = course.structure.generate_structure_graph();

            send_interactions(
                bot,
                user.id,
                [TelegramInteraction::PersonalImage(
                    tokio::task::spawn_blocking(move || {
                        graphviz_rust::exec(
                            graph,
                            &mut PrinterContext::default(),
                            Vec::from([Format::Jpeg.into()]),
                        )
                        .context("Failed to run 'dot'")
                    })
                    .await
                    .unwrap()?,
                )],
                user_state,
            )
            .await
            .context("fialed to send graph image")?;
        }
        "/revise" => {
            // TODO
            log_user_command(user, "revise");
            bot.send_message(user.id, "This command is temporarily disabled")
                .await?;
        }
        "/change_course_graph" => {
            log_user_command(user, "change_course_graph");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_course_graph command doesn't expect any arguments.",
                )
                .await
                .context(
                    "failed to notify user, that change_course_graph command doesn't arguments",
                )?;
                return Ok(());
            }
            handle_changing_course_graph(bot, user_state, user.id, course_id)
                .await
                .context("failed to change course graph")?;
        }
        "/change_deque" => {
            log_user_command(user, "change_deque");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_deque command doesn't expect any arguments.",
                )
                .await
                .context(
                    "failed to notify user, that change_deque command doesn't have arguments",
                )?;
                return Ok(());
            }
            handle_changing_deque(bot, user_state, user.id, course_id)
                .await
                .context("failed to change deque")?;
        }
        "/view_course_graph_source" => {
            log_user_command(user, "view_course_graph_source");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_course_graph_source command doesn't expect any arguments.",
                )
                .await.context("failed to notify user, that view_course_graph_source command doesn't have arguments")?;
                return Ok(());
            }
            send_interactions(
                bot,
                user.id,
                vec![
                    "Course graph source:".into(),
                    format!(
                        "```\n{}\n```",
                        db_get_course(course_id).unwrap().structure.get_source()
                    )
                    .into(),
                ],
                user_state,
            )
            .await
            .context("failed to send course graph source")?;
        }
        "/view_deque_source" => {
            log_user_command(user, "view_deque_source");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_deque_source command doesn't expect any arguments.",
                )
                .await
                .context(
                    "failed to notify user, that view_deque_source command doesn't have arguments",
                )?;
                return Ok(());
            }
            send_interactions(
                bot,
                user.id,
                vec![
                    "Deque source:".into(),
                    format!(
                        "```\n{}\n```",
                        db_get_course(course_id).unwrap().tasks.source.to_owned()
                    )
                    .into(),
                ],
                user_state,
            )
            .await
            .context("failed to send deque source")?;
        }
        "/view_course_errors" => {
            log_user_command(user, "view_course_errors");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_course_errors command doesn't expect any arguments.",
                )
                .await
                .context(
                    "failed to notify user, that view_ocurse_errors command doesn't have arguments",
                )?;
                return Ok(());
            }
            if let Some(errors) = db_get_course(course_id).unwrap().get_errors() {
                let mut msgs = Vec::new();
                msgs.push("Errors:".into());
                for error in errors {
                    msgs.push(error.into());
                }
                send_interactions(bot, user.id, msgs, user_state)
                    .await
                    .context("failed to send course errors")?;
            } else {
                send_interactions(bot, user.id, vec!["No errors!".into()], user_state)
                    .await
                    .context("failed to send, that course doesn't have any errors")?;
            }
        }
        _ => {
            handle_no_command(bot, user, message, user_state)
                .await
                .context("failed to handle 'no command'")?;
        }
    }
    Ok(())
}

async fn handle_no_command(
    bot: Bot,
    user: &User,
    message: &str,
    mut user_state: MutUserState<'_>,
) -> anyhow::Result<()> {
    match &mut user_state.current_interaction {
        Some(UserInteraction {
            interactions,
            current,
            current_id,
            current_message,
            answers,
            channel: _,
        }) => match &interactions[*current] {
            TelegramInteraction::UserInput => {
                let user_input = message.to_owned();

                bot.delete_message(user.id, current_message.unwrap())
                    .await
                    .log_err();

                answers.push(user_input);
                *current += 1;
                *current_id = rand::random();

                progress_on_user_event(bot, user.id, &mut user_state.current_interaction)
                    .await
                    .log_err()
                    .unwrap();
            }
            _ => {
                bot.send_message(user.id, "Unexpected input")
                    .await
                    .context("failed to notify user about unexpeceted input")?;
            }
        },
        None => {
            bot.send_message(user.id, "Command not found!")
                .await
                .context("failed to send user, that this command doesn't exist")?;
        }
    };
    Ok(())
}
