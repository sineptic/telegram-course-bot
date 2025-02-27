use std::error::Error;

use state::MyDialogue;
use telegram_interactions::TelegramInteraction;
use teloxide::{
    dispatching::dialogue::InMemStorage,
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Me},
    utils::command::BotCommands,
};

mod commands;
mod handlers;
mod inline_keyboard;
mod state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use handlers::*;
    use state::State;

    pretty_env_logger::init();
    log::info!("Starting buttons bot...");

    let bot = Bot::from_env();

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .enter_dialogue::<Message, InMemStorage<State>, State>()
                .endpoint(message_handler),
        )
        .branch(
            Update::filter_callback_query()
                .enter_dialogue::<CallbackQuery, InMemStorage<State>, State>()
                .branch(
                    dptree::case![State::UserEvent {
                        interactions,
                        current,
                        current_id,
                        current_message,
                        answers,
                        channel
                    }]
                    .endpoint(callback_handler),
                )
                .branch(dptree::case![State::General].endpoint(unexpected_callback_query)),
        );
    async fn unexpected_callback_query(
        bot: Bot,
        dialogue: MyDialogue,
        _q: CallbackQuery,
    ) -> HandleResult {
        bot.answer_callback_query(&_q.id).await?;
        bot.send_message(
            dialogue.chat_id(),
            "got unexpected callback query. run '/help' for help",
        )
        .await?;
        Ok(())
    }

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![InMemStorage::<State>::new()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
