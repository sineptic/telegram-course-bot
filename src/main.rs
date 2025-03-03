#![feature(async_fn_traits)]

use std::{collections::HashMap, error::Error, sync::LazyLock};

use telegram_interactions::TelegramInteraction;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me},
    utils::command::BotCommands,
};
use tokio::sync::Mutex;

mod commands;
mod handlers;
mod inline_keyboard;
mod state;

use state::State;
static STATE: LazyLock<Mutex<HashMap<UserId, State>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use handlers::*;

    pretty_env_logger::init();
    log::info!("Starting buttons bot...");

    let bot = Bot::from_env();

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
