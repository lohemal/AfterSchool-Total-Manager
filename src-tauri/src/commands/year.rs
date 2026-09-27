//! 학년도 · 작업공간 명령.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{Workspace, WorkspaceInput, Year};
use crate::repo;
use crate::repo::year;

#[tauri::command]
pub fn year_list(db: State<'_, Db>) -> AppResult<Vec<Year>> {
    db.read(|c| year::list_years(c))
}

#[tauri::command]
pub fn year_create(db: State<'_, Db>, year_no: i64, name: String) -> AppResult<i64> {
    db.write(|c| year::create_year(c, year_no, &name))
}

#[tauri::command]
pub fn year_update(
    db: State<'_, Db>,
    id: i64,
    name: String,
    start_date: String,
    end_date: String,
) -> AppResult<()> {
    db.write(|c| year::update_year(c, id, &name, &start_date, &end_date))
}

#[tauri::command]
pub fn year_set_current(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.write(|c| year::set_current_year(c, id))
}

#[tauri::command]
pub fn year_delete(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.write(|c| year::delete_year(c, id))
}

#[tauri::command]
pub fn workspace_list(db: State<'_, Db>, year_id: i64) -> AppResult<Vec<Workspace>> {
    db.read(|c| year::list_workspaces(c, year_id))
}

#[tauri::command]
pub fn workspace_create(
    db: State<'_, Db>,
    year_id: i64,
    input: WorkspaceInput,
) -> AppResult<i64> {
    db.write(|c| year::create_workspace(c, year_id, &input))
}

#[tauri::command]
pub fn workspace_update(db: State<'_, Db>, id: i64, input: WorkspaceInput) -> AppResult<()> {
    db.write(|c| year::update_workspace(c, id, &input))
}

#[tauri::command]
pub fn workspace_delete(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.write(|c| year::delete_workspace(c, id))
}

#[tauri::command]
pub fn workspace_set_current(db: State<'_, Db>, id: i64) -> AppResult<()> {
    db.write(|c| year::set_current_workspace(c, id))
}

/// 기간이 겹치는 다른 작업공간을 돌려준다. 저장을 막지 않고 화면에서 확인만 받는다.
#[tauri::command]
pub fn workspace_overlaps(
    db: State<'_, Db>,
    year_id: i64,
    exclude_id: Option<i64>,
    start_date: String,
    end_date: String,
) -> AppResult<Vec<Workspace>> {
    db.read(|c| year::overlapping_workspaces(c, year_id, exclude_id, &start_date, &end_date))
}

/// 새 작업공간을 만들며 기존 작업공간의 운영자료를 가져온다 (v0.1.6).
///
/// `copy` 를 주면 **같은 트랜잭션**에서 부서·수강을 옮긴다. 중간에 실패하면
/// 작업공간 자체가 만들어지지 않는다.
#[tauri::command]
pub fn workspace_create_with_copy(
    db: State<'_, Db>,
    year_id: i64,
    input: WorkspaceInput,
    copy: Option<crate::model::WorkspaceCopyInput>,
) -> AppResult<crate::model::WorkspaceCopyResult> {
    db.write(|c| {
        let id = repo::year::create_workspace(c, year_id, &input)?;
        match copy.as_ref() {
            None => Ok(crate::model::WorkspaceCopyResult {
                workspace_id: id,
                departments: 0,
                enrollments: 0,
                charges: 0,
            }),
            Some(opt) => {
                let items = repo::cost_items(c)?;
                repo::workspace_copy::run(c, id, opt, &items)
            }
        }
    })
}

/// 무엇이 넘어오고 무엇이 넘어오지 않는지 미리 보여 준다.
#[tauri::command]
pub fn workspace_copy_preview(
    db: State<'_, Db>,
    source_workspace_id: i64,
) -> AppResult<crate::model::WorkspaceCopyPreview> {
    db.read(|c| repo::workspace_copy::preview(c, source_workspace_id))
}
