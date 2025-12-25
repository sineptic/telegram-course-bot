use std::sync::{LazyLock, Mutex, MutexGuard};

use course_graph::graph::CourseGraph;
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use teloxide_core::types::UserId;

use crate::{event_handler::progress_store::UserProgress, interaction_types::deque::Deque};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Debug)]
pub struct CourseId(pub u64);
#[derive(Clone, Serialize, Deserialize)]
pub struct Course {
    pub owner_id: UserId,
    pub structure: CourseGraph,
    pub tasks: Deque,
}

static STORAGE: LazyLock<Mutex<Connection>> =
    LazyLock::new(|| Mutex::new(Connection::open("db.sqlite").unwrap()));

fn get_connection<'a>() -> MutexGuard<'a, Connection> {
    STORAGE.lock().unwrap_or_else(|err| {
        log::error!("Some thread panicked while holding mutex");
        err.into_inner()
    })
}

pub fn db_create_tables() {
    let conn = get_connection();

    conn.execute_batch(
        "
BEGIN;

CREATE TABLE IF NOT EXISTS courses (
    course_id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id INTEGER NOT NULL,
    structure TEXT NOT NULL,  -- JSON serialized CourseGraph
    tasks TEXT NOT NULL       -- JSON serialized Deque
);

CREATE TABLE IF NOT EXISTS user_progress (
    user_id INTEGER NOT NULL,
    course_id INTEGER NOT NULL,
    progress TEXT NOT NULL,   -- JSON serialized UserProgress
    PRIMARY KEY (user_id, course_id),
    FOREIGN KEY (course_id) REFERENCES courses(course_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_courses_owner ON courses(owner_id);

CREATE INDEX IF NOT EXISTS idx_user_progress_user ON user_progress(user_id);

COMMIT;
",
    )
    .unwrap();
}

pub fn db_insert(course: Course) -> CourseId {
    let mut conn = get_connection();

    let tr = conn.transaction().unwrap();
    let owner_id = course.owner_id.0;
    let structure = serde_json::to_string(&course.structure).unwrap();
    let tasks = serde_json::to_string(&course.tasks).unwrap();
    tr.execute(
        "
        INSERT INTO courses (owner_id, structure, tasks)
        VALUES (?1, ?2, ?3);
        ",
        (owner_id, structure, tasks),
    )
    .unwrap();
    let course_id = CourseId(tr.last_insert_rowid() as u64);
    tr.commit().unwrap();

    course_id
}

fn row_to_course(row: &Row) -> rusqlite::Result<Course> {
    let owner_id = UserId(row.get_unwrap("owner_id"));
    let structure: String = row.get_unwrap("structure");
    let structure = serde_json::from_str(&structure).unwrap();
    let tasks: String = row.get_unwrap("tasks");
    let tasks = serde_json::from_str(&tasks).unwrap();
    Ok(Course {
        owner_id,
        structure,
        tasks,
    })
}
pub fn db_get_course(CourseId(course_id): CourseId) -> Option<Course> {
    let conn = get_connection();

    conn.query_one(
        "
        SELECT owner_id, structure, tasks
        FROM courses
        WHERE course_id = ?;
        ",
        (course_id,),
        row_to_course,
    )
    .optional()
    .unwrap()
}
pub fn db_set_course(CourseId(course_id): CourseId, course: Course) {
    let conn = get_connection();

    let owner_id = course.owner_id.0;
    let structure = serde_json::to_string(&course.structure).unwrap();
    let tasks = serde_json::to_string(&course.tasks).unwrap();
    conn.execute(
        "
        UPDATE courses
        SET owner_id = ?, structure = ?, tasks = ?
        WHERE course_id = ?;
        ",
        (owner_id, structure, tasks, course_id),
    )
    .unwrap();
}
pub fn db_select_courses_by_owner(owner: UserId) -> Vec<CourseId> {
    let conn = get_connection();

    conn.prepare(
        "
        SELECT course_id
        FROM courses
        WHERE owner_id = ?;
        ",
    )
    .unwrap()
    .query_map((owner.0,), |row| Ok(CourseId(row.get_unwrap("course_id"))))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
pub fn db_list_user_learned_courses(user_id: UserId) -> Vec<CourseId> {
    let conn = get_connection();

    conn.prepare(
        "
        SELECT course_id
        FROM user_progress
        WHERE user_id = ?;
        ",
    )
    .unwrap()
    .query_map((user_id.0,), |row| Ok(CourseId(row.get("course_id")?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
/// Panics if user doesn't have progress for this course.
pub fn db_get_progress(UserId(user_id): UserId, CourseId(course_id): CourseId) -> UserProgress {
    let conn = get_connection();

    conn.query_one(
        "SELECT progress FROM user_progress WHERE user_id = ? AND course_id = ?",
        (user_id, course_id),
        |row| {
            let progress: String = row.get_unwrap("progress");
            let progress = serde_json::from_str(&progress).unwrap();
            Ok(progress)
        },
    )
    .unwrap()
}
pub fn db_add_course_to_user(user_id: UserId, course_id: CourseId) {
    let mut conn = get_connection();

    let tr = conn.transaction().unwrap();
    let course = tr
        .query_one(
            "
            SELECT owner_id, structure, tasks
            FROM courses
            WHERE course_id = ?;
            ",
            (course_id.0,),
            row_to_course,
        )
        .unwrap();

    if course.owner_id != user_id {
        let default_progress = serde_json::to_string(&course.default_user_progress()).unwrap();
        tr.execute(
            "INSERT OR IGNORE INTO user_progress (user_id, course_id, progress) VALUE (?, ?, ?)",
            (user_id.0, course_id.0, default_progress),
        )
        .unwrap();
    }
    tr.commit().unwrap();
}
/// Returns None if this progress doesn't exists.
pub fn db_set_course_progress(user_id: UserId, course_id: CourseId, progress: UserProgress) {
    let conn = get_connection();
    let progress = serde_json::to_string(&progress).unwrap();
    conn.execute(
        "
        UPDATE user_progress
        SET progress = ?
        WHERE user_id = ? AND course_id = ?
        ",
        (progress, user_id.0, course_id.0),
    )
    .unwrap();
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
            .map(|id| format!("Graph has '{id}' card, but deque doesn't."))
            .for_each(|item| errors.push(item));
        deque
            .tasks
            .keys()
            .filter(|x| !CourseGraph::default().cards().contains_key(*x))
            .map(|err| format!("Deque has '{err}', but graph doesn't."))
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
