use teloxide::{dispatching::dialogue::GetChatId, types::MessageId};

use super::{
    commands::Command,
    inline_keyboard::make_keyboard,
    state::{MyDialogue, State},
    *,
};
pub type HandleResult = Result<(), Box<dyn Error + Send + Sync>>;

pub async fn message_handler(bot: Bot, dialogue: MyDialogue, msg: Message, me: Me) -> HandleResult {
    if let Some(text) = msg.text() {
        match BotCommands::parse(text, me.username()) {
            Ok(Command::Help) => {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
            }
            Ok(Command::Start) => {
                let interactions = vec![
                    TelegramInteraction::Text("2 * 3 = ".into()),
                    TelegramInteraction::OneOf(vec![5.to_string(), 6.to_string(), 7.to_string()]),
                    TelegramInteraction::Text("7 - 5 = ".into()),
                    TelegramInteraction::UserInput,
                ];
                let event = State::UserEvent {
                    interactions: interactions.clone(),
                    current: 0,
                    current_id: rand::random(),
                    current_message: None,
                    answers: Vec::new(),
                };
                dialogue.update(event).await?;
                progress_on_user_event(bot, dialogue, 0).await?;
            }

            Err(_) => match dialogue.get().await?.unwrap() {
                State::UserEvent {
                    interactions,
                    mut current,
                    mut current_id,
                    current_message,
                    mut answers,
                } => match &interactions[current] {
                    TelegramInteraction::UserInput => {
                        let user_input = msg.text().unwrap().to_owned();

                        bot.delete_message(msg.chat_id().unwrap(), current_message.unwrap())
                            .await?;

                        answers.push(user_input);
                        current += 1;
                        current_id = rand::random();
                        let event = State::UserEvent {
                            interactions,
                            current,
                            current_id,
                            current_message,
                            answers,
                        };
                        dialogue.update(event).await?;
                        progress_on_user_event(bot, dialogue, current).await?;
                    }
                    _ => {
                        bot.send_message(msg.chat.id, "Unexpected input").await?;
                    }
                },
                State::General => {
                    bot.send_message(msg.chat.id, "Command not found!").await?;
                }
            },
        }
    }

    Ok(())
}

pub async fn callback_handler(
    bot: Bot,
    dialogue: MyDialogue,
    (interactions, mut current, current_id, current_message, mut answers): (
        Vec<TelegramInteraction>,
        usize,
        u64,
        Option<MessageId>,
        Vec<String>,
    ),
    q: CallbackQuery,
) -> HandleResult {
    if let Some(ref response) = q.data {
        bot.answer_callback_query(&q.id).await?;

        let response = response.split_whitespace().collect::<Vec<_>>();

        if response[0] != current_id.to_string() {
            log::warn!("user answer to previous question");
            return Ok(());
        }

        let TelegramInteraction::OneOf(current_choice) = &interactions[current] else {
            todo!();
        };

        bot.edit_message_text(
            q.chat_id().unwrap(),
            current_message.unwrap(),
            format!(
                "You choose: {}",
                current_choice[response[1].parse::<usize>().unwrap()]
            ),
        )
        .await?;

        answers.push(response[1].to_string());
        current += 1;
        dialogue
            .update(State::UserEvent {
                interactions,
                current,
                current_id,
                current_message,
                answers,
            })
            .await?;

        progress_on_user_event(bot, dialogue, current).await?;

        log::info!("You chose: {}", response[1]);
    }

    Ok(())
}

pub async fn progress_on_user_event(
    bot: Bot,
    dialogue: Dialogue<State, InMemStorage<State>>,
    mut current: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let State::UserEvent {
        interactions,
        current: _,
        current_id: _,
        current_message: _,
        mut answers,
    } = dialogue.get().await?.unwrap()
    else {
        panic!("Unexpected state");
    };
    loop {
        let len = interactions.len();
        if current >= len {
            bot.send_message(
                dialogue.chat_id(),
                format!(
                    "Your answers:\n{answers:#?}
correct_answers: [
    \"\",
    \"1\",
    \"\",
    \"2\",
]",
                ),
            )
            .await?;
            log::error!("Handling task end not yet implemented");
            dialogue.exit().await?;
            break;
        }
        match &interactions[current] {
            TelegramInteraction::OneOf(vec) => {
                let current_id = rand::random();
                let message = bot
                    .send_message(dialogue.chat_id(), "ㅤ")
                    .reply_markup(make_keyboard(vec, current_id))
                    .await?;

                dialogue
                    .update(State::UserEvent {
                        interactions,
                        current,
                        current_id,
                        current_message: Some(message.id),
                        answers,
                    })
                    .await?;
                break;
            }
            TelegramInteraction::Text(text) => {
                bot.send_message(dialogue.chat_id(), text).await?;
                current += 1;
                answers.push(String::new());
            }
            TelegramInteraction::UserInput => {
                let message = bot
                    .send_message(dialogue.chat_id(), "Please enter your input")
                    .await?;

                dialogue
                    .update(State::UserEvent {
                        interactions,
                        current,
                        current_id: rand::random(),
                        current_message: Some(message.id),
                        answers,
                    })
                    .await?;
                break;
            }
        }
    }
    Ok(())
}
