use std::{cmp::max, error::Error, sync::LazyLock};

use dashmap::DashMap;
use teloxide_core::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, UpdateKind},
};

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use state::State;

use crate::{
    event_handler::{get_course, handle_event},
    handlers::{HandleResult, progress_on_user_event, send_interactions},
    interaction_types::TelegramInteraction,
    utils::ResultExt,
};
static STATE: LazyLock<DashMap<UserId, State>> = LazyLock::new(DashMap::new);

#[derive(Clone, Debug)]
#[allow(unused)]
enum Event {
    PreviewCard { user_id: UserId, card_name: String },
    ReviseCard { user_id: UserId, card_name: String },
    ViewGraph { user_id: UserId },
    Revise { user_id: UserId },
    Clear { user_id: UserId },
    ChangeCourseGraph { user_id: UserId },
    ChangeDeque { user_id: UserId },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use handlers::*;

    dotenvy::dotenv()?;
    pretty_env_logger::init();
    let bot = Bot::from_env();

    log::info!("Bot started");

    let mut offset = 0;
    loop {
        let updates = bot
            .get_updates()
            .offset((offset + 1).try_into().unwrap())
            // .timeout(30) // FIXME: can't do this(as docs sad)
            .send()
            .await?;
        for update in updates {
            offset = max(offset, update.id.0);

            let bot = bot.clone();
            tokio::spawn(async move {
                match update.kind {
                    UpdateKind::Message(message) => {
                        handle_message(bot, message).await.log_err();
                    }
                    UpdateKind::CallbackQuery(callback_query) => {
                        callback_handler(bot, callback_query).await.log_err();
                    }
                    _ => todo!(),
                };
            });
        }
    }
}

async fn handle_message(bot: Bot, message: Message) -> HandleResult {
    static HELP_MESSAGE: &str = "
/card CARD_NAME — Try to complete card
/graph — View course structure
/help — Display all commands
/clear — Reset your state to default(clear all progress)
/change_course_graph
/change_deque
/view_course_graph_source
/view_deque_source
/view_course_errors
";

    let Some(ref user) = message.from else {
        log::warn!("Can't get user info from message {}", message.id);
        return Ok(());
    };
    let Some(text) = message.text() else {
        log::error!(
            "Message should contain text. This message is from user {user:?} and has id {}",
            message.id
        );
        return Ok(());
    };
    assert!(!text.is_empty());
    log::trace!("user {user:?} sends message '{text}'.");
    let (first_word, tail) = text.trim().split_once(" ").unwrap_or((text, ""));
    match first_word {
        "/help" => {
            log::info!(
                "user {}({}) sends help command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            bot.send_message(message.chat.id, HELP_MESSAGE).await?;
        }
        "/start" => {
            log::info!(
                "user {}({}) sends start command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            // TODO: onboarding
            bot.send_message(message.chat.id, "TODO: onboarding")
                .await?;

            bot.send_message(message.chat.id, HELP_MESSAGE).await?;
        }
        "/card" => {
            if tail.contains(" ") {
                bot.send_message(user.id, "Error: Card name should not contain spaces.")
                    .await?;
                return Ok(());
            }
            if tail.is_empty() {
                bot.send_message(
                    user.id,
                    "Error: You should provide card name, you want to learn.",
                )
                .await?;
                return Ok(());
            }
            log::info!(
                "user {}({}) sends card '{tail}' command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(
                bot,
                Event::PreviewCard {
                    user_id: user.id,
                    card_name: tail.to_owned(),
                },
            )
            .await?;
        }
        "/graph" => {
            log::info!(
                "user {}({}) sends graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(bot, Event::ViewGraph { user_id: user.id }).await?;
        }
        "/revise" => {
            log::info!(
                "user {}({}) sends revise command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            bot.send_message(user.id, "This command is temporarily disabled")
                .await?;
        }
        "/clear" => {
            log::info!(
                "user {}({}) sends clear command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(bot, Event::Clear { user_id: user.id }).await?;
        }
        "/change_course_graph" => {
            log::info!(
                "user {}({}) sends change_course_graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(bot, Event::ChangeCourseGraph { user_id: user.id }).await?;
        }
        "/change_deque" => {
            log::info!(
                "user {}({}) sends change_deque command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(bot, Event::ChangeDeque { user_id: user.id }).await?;
        }
        "/view_course_graph_source" => {
            log::info!(
                "user {}({}) sends view_course_graph_source command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            send_interactions(
                bot,
                user.id,
                vec![
                    "Course graph source:".into(),
                    format!(
                        "```\n{}\n```",
                        get_course(user.id).get_course_graph().get_source()
                    )
                    .into(),
                ],
            )
            .await?;
        }
        "/view_deque_source" => {
            log::info!(
                "user {}({}) sends view_deque_source command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            send_interactions(
                bot,
                user.id,
                vec![
                    "Deque source:".into(),
                    format!(
                        "```\n{}\n```",
                        get_course(user.id).get_deque().source.to_owned()
                    )
                    .into(),
                ],
            )
            .await
            .log_err();
        }
        "/view_course_errors" => {
            log::info!(
                "user {}({}) sends view_course_errors command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if let Some(errors) = get_course(user.id).get_errors() {
                let mut msgs = Vec::new();
                msgs.push("Errors:".into());
                for error in errors {
                    msgs.push(error.into());
                }
                send_interactions(bot, user.id, msgs).await.log_err();
            } else {
                send_interactions(bot, user.id, vec!["No errors!".into()])
                    .await
                    .log_err();
            }
        }
        // dialogue handling
        _ => {
            let mut state = STATE.entry(user.id).or_default();
            let state = state.value_mut();
            match state {
                State::UserEvent {
                    interactions,
                    current,
                    current_id,
                    current_message,
                    answers,
                    channel: _,
                } => match &interactions[*current] {
                    TelegramInteraction::UserInput => {
                        let user_input = message.text().unwrap().to_owned();

                        bot.delete_message(message.chat.id, current_message.unwrap())
                            .await
                            .log_err()
                            .unwrap();

                        answers.push(user_input);
                        *current += 1;
                        *current_id = rand::random();

                        progress_on_user_event(
                            bot,
                            message
                                .from
                                .ok_or("Message should contain user id")
                                .log_err()
                                .unwrap()
                                .id,
                            state,
                        )
                        .await
                        .log_err()
                        .unwrap();
                    }
                    _ => {
                        bot.send_message(message.chat.id, "Unexpected input")
                            .await
                            .log_err()
                            .unwrap();
                    }
                },
                State::Idle => {
                    bot.send_message(message.chat.id, "Command not found!")
                        .await
                        .log_err()
                        .unwrap();
                }
            }
        }
    }
    Ok(())
}
