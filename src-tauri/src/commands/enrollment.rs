//! 수강 · 변경이력 · 학생 상세정보 명령 (Phase 2).

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{
    ApplyResult, ChangeLog, Enrollment, EnrollmentFilter, EnrollmentInput, Fee, FeeDiff, FeePick,
    StudentDetail,
};
use crate::repo;
use crate::repo::enrollment::StudentFeeEdit;

#[tauri::command]
pub fn enrollment_list(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: EnrollmentFilter,
) -> AppResult<Vec<Enrollment>> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::list(c, workspace_id, &items, &filter)
    })
}

#[tauri::command]
pub fn enrollment_get(db: State<'_, Db>, id: i64) -> AppResult<Enrollment> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::get(c, id, &items)
    })
}

/// 부서 하나의 수강생 (부서정보 › 학생별 수정).
#[tauri::command]
pub fn enrollment_by_department(
    db: State<'_, Db>,
    workspace_id: i64,
    department_id: i64,
) -> AppResult<Vec<Enrollment>> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::by_department(c, workspace_id, department_id, &items)
    })
}

/// 부서에 설정된 기준 수강료 — 수강 추가 팝업에서 미리 보여 준다.
#[tauri::command]
pub fn department_base_fees(db: State<'_, Db>, department_id: i64) -> AppResult<Vec<Fee>> {
    db.read(|c| repo::department::fees_of(c, department_id))
}

#[tauri::command]
pub fn enrollment_create(
    db: State<'_, Db>,
    workspace_id: i64,
    input: EnrollmentInput,
) -> AppResult<i64> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::create(c, workspace_id, &input, &items)
    })
}

/// 금액만 고친다. 학생·부서는 식별정보이므로 여기서 바꾸지 않는다.
#[tauri::command]
pub fn enrollment_update_fees(
    db: State<'_, Db>,
    id: i64,
    fees: Vec<Fee>,
    reason: String,
) -> AppResult<()> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::update_fees(c, id, &fees, &reason, &items)
    })
}

#[tauri::command]
pub fn enrollment_cancel(db: State<'_, Db>, id: i64, reason: String) -> AppResult<()> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::cancel(c, id, &reason, &items)
    })
}

#[tauri::command]
pub fn enrollment_restore(db: State<'_, Db>, id: i64, reason: String) -> AppResult<()> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::restore(c, id, &reason, &items)
    })
}

/// 학생별 금액 수정 팝업 저장.
#[tauri::command]
pub fn enrollment_save_student_fees(
    db: State<'_, Db>,
    edits: Vec<StudentFeeEdit>,
    reason: String,
) -> AppResult<i64> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::save_student_fees(c, &edits, &reason, &items)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeDiffView {
    pub rows: Vec<FeeDiff>,
    /// 학생별로 고쳐 둔 칸 수 — 기본 방식에서는 이만큼이 그대로 남는다
    pub overridden: i64,
}

/// 부서 기준금액과 어긋난 칸 미리보기. **바뀔 것만** 나온다.
#[tauri::command]
pub fn enrollment_fee_diff(
    db: State<'_, Db>,
    workspace_id: i64,
    department_id: Option<i64>,
) -> AppResult<FeeDiffView> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        let rows = repo::enrollment::fee_diff(c, workspace_id, department_id, &items)?;
        Ok(FeeDiffView {
            overridden: rows.iter().filter(|r| r.is_overridden).count() as i64,
            rows,
        })
    })
}

/// `mode`: `KEEP_EDITED`(권장) | `ALL` | `SELECTED`
#[tauri::command]
pub fn enrollment_apply_fees(
    db: State<'_, Db>,
    workspace_id: i64,
    department_id: Option<i64>,
    mode: String,
    picks: Vec<FeePick>,
    reason: String,
) -> AppResult<ApplyResult> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::apply_fees(
            c,
            workspace_id,
            department_id,
            &mode,
            &picks,
            &reason,
            &items,
        )
    })
}

#[tauri::command]
pub fn change_log_list(
    db: State<'_, Db>,
    year_id: i64,
    workspace_id: Option<i64>,
    student_id: Option<i64>,
    kind: Option<String>,
    limit: Option<i64>,
) -> AppResult<Vec<ChangeLog>> {
    db.read(|c| {
        repo::change_log::list(
            c,
            year_id,
            workspace_id,
            student_id,
            kind.as_deref().filter(|s| !s.is_empty()),
            limit.unwrap_or(500),
        )
    })
}

#[tauri::command]
pub fn student_detail(db: State<'_, Db>, year_id: i64, student_id: i64) -> AppResult<StudentDetail> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::student_detail(c, year_id, student_id, &items)
    })
}
