#![feature(async_fn_traits)]

use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, LazyLock},
};

use telegram_interactions::TelegramInteraction;
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me},
    utils::command::BotCommands,
};
use tokio::sync::Mutex;

mod commands;
mod event_handler;
mod handlers;
mod inline_keyboard;
mod state;
mod utils;

use state::State;
static STATE: LazyLock<Mutex<HashMap<UserId, State>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// const SINEPTIC_TELEGRAM_ID: UserId = UserId(1120774849);

#[derive(Clone, Debug)]
enum Event {
    StartInteraction(UserId),
}
type EventSender = Arc<tokio::sync::mpsc::Sender<Event>>;
type EventReceiver = tokio::sync::mpsc::Receiver<Event>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use handlers::*;

    pretty_env_logger::init();
    log::info!("Starting buttons bot...");

    let bot = Bot::from_env();

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let tx = Arc::new(tx);
    tokio::spawn(event_handler::event_handler(bot.clone(), rx));

    let handler = dptree::entry()
        .branch(
            Update::filter_message().endpoint(move |bot: Bot, msg: Message, me: Me| {
                message_handler(bot, msg, me, tx.clone())
            }),
        )
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
