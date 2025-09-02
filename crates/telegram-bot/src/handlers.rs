use rand::seq::SliceRandom;
use teloxide_core::types::{CallbackQuery, InputFile, ParseMode};
use tokio::sync::oneshot;

use super::*;
use crate::{interaction_types::TelegramInteraction, state::UserInteraction};

pub async fn send_interactions(
    bot: Bot,
    user_id: UserId,
    interactions: impl IntoIterator<Item = TelegramInteraction>,
) -> anyhow::Result<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async {
        let _ = rx.await;
    });
    set_task_for_user(bot, user_id, interactions.into_iter().collect(), tx).await
}

pub async fn set_task_for_user(
    bot: Bot,
    user_id: UserId,
    interactions: Vec<TelegramInteraction>,
    channel: oneshot::Sender<Vec<String>>,
) -> anyhow::Result<()> {
    let mut user_state = STATE.entry(user_id).or_default();

    user_state.current_interaction = Some(UserInteraction {
        interactions,
        current: 0,
        current_id: rand::random(),
        current_message: None,
        answers: Vec::new(),
        channel: Some(channel),
    });

    progress_on_user_event(bot, user_id, &mut user_state.current_interaction).await?;
    Ok(())
}

pub async fn callback_handler(bot: Bot, q: CallbackQuery) -> anyhow::Result<()> {
    {
        let CallbackQuery { id, from, data, .. } = &q;
        log::debug!("get callback query, 'id: {id}, from: {from:?}, data: {data:?}'");
    }
    let user_id = q.from.id;
    let Some(response) = q.data else {
        log::error!("reponse data should be assigned");
        return Ok(());
    };

    let _ = bot.answer_callback_query(q.id).await;

    let Some(mut user_state) = STATE.get_mut(&user_id) else {
        log::debug!("user {user_id} not in dialogue");
        return Ok(());
    };
    let Some(UserInteraction {
        current,
        current_id,
        current_message,
        answers,
        ..
    }) = &mut user_state.current_interaction
    else {
        log::warn!("user {:?} in different state", q.from);
        bot.send_message(user_id, "You can answer only to current question")
            .await?;
        return Ok(());
    };

    let whitespace = response.find(' ').unwrap();
    let (rand_id, response) = response.split_at(whitespace);
    let response = &response[1..];

    if rand_id != current_id.to_string() {
        log::info!("user {:?} answer to previous question", q.from);
        // TODO: maybe delete this message
        bot.send_message(user_id, "You can answer only to current question")
            .await?;
        return Ok(());
    }

    bot.edit_message_text(
        user_id,
        current_message.unwrap(),
        format!("You answer: {response}"),
    )
    .await?;

    answers.push(response.to_owned());
    *current += 1;

    progress_on_user_event(bot, user_id, &mut user_state.current_interaction).await?;

    Ok(())
}

pub async fn progress_on_user_event(
    bot: Bot,
    user_id: UserId,
    current_user_interaction: &mut Option<UserInteraction>,
) -> anyhow::Result<()> {
    let Some(UserInteraction {
        interactions,
        current,
        current_id,
        current_message,
        answers,
        channel,
    }) = current_user_interaction
    else {
        log::error!("unexpected idle state");
        panic!("Unexpected state");
    };
    loop {
        if *current >= interactions.len() {
            channel.take().unwrap().send(answers.clone()).unwrap();
            *current_user_interaction = None;
            break;
        }
        match &interactions[*current] {
            TelegramInteraction::OneOf(vec) => {
                *current_id = rand::random();
                let mut labels = vec.clone();
                labels.shuffle(&mut rand::rng());
                let keyboard = InlineKeyboardMarkup::new(labels.iter().map(|label| {
                    [InlineKeyboardButton::callback(
                        label,
                        format!("{current_id} {label}"),
                    )]
                }));
                let message = bot
                    .send_message(user_id, "choose answer")
                    .reply_markup(keyboard)
                    .await?;

                *current_message = Some(message.id);
                break;
            }
            TelegramInteraction::Text(text) => {
                bot.send_message(user_id, text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
            TelegramInteraction::UserInput => {
                let message = bot.send_message(user_id, "Please enter your input").await?;

                *current_message = Some(message.id);
                *current_id = rand::random();
                break;
            }
            TelegramInteraction::Image(link) => {
                bot.send_photo(user_id, InputFile::url(link.clone()))
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
            TelegramInteraction::PersonalImage(bytes) => {
                // FIXME: don't clone bytes(image)
                bot.send_photo(user_id, InputFile::memory(bytes.clone()))
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
        }
    }
    Ok(())
}
