#![allow(unused)]
use std::{cmp::max, sync::LazyLock};

use anyhow::Context;
use course_graph::{graph::CourseGraph, progress_store::TaskProgressStoreExt};
use dashmap::DashMap;
use graphviz_rust::{cmd::Format, printer::PrinterContext};
use teloxide_core::{
    RequestError,
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, Update, UpdateKind, User},
};

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use crate::{
    event_handler::{handle_event, syncronize},
    handlers::{callback_handler, progress_on_user_event, send_interactions},
    interaction_types::{TelegramInteraction, deque::Deque},
    state::{UserInteraction, UserState},
    utils::ResultExt,
};
static STATE: LazyLock<DashMap<UserId, UserState>> = LazyLock::new(DashMap::new);

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
    pub static STORAGE: LazyLock<CoursesWrapper> = LazyLock::new(|| CoursesWrapper {
        inner: Mutex::new(Courses {
            next_course_id: 0,
            courses: BTreeMap::new(),
            courses_owners_index: BTreeMap::new(),
            progress: HashMap::new(),
        }),
    });

    use std::{
        collections::{BTreeMap, HashMap, btree_map::Entry},
        ops::{Deref, DerefMut},
        rc::Rc,
        sync::{Arc, LazyLock, Mutex, MutexGuard},
    };

    use course_graph::graph::CourseGraph;
    use teloxide_core::types::UserId;

    use crate::{event_handler::progress_store::UserProgress, interaction_types::deque::Deque};

    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
    pub struct CourseId(pub u64);
    #[derive(Clone)]
    pub struct Course {
        pub owner_id: UserId,
        pub structure: CourseGraph,
        pub tasks: Deque,
    }
    pub struct CoursesWrapper {
        inner: Mutex<Courses>,
    }
    impl CoursesWrapper {
        fn inner(&self) -> MutexGuard<'_, Courses> {
            self.inner.lock().unwrap_or_else(|err| {
                log::error!("Some thread panicked while holding mutex");
                err.into_inner()
            })
        }
        pub fn insert(&self, course: Course) -> CourseId {
            self.inner().insert(course)
        }
        pub fn get_course(&self, cousre_id: CourseId) -> Option<Arc<Course>> {
            self.inner().get_course(cousre_id)
        }
        /// Returns whether course already exists.
        pub fn set_course(&self, course_id: CourseId, value: Course) -> bool {
            self.inner().set_course(course_id, value)
        }
        pub fn select_courses_by_owner(&self, owner: UserId) -> Option<Vec<Arc<Course>>> {
            self.inner().select_courses_by_owner(owner)
        }
        pub fn list_user_courses(&self, user_id: UserId) -> Option<Vec<CourseId>> {
            self.inner().list_user_courses(user_id)
        }
        /// Returns None if there is no course with this id.
        pub fn get_progress(
            &self,
            user_id: UserId,
            course_id: CourseId,
        ) -> Option<Arc<UserProgress>> {
            self.inner().get_progress(user_id, course_id)
        }
        /// Returns false if course doesn't exists or already tracked to user
        pub fn add_course_to_user(&self, user_id: UserId, course_id: CourseId) -> bool {
            self.inner().add_course_to_user(user_id, course_id)
        }
        /// Returns None if this progress doesn't exists.
        pub fn set_course_progress(
            &self,
            user_id: UserId,
            course_id: CourseId,
            progress: UserProgress,
        ) -> Option<()> {
            self.inner()
                .set_course_progress(user_id, course_id, progress)
        }
        pub fn delete_user_progress(&self, user_id: UserId) {
            self.inner().delete_user_progress(user_id);
        }
    }
    struct Courses {
        next_course_id: u64,
        courses: BTreeMap<CourseId, Arc<Course>>,
        courses_owners_index: BTreeMap<UserId, Vec<CourseId>>,
        progress: HashMap<UserId, BTreeMap<CourseId, Arc<UserProgress>>>,
    }
    impl Courses {
        fn insert(&mut self, course: Course) -> CourseId {
            let course_id = CourseId(self.next_course_id);
            self.next_course_id += 1;
            self.courses_owners_index
                .entry(course.owner_id)
                .or_default()
                .push(course_id);
            self.courses.insert(course_id, course.into());
            course_id
        }
        fn get_course(&self, id: CourseId) -> Option<Arc<Course>> {
            self.courses.get(&id).cloned()
        }
        /// Returns whether course already exists.
        fn set_course(&mut self, id: CourseId, content: Course) -> bool {
            self.courses.insert(id, content.into()).is_some()
        }
        fn select_courses_by_owner(&self, owner: UserId) -> Option<Vec<Arc<Course>>> {
            self.courses_owners_index.get(&owner).map(|course_ids| {
                course_ids
                    .iter()
                    .map(|course_id| self.courses.get(course_id).unwrap().clone())
                    .collect::<Vec<_>>()
            })
        }
        fn list_user_courses(&self, user: UserId) -> Option<Vec<CourseId>> {
            self.progress
                .get(&user)
                .map(|list| list.keys().copied().collect())
        }
        /// Returns None if there is no course with this id.
        fn get_progress(&mut self, user: UserId, course: CourseId) -> Option<Arc<UserProgress>> {
            let def = self.get_course(course)?.default_user_progress();
            Some(
                self.progress
                    .entry(user)
                    .or_default()
                    .entry(course)
                    .or_insert(def.into())
                    .clone(),
            )
        }
        /// Returns false if course doesn't exists or already tracked to user
        fn add_course_to_user(&mut self, user_id: UserId, course_id: CourseId) -> bool {
            let Some(course) = self.get_course(course_id) else {
                return false;
            };
            let entry = self.progress.entry(user_id).or_default().entry(course_id);
            match entry {
                Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(Arc::new(course.default_user_progress()));
                    true
                }
                Entry::Occupied(occupied_entry) => false,
            }
        }
        /// Returns None if this progress doesn't exists.
        fn set_course_progress(
            &mut self,
            user_id: UserId,
            course_id: CourseId,
            progress: UserProgress,
        ) -> Option<()> {
            *self.progress.get_mut(&user_id)?.get_mut(&course_id)? = Arc::new(progress);
            Some(())
        }
        pub fn delete_user_progress(&mut self, user: UserId) {
            self.progress.remove(&user);
        }
    }
    impl Courses {
        pub fn partial_serialize(&self) -> (u64, Vec<(CourseId, Course)>) {
            todo!()
            // (
            //     self.next_course_id,
            //     self.data
            //         .iter()
            //         .map(|(id, value)| (*id, value.clone()))
            //         .collect::<Vec<_>>(),
            // )
        }
        pub fn partial_deserialize(next_course_id: u64, courses: Vec<(CourseId, Course)>) -> Self {
            todo!()
            // let mut owners_index: BTreeMap<UserId, Vec<CourseId>> = BTreeMap::new();
            // let mut data = BTreeMap::new();
            // for (id, course) in courses {
            //     owners_index.entry(course.owner_id).or_default().push(id);
            //     assert!(data.insert(id, course).is_none());
            // }
            // Self {
            //     next_course_id,
            //     owners_index,
            //     data,
            // }
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
                .filter(|x| !CourseGraph::default().cards().contains_key(*x))
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
                    log::trace!("Telegram connection timed out.");
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
            tokio::spawn(update_handler(bot, update));
        }
    }
}

