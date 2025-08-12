use std::{cmp::max, error::Error, sync::LazyLock};

use dashmap::DashMap;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, UpdateKind},
};

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use state::State;

use crate::{interaction_types::TelegramInteraction, utils::ResultExt};
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
    ViewCourseGraphSource { user_id: UserId },
    ViewDequeSource { user_id: UserId },
    ViewCourseErrors { user_id: UserId },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use handlers::*;

    dotenvy::dotenv()?;
    pretty_env_logger::init();
    let bot = Bot::from_env();

    log::info!("Bot started");

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    tokio::spawn(event_handler::event_handler(bot.clone(), rx));

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

            let tx = tx.clone();
            let bot = bot.clone();
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
            tokio::spawn(async move {
                match update.kind {
                    UpdateKind::Message(message) => {
                        let Some(ref user) = message.from else {
                            log::warn!("Can't get user info from message {}", message.id);
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
                        log::debug!("user {user:?} sends message '{text}'.");
                        let (first_word, tail) = text.trim().split_once(" ").unwrap_or((text, ""));
                        match first_word {
                            "/help" => {
                                bot.send_message(message.chat.id, HELP_MESSAGE)
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/start" => {
                                // TODO: onboarding
                                bot.send_message(message.chat.id, "TODO: onboarding")
                                    .await
                                    .log_err()
                                    .unwrap();

                                bot.send_message(message.chat.id, HELP_MESSAGE)
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/card" => {
                                if tail.contains(" ") {
                                    bot.send_message(
                                        user.id,
                                        "Error: Card name should not contain spaces.",
                                    )
                                    .await
                                    .log_err()
                                    .unwrap();
                                    return;
                                }
                                if tail.is_empty() {
                                    bot.send_message(
                                        user.id,
                                        "Error: You should provide card name, you want to learn.",
                                    )
                                    .await
                                    .log_err()
                                    .unwrap();
                                    return;
                                }
                                tx.send(Event::PreviewCard {
                                    user_id: user.id,
                                    card_name: tail.to_owned(),
                                })
                                .await
                                .log_err()
                                .unwrap();
                            }
                            "/graph" => {
                                tx.send(Event::ViewGraph { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/revise" => {
                                bot.send_message(user.id, "This command is temporarily disabled")
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/clear" => {
                                tx.send(Event::Clear { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/change_course_graph" => {
                                tx.send(Event::ChangeCourseGraph { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/change_deque" => {
                                tx.send(Event::ChangeDeque { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/view_course_graph_source" => {
                                tx.send(Event::ViewCourseGraphSource { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/view_deque_source" => {
                                tx.send(Event::ViewDequeSource { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            "/view_course_errors" => {
                                tx.send(Event::ViewCourseErrors { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
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

                                            bot.delete_message(
                                                message.chat.id,
                                                current_message.unwrap(),
                                            )
                                            .await
                                            .log_err()
                                            .unwrap();

                                            answers.push(user_input);
                                            *current += 1;
                                            *current_id = rand::random();

                                            progress_on_user_event(bot, message.chat.id, state)
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
