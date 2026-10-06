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

/// 이 작업공간의 부서를 모두 지운다. 수강 자료도 함께 사라지므로
/// **삭제 직전에 자동백업을 남긴다.**
#[tauri::command]
pub fn department_delete_all(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::commands::system::BulkDeleteResult> {
    let backup = crate::commands::system::guard_bulk_delete(&db, "부서정보 전체")?;
    let deleted = db.write(|c| department::delete_all(c, workspace_id))?;
    Ok(crate::commands::system::BulkDeleteResult {
        deleted: deleted as i64,
        backup,
    })
}

// ─────────────────────────────────────────────── 부서별 수강현황 (v0.1.7)

/// 한 작업공간의 수강현황 한 벌 — 요약 · 반별 · 학년별 · 요일별.
#[tauri::command]
pub fn capacity_stats(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::model::CapacityStats> {
    db.read(|c| crate::repo::capacity::stats(c, workspace_id))
}

/// 수강 가능 부서 찾기. **읽기 전용이다** — 수강을 만들지도 고치지도 않는다.
#[tauri::command]
pub fn capacity_find_seats(
    db: State<'_, Db>,
    workspace_id: i64,
    query: crate::model::SeatQuery,
) -> AppResult<crate::model::SeatResult> {
    db.read(|c| crate::repo::capacity::find_seats(c, workspace_id, &query))
}

/// 통계 Excel. **화면 필터와 무관하게 작업공간 전체**를 낸다 — 공식 통계에서
/// 일부 반이 조용히 빠지면 결재 자료가 틀린다.
#[tauri::command]
pub fn capacity_export(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::excel::ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        let ws = crate::repo::year::get_workspace(c, workspace_id)?;
        let year = crate::repo::year::get_year(c, ws.year_id)?;
        let scope = [year.name.as_str(), ws.name.as_str()];
        crate::excel::export_capacity(c, workspace_id, &scope, &dir)
    })
}
