//! 부서정보 명령.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{Department, DepartmentInput};
use crate::repo::department;

#[tauri::command]
pub fn department_list(
    db: State<'_, Db>,
    workspace_id: i64,
    query: Option<String>,
) -> AppResult<Vec<Department>> {
    db.read(|c| department::list(c, workspace_id, query.as_deref()))
}

#[tauri::command]
pub fn department_create(
    db: State<'_, Db>,
    workspace_id: i64,
    input: DepartmentInput,
) -> AppResult<i64> {
    db.write(|c| department::create(c, workspace_id, &input))
}

#[tauri::command]
pub fn department_update(db: State<'_, Db>, id: i64, input: DepartmentInput) -> AppResult<()> {
    db.write(|c| department::update(c, id, &input))
}

#[tauri::command]
pub fn department_delete(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<usize> {
    db.write(|c| department::delete_many(c, &ids))
}

#[tauri::command]
pub fn department_delete_all(db: State<'_, Db>, workspace_id: i64) -> AppResult<usize> {
    db.write(|c| department::delete_all(c, workspace_id))
}
