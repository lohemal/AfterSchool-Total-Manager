//! 학생정보 명령.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{Student, StudentFilter, StudentInput};
use crate::repo::student;

#[tauri::command]
pub fn student_list(
    db: State<'_, Db>,
    year_id: i64,
    filter: StudentFilter,
) -> AppResult<Vec<Student>> {
    db.read(|c| student::list(c, year_id, &filter))
}

#[tauri::command]
pub fn student_create(db: State<'_, Db>, year_id: i64, input: StudentInput) -> AppResult<i64> {
    db.write(|c| student::create(c, year_id, &input))
}

#[tauri::command]
pub fn student_update(db: State<'_, Db>, id: i64, input: StudentInput) -> AppResult<()> {
    db.write(|c| student::update(c, id, &input))
}

#[tauri::command]
pub fn student_delete(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<usize> {
    db.write(|c| student::delete_many(c, &ids))
}

#[tauri::command]
pub fn student_delete_all(db: State<'_, Db>, year_id: i64) -> AppResult<usize> {
    db.write(|c| student::delete_all(c, year_id))
}
