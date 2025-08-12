use std::{cmp::max, error::Error, sync::LazyLock};

use dashmap::DashMap;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, UpdateKind},
    utils::command::BotCommands,
};

mod commands;
mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use state::State;

use crate::{commands::Command, interaction_types::TelegramInteraction, utils::ResultExt};
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
    log::info!("Starting buttons bot...");

    let bot = Bot::from_env();

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

                        match BotCommands::parse(
                            text,
                            bot.get_me().await.log_err().unwrap().username(),
                        ) {
                            Ok(Command::Help) => {
                                bot.send_message(
                                    message.chat.id,
                                    Command::descriptions().to_string(),
                                )
                                .await
                                .log_err()
                                .unwrap();
                            }
                            Ok(Command::Start) => {
                                // TODO: onboarding
                                bot.send_message(message.chat.id, "TODO: onboarding")
                                    .await
                                    .log_err()
                                    .unwrap();

                                bot.send_message(
                                    message.chat.id,
                                    Command::descriptions().to_string(),
                                )
                                .await
                                .log_err()
                                .unwrap();
                            }
                            Ok(Command::Card(card_name)) => {
                                tx.send(Event::PreviewCard {
                                    user_id: user.id,
                                    card_name,
                                })
                                .await
                                .log_err()
                                .unwrap();
                            }
                            Ok(Command::Graph) => {
                                tx.send(Event::ViewGraph { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            // Ok(Command::Revise) => {
                            //     tx.send(Event::Revise { user_id: user.id }).await.log_err().unwrap();
                            // }
                            Ok(Command::Clear) => {
                                tx.send(Event::Clear { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            Ok(Command::ChangeCourseGraph) => {
                                tx.send(Event::ChangeCourseGraph { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            Ok(Command::ChangeDeque) => {
                                tx.send(Event::ChangeDeque { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            Ok(Command::ViewCourseGraphSource) => {
                                tx.send(Event::ViewCourseGraphSource { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            Ok(Command::ViewDequeSource) => {
                                tx.send(Event::ViewDequeSource { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }
                            Ok(Command::ViewCourseErrors) => {
                                tx.send(Event::ViewCourseErrors { user_id: user.id })
                                    .await
                                    .log_err()
                                    .unwrap();
                            }

                            Err(_) => {
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
