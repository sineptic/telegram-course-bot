use std::{cmp::max, collections::BTreeMap, sync::LazyLock};

use anyhow::Context;
use course_graph::progress_store::TaskProgressStoreExt;
use dashmap::DashMap;
use graphviz_rust::{cmd::Format, printer::PrinterContext};
use teloxide_core::{
    RequestError,
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, UpdateKind},
};
use tokio::sync::Mutex;

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use state::State;

use crate::{
    event_handler::{
        course::{DEFAULT_COURSE_GRAPH, DEFAULT_DEQUE},
        get_progress, handle_event, syncronize,
    },
    handlers::{progress_on_user_event, send_interactions},
    interaction_types::TelegramInteraction,
    utils::ResultExt,
};
static STATE: LazyLock<DashMap<UserId, State>> = LazyLock::new(DashMap::new);

#[derive(Clone, Debug)]
#[allow(unused)]
enum Event {
    PreviewCard {
        user_id: UserId,
        course_id: CourseId,
        card_name: String,
    },
    ReviseCard {
        user_id: UserId,
        course_id: CourseId,
        card_name: String,
    },
    Revise {
        user_id: UserId,
    },
    Clear {
        user_id: UserId,
    },
    ChangeCourseGraph {
        user_id: UserId,
        course_id: CourseId,
    },
    ChangeDeque {
        user_id: UserId,
        course_id: CourseId,
    },
}

use database::*;
mod database {
    pub static COURSES_STORAGE: LazyLock<Mutex<Courses>> = LazyLock::new(|| {
        Mutex::new(Courses {
            next_course_id: 0,
            data: BTreeMap::new(),
            owners_index: BTreeMap::new(),
        })
    });

    use std::{collections::BTreeMap, sync::LazyLock};

    use course_graph::graph::CourseGraph;
    use teloxide_core::types::UserId;
    use tokio::sync::Mutex;

    use crate::{
        event_handler::{course::DEFAULT_COURSE_GRAPH, progress_store::UserProgress},
        interaction_types::deque::Deque,
    };

    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
    pub struct CourseId(pub u64);
    #[derive(Clone)]
    pub struct Course {
        pub owner_id: UserId,
        pub structure: CourseGraph,
        pub tasks: Deque,
    }
    pub struct Courses {
        next_course_id: u64,
        data: BTreeMap<CourseId, Course>,
        owners_index: BTreeMap<UserId, Vec<CourseId>>,
    }
    impl Courses {
        pub fn insert(&mut self, course: Course) -> CourseId {
            let course_id = CourseId(self.next_course_id);
            self.next_course_id += 1;
            self.owners_index
                .entry(course.owner_id)
                .or_default()
                .push(course_id);
            self.data.insert(course_id, course);
            course_id
        }
        pub fn get_course(&self, id: CourseId) -> Option<&Course> {
            self.data.get(&id)
        }
        pub fn get_course_mut(&mut self, id: CourseId) -> Option<&mut Course> {
            self.data.get_mut(&id)
        }
        pub fn select_courses(&self, owner: UserId) -> Option<Vec<&Course>> {
            self.owners_index.get(&owner).map(|course_ids| {
                course_ids
                    .iter()
                    .map(|course_id| self.data.get(course_id).unwrap())
                    .collect::<Vec<_>>()
            })
        }
    }
    impl Courses {
        pub fn partial_serialize(&self) -> (u64, Vec<(CourseId, Course)>) {
            (
                self.next_course_id,
                self.data
                    .iter()
                    .map(|(id, value)| (*id, value.clone()))
                    .collect::<Vec<_>>(),
            )
        }
        pub fn partial_deserialize(next_course_id: u64, courses: Vec<(CourseId, Course)>) -> Self {
            let mut owners_index: BTreeMap<UserId, Vec<CourseId>> = BTreeMap::new();
            let mut data = BTreeMap::new();
            for (id, course) in courses {
                owners_index.entry(course.owner_id).or_default().push(id);
                assert!(data.insert(id, course).is_none());
            }
            Self {
                next_course_id,
                owners_index,
                data,
            }
        }
    }
    impl Course {
        pub fn default_user_progress(&self) -> UserProgress {
            let mut user_progress = UserProgress::default();
            self.structure.init_store(&mut user_progress);
            user_progress
        }
        pub fn get_errors(&self) -> Option<Vec<String>> {
            let deque = &self.tasks;
            let course_graph = &self.structure;
            let mut errors = Vec::new();

            course_graph
                .cards()
                .keys()
                .filter(|&id| !deque.tasks.contains_key(id))
                .map(|id| format!("Graph has '{id}' card, but deque(cards.md) doesn't."))
                .for_each(|item| errors.push(item));
            deque
                .tasks
                .keys()
                .filter(|x| !DEFAULT_COURSE_GRAPH.cards().contains_key(*x))
                .map(|err| format!("Deque(cards.md) has '{err}', but graph doesn't."))
                .for_each(|item| {
                    errors.push(item);
                });

            if errors.is_empty() {
                None
            } else {
                Some(errors)
            }
        }
    }
}

