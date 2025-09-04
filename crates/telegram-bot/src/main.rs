use std::cmp::max;

use anyhow::Context;
use course_graph::{graph::CourseGraph, progress_store::TaskProgressStoreExt};
use dashmap::DashMap;
use graphviz_rust::{cmd::Format, printer::PrinterContext};
use teloxide_core::{
    RequestError,
    payloads::SendMessageSetters,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Update, UpdateKind, User},
};

mod event_handler;
mod handlers;
mod interaction_types;
mod state;
mod utils;

use database::*;

use crate::{
    event_handler::{
        complete_card, handle_changing_course_graph, handle_changing_deque, syncronize,
    },
    handlers::{callback_handler, progress_on_user_event, send_interactions},
    interaction_types::{TelegramInteraction, deque::Deque},
    state::*,
    utils::ResultExt,
};
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
        pub fn get_course(&self, course_id: CourseId) -> Option<Arc<Course>> {
            self.inner().get_course(course_id)
        }
        /// Returns whether course already exists.
        pub fn set_course(&self, course_id: CourseId, value: Course) -> bool {
            self.inner().set_course(course_id, value)
        }
        pub fn select_courses_by_owner(&self, owner: UserId) -> Vec<CourseId> {
            self.inner().select_courses_by_owner(owner)
        }
        pub fn list_user_learned_courses(&self, user_id: UserId) -> Vec<CourseId> {
            self.inner().list_user_learned_courses(user_id)
        }
        /// Panics if user doesn't have progress for this course.
        pub fn get_progress(&self, user_id: UserId, course_id: CourseId) -> Arc<UserProgress> {
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
        fn select_courses_by_owner(&self, owner: UserId) -> Vec<CourseId> {
            self.courses_owners_index
                .get(&owner)
                .cloned()
                .unwrap_or_default()
        }
        fn list_user_learned_courses(&self, user: UserId) -> Vec<CourseId> {
            self.progress
                .get(&user)
                .map(|list| list.keys().copied().collect())
                .unwrap_or_default()
        }
        /// Panics if user doesn't have progress for this course.
        fn get_progress(&mut self, user_id: UserId, course_id: CourseId) -> Arc<UserProgress> {
            self.progress
                .entry(user_id)
                .or_default()
                .get(&course_id)
                .unwrap()
                .clone()
        }
        /// Returns false if course already tracked to user
        fn add_course_to_user(&mut self, user_id: UserId, course_id: CourseId) -> bool {
            let course = self.get_course(course_id).unwrap();
            if course.owner_id == user_id {
                return true;
            }
            let entry = self.progress.entry(user_id).or_default().entry(course_id);
            match entry {
                Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(Arc::new(course.default_user_progress()));
                    true
                }
                Entry::Occupied(_occupied_entry) => false,
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

async fn update_handler(bot: Bot, update: Update, user_states: &DashMap<UserId, UserState>) {
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
            let user_state = user_states.entry(user.id).or_default();
            match user_state.current_screen {
                Screen::Main => {
                    handle_main_menu_interaction(bot, user, text, user_state)
                        .await
                        .log_err();
                }
                Screen::Course(course_id) => {
                    match STORAGE.get_course(course_id).unwrap().owner_id == user.id {
                        true => {
                            handle_owned_course_interaction(
                                bot,
                                user,
                                text,
                                course_id,
                                user_state,
                                user_states,
                            )
                            .await
                            .log_err();
                        }
                        false => {
                            handle_learned_course_interaction(
                                bot,
                                user,
                                text,
                                course_id,
                                user_state,
                                user_states,
                            )
                            .await
                            .log_err();
                        }
                    };
                }
            }
        }
        UpdateKind::CallbackQuery(callback_query) => {
            callback_handler(bot, callback_query, user_states)
                .await
                .log_err();
        }
        _ => todo!(),
    };
}

async fn send_help_message(
    bot: Bot,
    user: &User,
    user_state: &MutUserState<'_>,
) -> anyhow::Result<()> {
    let main_menu_help_message = "
/help - Display all commands

/create_course - Create new course and get it's ID
/list - List all your courses
/course COURSE_ID - Go to course menu
";
    let owned_course_help_message = "
/help — Display all commands
/exit - Go to main menu

/preview CARD_NAME — Try to complete card
/graph — View course structure
/change_course_graph
/change_deque
/view_course_graph_source
/view_deque_source
/view_course_errors
";
    let learned_course_help_message = "
/help — Display all commands
/exit - Go to main menu

/card CARD_NAME — Try to complete card
/graph — View course structure
";

    bot.send_message(
        user.id,
        match user_state.current_screen {
            Screen::Main => main_menu_help_message,
            Screen::Course(course_id) => {
                match STORAGE.get_course(course_id).unwrap().owner_id == user.id {
                    true => owned_course_help_message,
                    false => learned_course_help_message,
                }
            }
        },
    )
    .await?;
    Ok(())
}

fn log_user_command(user: &User, command_name: &str) {
    log::info!(
        "user {}({}) sends {command_name} command",
        user.username.clone().unwrap_or("unknown".into()),
        user.id
    );
}

