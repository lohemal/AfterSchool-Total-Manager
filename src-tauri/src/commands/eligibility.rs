//! 지원대상자 명령.

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{Eligibility, EligibilityInput};
use crate::repo::eligibility;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EligibilityView {
    pub rows: Vec<Eligibility>,
    /// 대상학년 정책 문구 (`3학년`, `전 학년`)
    pub target_grade_text: String,
    /// 대상학년과 어긋나는 대상자 수 — 경고만 하고 자료는 건드리지 않는다
    pub mismatch_count: i64,
}

#[tauri::command]
pub fn eligibility_list(
    db: State<'_, Db>,
    year_id: i64,
    program: String,
    query: Option<String>,
) -> AppResult<EligibilityView> {
    db.read(|c| {
        let rows = eligibility::list(c, year_id, &program, query.as_deref())?;
        let targets = eligibility::target_grades(c, year_id, &program)?;
        let text = crate::excel::target_grade_text(
            &targets
                .iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        Ok(EligibilityView {
            mismatch_count: rows.iter().filter(|r| r.grade_mismatch).count() as i64,
            rows,
            target_grade_text: text,
        })
    })
}

#[tauri::command]
pub fn eligibility_create(
    db: State<'_, Db>,
    year_id: i64,
    input: EligibilityInput,
) -> AppResult<i64> {
    db.write(|c| eligibility::create(c, year_id, &input))
}

#[tauri::command]
pub fn eligibility_update(db: State<'_, Db>, id: i64, input: EligibilityInput) -> AppResult<()> {
    db.write(|c| eligibility::update(c, id, &input))
}

#[tauri::command]
pub fn eligibility_delete(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<usize> {
    db.write(|c| eligibility::delete_many(c, &ids))
}

/// 그 제도의 대상자 명단을 모두 지운다. **삭제 직전에 자동백업을 남긴다.**
#[tauri::command]
pub fn eligibility_delete_all(
    db: State<'_, Db>,
    year_id: i64,
    program: String,
) -> AppResult<crate::commands::system::BulkDeleteResult> {
    let backup = crate::commands::system::guard_bulk_delete(&db, "지원대상자 명단 전체")?;
    let deleted = db.write(|c| eligibility::delete_all(c, year_id, &program))?;
    Ok(crate::commands::system::BulkDeleteResult {
        deleted: deleted as i64,
        backup,
    })
}
