pub static STORAGE: LazyLock<CoursesWrapper> = LazyLock::new(|| CoursesWrapper {
    inner: Mutex::new(Courses {
        next_course_id: 0,
        courses: HashMap::new(),
        progress: Vec::new(),
    }),
});

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
    pub fn get_course(&self, course_id: CourseId) -> Option<Course> {
        self.inner().get_course(course_id)
    }
    pub fn set_course(&self, course_id: CourseId, value: Course) {
        self.inner().set_course(course_id, value)
    }
    pub fn select_courses_by_owner(&self, owner: UserId) -> Vec<CourseId> {
        self.inner().select_courses_by_owner(owner)
    }
    pub fn list_user_learned_courses(&self, user_id: UserId) -> Vec<CourseId> {
        self.inner().list_user_learned_courses(user_id)
    }
    /// Panics if user doesn't have progress for this course.
    pub fn get_progress(&self, user_id: UserId, course_id: CourseId) -> UserProgress {
        self.inner().get_progress(user_id, course_id)
    }
    pub fn add_course_to_user(&self, user_id: UserId, course_id: CourseId) {
        self.inner().add_course_to_user(user_id, course_id)
    }
    /// Returns None if this progress doesn't exists.
    pub fn set_course_progress(
        &self,
        user_id: UserId,
        course_id: CourseId,
        progress: UserProgress,
    ) {
        self.inner()
            .set_course_progress(user_id, course_id, progress)
    }
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
impl Courses {
    fn insert(&mut self, course: Course) -> CourseId {
        let course_id = CourseId(self.next_course_id);
        self.next_course_id += 1;
        self.courses.insert(course_id, course);
        course_id
    }
    fn get_course(&self, id: CourseId) -> Option<Course> {
        self.courses.get(&id).cloned()
    }
    fn set_course(&mut self, id: CourseId, content: Course) {
        self.courses.insert(id, content);
    }
    fn select_courses_by_owner(&self, owner: UserId) -> Vec<CourseId> {
        self.courses
            .iter()
            .filter(|(_, course)| course.owner_id == owner)
            .map(|(&course_id, _)| course_id)
            .collect()
    }
    fn list_user_learned_courses(&self, user: UserId) -> Vec<CourseId> {
        self.progress
            .iter()
            .filter(|row| row.user_id == user)
            .map(|row| row.course_id)
            .collect()
    }
    /// Panics if user doesn't have progress for this course.
    fn get_progress(&mut self, user_id: UserId, course_id: CourseId) -> UserProgress {
        self.progress
            .iter()
            .find(|row| row.user_id == user_id && row.course_id == course_id)
            .map(|row| row.progress.clone())
            .unwrap()
    }
    fn add_course_to_user(&mut self, user_id: UserId, course_id: CourseId) {
        let course = self.get_course(course_id).unwrap();
        if course.owner_id != user_id
            && !self
                .progress
                .iter()
                .any(|row| row.user_id == user_id && row.course_id == course_id)
        {
            self.progress.push(ProgressTableRow {
                user_id,
                course_id,
                progress: course.default_user_progress(),
            });
        }
    }
    fn set_course_progress(
        &mut self,
        user_id: UserId,
        course_id: CourseId,
        progress: UserProgress,
    ) {
        self.progress
            .iter_mut()
            .find(|row| row.user_id == user_id && row.course_id == course_id)
            .expect("You should run `add_course_to_user` before this function")
            .progress = progress;
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
