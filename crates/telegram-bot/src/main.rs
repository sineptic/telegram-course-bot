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

use crate::utils::ResultExt;
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
type EventSender = tokio::sync::mpsc::Sender<Event>;
type EventReceiver = tokio::sync::mpsc::Receiver<Event>;

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
            .timeout(30)
            .send()
            .await?;
        for update in updates {
            offset = max(offset, update.id.0);

            let tx = tx.clone();
            let bot = bot.clone();
            tokio::spawn(async move {
                match update.kind {
                    UpdateKind::Message(message) => {
                        message_handler(bot, message, tx).await.log_err();
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
