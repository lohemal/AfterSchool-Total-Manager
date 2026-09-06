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

/// 전교생을 모두 지운다. **삭제 직전에 자동백업을 남긴다** (요구사항 §7).
#[tauri::command]
pub fn student_delete_all(
    db: State<'_, Db>,
    year_id: i64,
) -> AppResult<crate::commands::system::BulkDeleteResult> {
    let backup = crate::commands::system::guard_bulk_delete(&db, "학생정보 전체")?;
    let deleted = db.write(|c| student::delete_all(c, year_id))?;
    Ok(crate::commands::system::BulkDeleteResult {
        deleted: deleted as i64,
        backup,
    })
}