async fn update_handler(bot: Bot, update: Update) {
    match update.kind {
        UpdateKind::Message(message) => {
            let Some(ref user) = message.from else {
                log::warn!("Can't get user info from message {}", message.id);
                bot.send_message(message.chat.id, "Bot works only with users")
                    .await
                    .log_err();
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
            log::trace!("user {user:?} sends message '{text}'.");
            handle_message(bot, user, text).await.log_err();
        }
        UpdateKind::CallbackQuery(callback_query) => {
            callback_handler(bot, callback_query).await.log_err();
        }
        _ => todo!(),
    };
}

async fn handle_message(bot: Bot, user: &User, message: &str) -> anyhow::Result<()> {
    static HELP_MESSAGE: &str = "
/create_course - Create new course and get it's ID
/card COURSE_ID CARD_NAME — Try to complete card
/graph COURSE_ID— View course structure
/help — Display all commands
/clear — Reset your state to default(clear all progress)
/change_course_graph COURSE_ID
/change_deque COURSE_ID
/view_course_graph_source COURSE_ID
/view_deque_source COURSE_ID
/view_course_errors COURSE_ID
";

    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log::info!(
                "user {}({}) sends help command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            bot.send_message(user.id, HELP_MESSAGE).await?;
        }
        "/start" => {
            log::info!(
                "user {}({}) sends start command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            // TODO: onboarding
            bot.send_message(user.id, "TODO: onboarding").await?;

            bot.send_message(user.id, HELP_MESSAGE).await?;
        }
        "/create_course" => {
            log::info!(
                "user {}({}) sends create_course command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let id = STORAGE.insert(Course {
                owner_id: user.id,
                structure: CourseGraph::default(),
                tasks: Deque::default(),
            });
            bot.send_message(user.id, format!("Course created with id {}", id.0))
                .await?;
        }
        "/card" => {
            let (first_word, tail) = tail.trim().split_once(" ").unwrap_or((tail, ""));
            let course_id = CourseId(first_word.parse().unwrap());
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
            let course_id = CourseId(tail.parse::<u64>().unwrap());
            log::info!(
                "user {}({}) sends graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !syncronize(user.id, course_id).await {
                bot.send_message(
                    user.id,
                    format!("Course with id {} not found.", course_id.0),
                )
                .await?;
                return Ok(());
            };

            let Some(course) = STORAGE.get_course(course_id) else {
                bot.send_message(
                    user.id,
                    format!("Course with id {} not found.", course_id.0),
                )
                .await?;
                return Ok(());
            };
            let mut graph = course.structure.generate_structure_graph();

            STORAGE
                .get_progress(user.id, course_id)
                .unwrap()
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
            let course_id = CourseId(tail.parse().unwrap());
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
            let course_id = CourseId(tail.parse().unwrap());
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
            let course_id = CourseId(tail.parse().unwrap());
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
                        STORAGE
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
            let course_id = CourseId(tail.parse().unwrap());
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
                        STORAGE
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
            let course_id = CourseId(tail.parse().unwrap());
            log::info!(
                "user {}({}) sends view_course_errors command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if let Some(errors) = STORAGE.get_course(course_id).unwrap().get_errors() {
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
            let mut user_state = STATE.entry(user.id).or_default();
            match &mut user_state.current_interaction {
                Some(UserInteraction {
                    interactions,
                    current,
                    current_id,
                    current_message,
                    answers,
                    channel: _,
                }) => match &interactions[*current] {
                    TelegramInteraction::UserInput => {
                        let user_input = message.to_owned();

                        bot.delete_message(user.id, current_message.unwrap())
                            .await
                            .log_err();

                        answers.push(user_input);
                        *current += 1;
                        *current_id = rand::random();

                        progress_on_user_event(bot, user.id, &mut user_state.current_interaction)
                            .await
                            .log_err()
                            .unwrap();
                    }
                    _ => {
                        bot.send_message(user.id, "Unexpected input").await?;
                    }
                },
                None => {
                    bot.send_message(user.id, "Command not found!").await?;
                }
            }
        }
    }
    Ok(())
}
