use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, MutexGuard},
};

use course_graph::graph::CourseGraph;
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

struct ProgressTableRow {
    user_id: UserId,
    course_id: CourseId,
    progress: UserProgress,
}
struct Courses {
    next_course_id: u64,
    courses: HashMap<CourseId, Course>,
    progress: Vec<ProgressTableRow>,
}

static STORAGE: LazyLock<Mutex<Courses>> = LazyLock::new(|| {
    Mutex::new(Courses {
        next_course_id: 0,
        courses: HashMap::new(),
        progress: Vec::new(),
    })
});

fn get_storage<'a>() -> MutexGuard<'a, Courses> {
    STORAGE.lock().unwrap_or_else(|err| {
        log::error!("Some thread panicked while holding mutex");
        err.into_inner()
    })
}

pub fn db_insert(course: Course) -> CourseId {
    let mut storage = get_storage();

    let course_id = CourseId(storage.next_course_id);
    storage.next_course_id += 1;
    storage.courses.insert(course_id, course);
    course_id
}
pub fn db_get_course(course_id: CourseId) -> Option<Course> {
    let storage = get_storage();

    storage.courses.get(&course_id).cloned()
}
pub fn db_set_course(course_id: CourseId, value: Course) {
    let mut storage = get_storage();

    storage.courses.insert(course_id, value);
}
pub fn db_select_courses_by_owner(owner: UserId) -> Vec<CourseId> {
    let storage = get_storage();

    storage
        .courses
        .iter()
        .filter(|(_, course)| course.owner_id == owner)
        .map(|(&course_id, _)| course_id)
        .collect()
}
pub fn db_list_user_learned_courses(user_id: UserId) -> Vec<CourseId> {
    let storage = get_storage();

    storage
        .progress
        .iter()
        .filter(|row| row.user_id == user_id)
        .map(|row| row.course_id)
        .collect()
}
/// Panics if user doesn't have progress for this course.
pub fn db_get_progress(user_id: UserId, course_id: CourseId) -> UserProgress {
    let storage = get_storage();

    storage
        .progress
        .iter()
        .find(|row| row.user_id == user_id && row.course_id == course_id)
        .map(|row| row.progress.clone())
        .unwrap()
}
pub fn db_add_course_to_user(user_id: UserId, course_id: CourseId) {
    let mut storage = get_storage();

    let course = storage.courses[&course_id].clone();
    if course.owner_id != user_id
        && !storage
            .progress
            .iter()
            .any(|row| row.user_id == user_id && row.course_id == course_id)
    {
        storage.progress.push(ProgressTableRow {
            user_id,
            course_id,
            progress: course.default_user_progress(),
        });
    }
}
/// Returns None if this progress doesn't exists.
pub fn db_set_course_progress(user_id: UserId, course_id: CourseId, progress: UserProgress) {
    let mut storage = get_storage();

    storage
        .progress
        .iter_mut()
        .find(|row| row.user_id == user_id && row.course_id == course_id)
        .expect("You should run `add_course_to_user` before this function")
        .progress = progress;
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
