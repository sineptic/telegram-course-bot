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
    state::*,
    utils::ResultExt,
};

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
    let users_state: &DashMap<UserId, UserState> = Box::leak(Box::new(DashMap::new()));

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
            tokio::spawn(update_handler(bot, update, users_state));
        }
    }
}

async fn update_handler(bot: Bot, update: Update, users_state: &DashMap<UserId, UserState>) {
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
            let mut user_state = users_state.entry(user.id).or_default();
            match user_state.current_screen {
                Screen::Main => {
                    handle_main_menu_interaction(bot, user, text, &mut user_state)
                        .await
                        .log_err();
                }
                Screen::Course(course_id) => {
                    handle_course_interaction(bot, user, text, course_id, &mut user_state)
                        .await
                        .log_err();
                }
            }
        }
        UpdateKind::CallbackQuery(callback_query) => {
            callback_handler(bot, callback_query, users_state)
                .await
                .log_err();
        }
        _ => todo!(),
    };
}

async fn send_help_message(
    bot: Bot,
    user: &User,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<()> {
    let main_menu_help_message = "
/help - Display all commands
/create_course - Create new course and get it's ID
/course COURSE_ID - Go to courses menu
";
    let course_help_message = "
/card CARD_NAME — Try to complete card
/graph — View course structure
/help — Display all commands
/change_course_graph
/change_deque
/view_course_graph_source
/view_deque_source
/view_course_errors
";
    bot.send_message(
        user.id,
        match user_state.current_screen {
            Screen::Main => main_menu_help_message,
            Screen::Course(_course_id) => course_help_message,
        },
    )
    .await?;
    Ok(())
}

async fn handle_main_menu_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log::info!(
                "user {}({}) sends help command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            send_help_message(bot, user, user_state).await?;
        }
        "/start" => {
            log::info!(
                "user {}({}) sends start command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            // TODO: onboarding
            bot.send_message(user.id, "TODO: onboarding").await?;

            send_help_message(bot, user, user_state).await?;
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
            bot.send_message(user.id, format!("Course created with id {}.", id.0))
                .await?;
        }
        "/course" => {
            log::info!(
                "user {}({}) sends course command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let Ok(course_id) = tail.parse() else {
                bot.send_message(
                    user.id,
                    format!("Can't parse course id from this string: '{tail}'."),
                )
                .await?;
                return Ok(());
            };
            let course_id = CourseId(course_id);
            if STORAGE.get_course(course_id).is_none() {
                bot.send_message(user.id, "Can't find course with this id.")
                    .await?;
                return Ok(());
            }
            user_state.current_screen = Screen::Course(course_id);
            bot.send_message(user.id, "You are now in courses menu.")
                .await?;
            send_help_message(bot, user, user_state).await?;
        }
        _ => todo!(),
    }
    Ok(())
}

async fn handle_course_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    course_id: CourseId,
    mut user_state: MutUserState<'_, '_>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log::info!(
                "user {}({}) sends help command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            send_help_message(bot, user, user_state).await?;
        }
        "/card" => {
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
                user_state,
            )
            .await?;
        }
        "/graph" => {
            log::info!(
                "user {}({}) sends graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(user.id, "graph command doesn't expect any arguments.")
                    .await?;
                return Ok(());
            }
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
                user_state,
            )
            .await?;
        }
        "/revise" => {
            // TODO
            log::info!(
                "user {}({}) sends revise command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            bot.send_message(user.id, "This command is temporarily disabled")
                .await?;
        }
        "/change_course_graph" => {
            log::info!(
                "user {}({}) sends change_course_graph command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_course_graph command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
            handle_event(
                bot,
                Event::ChangeCourseGraph {
                    user_id: user.id,
                    course_id,
                },
                user_state,
            )
            .await?;
        }
        "/change_deque" => {
            log::info!(
                "user {}({}) sends change_deque command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_deque command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
            handle_event(
                bot,
                Event::ChangeDeque {
                    user_id: user.id,
                    course_id,
                },
                user_state,
            )
            .await?;
        }
        "/view_course_graph_source" => {
            log::info!(
                "user {}({}) sends view_course_graph_source command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_course_graph_source command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
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
                user_state,
            )
            .await?;
        }
        "/view_deque_source" => {
            log::info!(
                "user {}({}) sends view_deque_source command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_deque_source command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
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
                user_state,
            )
            .await?;
        }
        "/view_course_errors" => {
            log::info!(
                "user {}({}) sends view_course_errors command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "view_course_errors command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
            if let Some(errors) = STORAGE.get_course(course_id).unwrap().get_errors() {
                let mut msgs = Vec::new();
                msgs.push("Errors:".into());
                for error in errors {
                    msgs.push(error.into());
                }
                send_interactions(bot, user.id, msgs, user_state).await?;
            } else {
                send_interactions(bot, user.id, vec!["No errors!".into()], user_state).await?;
            }
        }
        // dialogue handling
        _ => match &mut user_state.current_interaction {
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
        },
    }
    Ok(())
}