async fn handle_main_menu_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    mut user_state: MutUserState<'_>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/start" => {
            log_user_command(user, "start");
            // TODO: onboarding
            bot.send_message(user.id, "TODO: onboarding").await?;

            send_help_message(bot, user, &user_state).await?;
        }
        "/create_course" => {
            log_user_command(user, "create_course");
            let course_id = STORAGE.insert(Course {
                owner_id: user.id,
                structure: CourseGraph::default(),
                tasks: Deque::default(),
            });
            bot.send_message(user.id, format!("Course created with id {}.", course_id.0))
                .await?;
            user_state.current_screen = Screen::Course(course_id);
            bot.send_message(user.id, "You are now in course menu.")
                .await?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/course" => {
            let Ok(course_id) = tail.parse() else {
                bot.send_message(
                    user.id,
                    format!("Can't parse course id from this string: '{tail}'."),
                )
                .await?;
                return Ok(());
            };
            log::info!(
                "user {}({}) sends course '{course_id}' command",
                user.username.clone().unwrap_or("unknown".into()),
                user.id
            );
            let course_id = CourseId(course_id);
            if STORAGE.get_course(course_id).is_none() {
                bot.send_message(user.id, "Can't find course with this id.")
                    .await?;
                return Ok(());
            }
            user_state.current_screen = Screen::Course(course_id);
            STORAGE.add_course_to_user(user.id, course_id);
            bot.send_message(user.id, "You are now in course menu.")
                .await?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/list" => {
            log_user_command(user, "list");
            let owned_courses = STORAGE.select_courses_by_owner(user.id);
            let learned_courses = STORAGE.list_user_learned_courses(user.id);
            let mut message = String::new();
            message.push_str("# Owned\n");
            for course in owned_courses {
                message.push_str(&course.0.to_string());
                message.push('\n');
            }
            message.push_str("# Learned\n");
            for course in learned_courses {
                message.push_str(&course.0.to_string());
                message.push('\n');
            }
            bot.send_message(user.id, message).await?;
        }
        _ => {
            // FIXME
            bot.send_message(user.id, "Command not found!").await?;
        }
    }
    Ok(())
}

async fn handle_learned_course_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    course_id: CourseId,
    mut user_state: MutUserState<'_>,
    user_states: &DashMap<UserId, UserState>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/exit" => {
            log_user_command(user, "exit");
            user_state.current_screen = Screen::Main;
            bot.send_message(user.id, "You are now in main menu.")
                .await?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/card" => {
            log_user_command(user, "card");
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
            complete_card(bot, user.id, course_id, user_state, user_states, tail).await;
        }
        "/graph" => {
            log_user_command(user, "graph");
            if !tail.is_empty() {
                bot.send_message(user.id, "graph command doesn't expect any arguments.")
                    .await?;
                return Ok(());
            }
            syncronize(user.id, course_id);

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
        _ => {
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
            };
        }
    }
    Ok(())
}

async fn handle_owned_course_interaction(
    bot: Bot,
    user: &User,
    message: &str,
    course_id: CourseId,
    mut user_state: MutUserState<'_>,
    user_states: &DashMap<UserId, UserState>,
) -> anyhow::Result<()> {
    let (first_word, tail) = message.trim().split_once(" ").unwrap_or((message, ""));
    match first_word {
        "/help" => {
            log_user_command(user, "help");
            send_help_message(bot, user, &user_state).await?;
        }
        "/exit" => {
            log_user_command(user, "exit");
            user_state.current_screen = Screen::Main;
            bot.send_message(user.id, "You are now in main menu.")
                .await?;
            send_help_message(bot, user, &user_state).await?;
        }
        "/preview" => {
            log_user_command(user, "preview");
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
            complete_card(bot, user.id, course_id, user_state, user_states, tail).await;
        }
        "/graph" => {
            log_user_command(user, "graph");
            if !tail.is_empty() {
                bot.send_message(user.id, "graph command doesn't expect any arguments.")
                    .await?;
                return Ok(());
            }
            syncronize(user.id, course_id);

            let Some(course) = STORAGE.get_course(course_id) else {
                bot.send_message(
                    user.id,
                    format!("Course with id {} not found.", course_id.0),
                )
                .await?;
                return Ok(());
            };
            let graph = course.structure.generate_structure_graph();

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
            log_user_command(user, "revise");
            bot.send_message(user.id, "This command is temporarily disabled")
                .await?;
        }
        "/change_course_graph" => {
            log_user_command(user, "change_course_graph");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_course_graph command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
            handle_changing_course_graph(bot, user_state, user.id, course_id).await?;
        }
        "/change_deque" => {
            log_user_command(user, "change_deque");
            if !tail.is_empty() {
                bot.send_message(
                    user.id,
                    "change_deque command doesn't expect any arguments.",
                )
                .await?;
                return Ok(());
            }
            handle_changing_deque(bot, user_state, user.id, course_id).await?;
        }
        "/view_course_graph_source" => {
            log_user_command(user, "view_course_graph_source");
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
            log_user_command(user, "view_deque_source");
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
            log_user_command(user, "view_course_errors");
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
        _ => {
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
            };
        }
    }
    Ok(())
}