#[tokio::main]
async fn main() {
    use handlers::*;

    dotenvy::dotenv().expect("'TELOXIDE_TOKEN' variable should be specified in '.env' file");
    pretty_env_logger::init();
    let bot = Bot::from_env();

    log::info!("Bot started");

    let mut offset = 0;
    loop {
        let updates = bot
            .get_updates()
            .offset((offset + 1).try_into().unwrap())
            .timeout(30)
            .send()
            .await;
        let updates = match updates {
            Ok(x) => x,
            Err(err) => match err {
                RequestError::Network(error) if error.is_timeout() => {
                    log::debug!("Telegram connection timed out.");
                    continue;
                }
                other_error => {
                    log::error!(
                        "Error while connection to telegram to receive updates: {other_error}."
                    );
                    continue;
                }
            },
        };
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

async fn handle_message(bot: Bot, message: Message) -> anyhow::Result<()> {
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
        "/create_course" => {
            log::info!(
                "user {}({}) sends create_course command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let mut courses = COURSES_STORAGE.lock().await;
            let id = courses.insert(Course {
                owner_id: user.id,
                structure: DEFAULT_COURSE_GRAPH.clone(),
                tasks: DEFAULT_DEQUE.clone(),
            });
            bot.send_message(user.id, format!("Course created with id {}", id.0))
                .await?;
        }
        "/card" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
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
                    course_id,
                    card_name: tail.to_owned(),
                },
            )
            .await?;
        }
        "/graph" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
            log::info!(
                "user {}({}) sends graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            syncronize(user.id, course_id);

            let mut graph = COURSES_STORAGE
                .lock()
                .await
                .get_course(course_id)
                .unwrap()
                .structure
                .generate_structure_graph();

            get_progress(course_id, user.id)
                .await
                .generate_stmts()
                .into_iter()
                .for_each(|stmt| {
                    graph.add_stmt(stmt);
                });

            send_interactions(
                bot,
                user.id,
                [TelegramInteraction::PersonalImage(
                    tokio::task::spawn_blocking(move || {
                        graphviz_rust::exec(
                            graph,
                            &mut PrinterContext::default(),
                            Vec::from([Format::Jpeg.into()]),
                        )
                        .context("Failed to run 'dot'")
                    })
                    .await
                    .unwrap()?,
                )],
            )
            .await?;
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
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
            log::info!(
                "user {}({}) sends change_course_graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(
                bot,
                Event::ChangeCourseGraph {
                    user_id: user.id,
                    course_id,
                },
            )
            .await?;
        }
        "/change_deque" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
            log::info!(
                "user {}({}) sends change_deque command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            handle_event(
                bot,
                Event::ChangeDeque {
                    user_id: user.id,
                    course_id,
                },
            )
            .await?;
        }
        "/view_course_graph_source" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
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
                        COURSES_STORAGE
                            .lock()
                            .await
                            .get_course(course_id)
                            .unwrap()
                            .structure
                            .get_source()
                    )
                    .into(),
                ],
            )
            .await?;
        }
        "/view_deque_source" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
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
                        COURSES_STORAGE
                            .lock()
                            .await
                            .get_course(course_id)
                            .unwrap()
                            .tasks
                            .source
                            .to_owned()
                    )
                    .into(),
                ],
            )
            .await?;
        }
        "/view_course_errors" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(u64::from_str_radix(first_word, 10).unwrap());
            log::info!(
                "user {}({}) sends view_course_errors command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if let Some(errors) = COURSES_STORAGE
                .lock()
                .await
                .get_course(course_id)
                .unwrap()
                .get_errors()
            {
                let mut msgs = Vec::new();
                msgs.push("Errors:".into());
                for error in errors {
                    msgs.push(error.into());
                }
                send_interactions(bot, user.id, msgs).await?;
            } else {
                send_interactions(bot, user.id, vec!["No errors!".into()]).await?;
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
                            .log_err();

                        answers.push(user_input);
                        *current += 1;
                        *current_id = rand::random();

                        progress_on_user_event(
                            bot,
                            message
                                .from
                                .ok_or(anyhow::anyhow!("Message should contain user id"))?
                                .id,
                            state,
                        )
                        .await
                        .log_err()
                        .unwrap();
                    }
                    _ => {
                        bot.send_message(message.chat.id, "Unexpected input")
                            .await?;
                    }
                },
                State::Idle => {
                    bot.send_message(message.chat.id, "Command not found!")
                        .await?;
                }
            }
        }
    }
    Ok(())
}
