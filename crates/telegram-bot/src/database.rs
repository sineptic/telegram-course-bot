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

    tr.execute(
        "
INSERT INTO courses (owner_id, structure, tasks)
VALUES (?1, ?2, ?3);
",
        (
            course.owner_id.0,
            serde_json::to_string(&course.structure).unwrap(),
            serde_json::to_string(&course.tasks).unwrap(),
        ),
    )
    .unwrap();

    let course_id = CourseId(tr.last_insert_rowid() as u64);
    tr.commit().unwrap();
    course_id
}

fn row_to_course(row: &Row) -> rusqlite::Result<Course> {
    Ok(Course {
        owner_id: UserId(row.get("owner_id")?),
        structure: serde_json::from_str(String::as_str(&row.get("structure")?)).unwrap(),
        tasks: serde_json::from_str(String::as_str(&row.get("tasks")?)).unwrap(),
    })
}
pub fn db_get_course(course_id: CourseId) -> Option<Course> {
    let conn = get_connection();

    conn.query_one(
        "
SELECT owner_id, structure, tasks
FROM courses
WHERE course_id = ?;
",
        (course_id.0,),
        row_to_course,
    )
    .optional()
    .unwrap()
}
pub fn db_set_course(course_id: CourseId, value: Course) {
    let conn = get_connection();

    conn.execute(
        "
UPDATE courses
SET owner_id = ?, structure = ?, tasks = ?
WHERE course_id = ?;
",
        (
            value.owner_id.0,
            serde_json::to_string(&value.structure).unwrap(),
            serde_json::to_string(&value.tasks).unwrap(),
            course_id.0,
        ),
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
    .query_map((owner.0,), |row| Ok(CourseId(row.get("course_id")?)))
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
pub fn db_get_progress(user_id: UserId, course_id: CourseId) -> UserProgress {
    let conn = get_connection();

    conn.query_one(
        "SELECT progress FROM user_progress WHERE user_id = ? AND course_id = ?",
        (user_id.0, course_id.0),
        |row| Ok(serde_json::from_str(String::as_str(&row.get("progress")?)).unwrap()),
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
        tr.execute(
            "INSERT INTO user_progress (user_id, course_id, progress) VALUE (?, ?, ?)",
            (
                user_id.0,
                course_id.0,
                serde_json::to_string(&course.default_user_progress()).unwrap(),
            ),
        )
        .unwrap();
    }
    tr.commit().unwrap();
}
/// Returns None if this progress doesn't exists.
pub fn db_set_course_progress(user_id: UserId, course_id: CourseId, progress: UserProgress) {
    let conn = get_connection();
    conn.execute(
        "
UPDATE user_progress
SET progress = ?
WHERE user_id = ? AND course_id = ?
",
        (
            serde_json::to_string(&progress).unwrap(),
            user_id.0,
            course_id.0,
        ),
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
