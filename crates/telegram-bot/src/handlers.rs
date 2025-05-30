use teloxide::{
    dispatching::dialogue::GetChatId,
    types::{InputFile, ParseMode},
};
use tokio::sync::oneshot;

use super::{commands::Command, inline_keyboard::make_keyboard, state::State, *};
use crate::interaction_types::TelegramInteraction;

pub type HandleResult = Result<(), Box<dyn Error + Send + Sync>>;

pub async fn message_handler(bot: Bot, msg: Message, me: Me, events: EventSender) -> HandleResult {
    let Some(chat_id) = msg.chat_id() else {
        log::warn!("Unexpected chat ID");
        return Ok(());
    };
    let Some(user_id) = chat_id.as_user() else {
        bot.send_message(chat_id, "Only users can answer").await?;
        return Ok(());
    };

    let Some(text) = msg.text() else {
        bot.send_message(chat_id, "Message should contain text")
            .await?;
        return Ok(());
    };

    match BotCommands::parse(text, me.username()) {
        Ok(Command::Help) => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Ok(Command::Start) => {
            // TODO: onboarding
            bot.send_message(chat_id, "TODO: onboarding").await?;

            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Ok(Command::Card(card_name)) => {
            events
                .send(Event::PreviewCard { user_id, card_name })
                .await?;
        }
        Ok(Command::Graph) => {
            events.send(Event::ViewGraph { user_id }).await?;
        }
        // Ok(Command::Revise) => {
        //     events.send(Event::Revise { user_id }).await?;
        // }
        Ok(Command::Clear) => {
            events.send(Event::Clear { user_id }).await?;
        }
        Ok(Command::ChangeCourseGraph) => {
            events.send(Event::ChangeCourseGraph { user_id }).await?;
        }
        Ok(Command::ChangeDeque) => {
            events.send(Event::ChangeDeque { user_id }).await?;
        }
        Ok(Command::ViewCourseGraphSource) => {
            events
                .send(Event::ViewCourseGraphSource { user_id })
                .await?;
        }
        Ok(Command::ViewDequeSource) => {
            events.send(Event::ViewDequeSource { user_id }).await?;
        }
        Ok(Command::ViewCourseErrors) => {
            events.send(Event::ViewCourseErrors { user_id }).await?;
        }

        Err(_) => {
            let mut state = STATE.entry(user_id).or_default();
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
                        let user_input = msg.text().unwrap().to_owned();

                        bot.delete_message(msg.chat_id().unwrap(), current_message.unwrap())
                            .await?;

                        answers.push(user_input);
                        *current += 1;
                        *current_id = rand::random();

                        progress_on_user_event(bot, chat_id, state).await?;
                    }
                    _ => {
                        bot.send_message(msg.chat.id, "Unexpected input").await?;
                    }
                },
                State::Idle => {
                    bot.send_message(msg.chat.id, "Command not found!").await?;
                }
            }
        }
    }

    Ok(())
}

pub async fn send_interactions(
    bot: Bot,
    user_id: UserId,
    interactions: Vec<TelegramInteraction>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async {
        let _ = rx.await;
    });
    set_task_for_user(bot, user_id, interactions, tx).await
}

pub async fn set_task_for_user(
    bot: Bot,
    user_id: UserId,
    interactions: Vec<TelegramInteraction>,
    channel: oneshot::Sender<Vec<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut state = STATE.entry(user_id).or_default();

    *state = State::UserEvent {
        interactions,
        current: 0,
        current_id: rand::random(),
        current_message: None,
        answers: Vec::new(),
        channel: Some(channel),
    };

    progress_on_user_event(bot, user_id.into(), state.value_mut()).await?;
    Ok(())
}

pub async fn callback_handler(bot: Bot, q: CallbackQuery) -> HandleResult {
    let Some(chat_id) = q.chat_id() else {
        log::warn!("can't get chat id");
        return Ok(());
    };
    let Some(user_id) = chat_id.as_user() else {
        bot.send_message(chat_id, "Only users can answer").await?;
        return Ok(());
    };

    let _ = bot.answer_callback_query(&q.id).await;

    let Some(mut state) = STATE.get_mut(&user_id) else {
        log::debug!("user {user_id} not in dialogue");
        return Ok(());
    };
    let state = state.value_mut();
    let State::UserEvent {
        interactions: _,
        current,
        current_id,
        current_message,
        answers,
        channel: _,
    } = state
    else {
        log::debug!("user {user_id} in different state");
        bot.send_message(chat_id, "You can answer only to current question")
            .await?;
        return Ok(());
    };

    let Some(response) = q.data else {
        log::error!("reponse data should be assigned");
        return Ok(());
    };

    let whitespace = response.find(' ').unwrap();
    let (rand_id, response) = response.split_at(whitespace);
    let response = &response[1..];

    if rand_id != current_id.to_string() {
        log::debug!("user {user_id} answer to previous question");
        // TODO: maybe delete this message
        bot.send_message(chat_id, "You can answer only to current question")
            .await?;
        return Ok(());
    }

    bot.edit_message_text(
        chat_id,
        current_message.unwrap(),
        format!("You answer: {response}",),
    )
    .await?;

    answers.push(response.to_owned());
    *current += 1;

    progress_on_user_event(bot, chat_id, state).await?;

    Ok(())
}

pub async fn progress_on_user_event(
    bot: Bot,
    chat_id: ChatId,
    state: &mut State,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let State::UserEvent {
        interactions,
        current,
        current_id,
        current_message,
        answers,
        channel,
    } = state
    else {
        panic!("Unexpected state");
    };
    loop {
        let len = interactions.len();
        if *current >= len {
            channel.take().unwrap().send(answers.clone()).unwrap();
            *state = State::Idle;
            break;
        }
        match &interactions[*current] {
            TelegramInteraction::OneOf(vec) => {
                *current_id = rand::random();
                let message = bot
                    .send_message(chat_id, "choose answer")
                    .reply_markup(make_keyboard(vec, *current_id))
                    .await?;

                *current_message = Some(message.id);
                break;
            }
            TelegramInteraction::Text(text) => {
                bot.send_message(chat_id, text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
            TelegramInteraction::UserInput => {
                let message = bot.send_message(chat_id, "Please enter your input").await?;

                *current_message = Some(message.id);
                *current_id = rand::random();
                break;
            }
            TelegramInteraction::Image(link) => {
                bot.send_photo(chat_id, InputFile::url(link.clone()))
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
            TelegramInteraction::PersonalImage(bytes) => {
                // FIXME: don't clone bytes(image)
                bot.send_photo(chat_id, InputFile::memory(bytes.clone()))
                    .await?;
                *current += 1;
                answers.push(String::new());
            }
        }
    }
    Ok(())
}
