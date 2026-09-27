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

/// 수강 수기 추가.
///
/// `adjustment` 를 주면 이 수강 건을 **추가징수 대상**으로 함께 등록한다
/// (v0.1.5). 프로그램이 등록일만 보고 스스로 정하지 않는다 — 최초 징수 전에
/// 넣는 학생도 있기 때문이다.
///
/// `db.write` 하나로 감싸므로 수강·금액·추가징수 기록이 **함께 남거나 함께
/// 되돌아간다.**
#[tauri::command]
pub fn enrollment_create(
    db: State<'_, Db>,
    workspace_id: i64,
    input: EnrollmentInput,
    adjustment: Option<crate::model::AdjustmentInput>,
) -> AppResult<i64> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        let id = repo::enrollment::create(c, workspace_id, &input, &items)?;
        if let Some(adj) = adjustment.as_ref() {
            repo::adjustment::create_additional(c, id, adj, &items)?;
        }
        Ok(id)
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

/// 수강 취소. `fees`를 주면 최종 징수금액을 함께 확정한다 (v0.1.3).
///
/// `adjustment` 를 주면 **환불 대상**으로 함께 등록한다 (v0.1.5).
/// 환불액은 `취소 직전 금액 - 취소 후 최종 금액`이고, 그 '취소 직전'은 취소가
/// 끝나면 다시 구할 수 없으므로 **여기서 먼저 떠 둔다.**
///
/// 어느 항목이든 환불액이 음수가 되면 `create_refund` 가 오류를 내고, 트랜잭션
/// 전체가 되돌아가 **취소도 일어나지 않는다.** 금액을 고치거나 [환불 대상]
/// 체크를 풀면 기존과 똑같이 취소된다 — 취소 기능 자체의 허용 범위는 그대로다.
///
/// `db.write`가 트랜잭션을 감싸므로 금액·상태·이력·환불기록이 함께 커밋되거나
/// 함께 되돌아간다.
#[tauri::command]
pub fn enrollment_cancel(
    db: State<'_, Db>,
    id: i64,
    fees: Option<Vec<Fee>>,
    reason: String,
    adjustment: Option<crate::model::AdjustmentInput>,
) -> AppResult<()> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        let before = repo::enrollment::get(c, id, &items)?;
        repo::enrollment::cancel(c, id, fees.as_deref(), &reason, &items)?;
        if let Some(adj) = adjustment.as_ref() {
            repo::adjustment::create_refund(c, id, &before, adj, &items)?;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn enrollment_restore(
    db: State<'_, Db>,
    id: i64,
    reset_fees: Option<bool>,
    reason: String,
) -> AppResult<()> {
    db.write(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::restore(c, id, reset_fees.unwrap_or(false), &reason, &items)
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

/// 이 작업공간의 수강 자료를 모두 지운다. **삭제 직전에 자동백업을 남긴다.**
#[tauri::command]
pub fn enrollment_delete_all(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::commands::system::BulkDeleteResult> {
    let backup = crate::commands::system::guard_bulk_delete(&db, "수강 자료 전체")?;
    let deleted = db.write(|c| repo::enrollment::delete_all(c, workspace_id))?;
    Ok(crate::commands::system::BulkDeleteResult {
        deleted: deleted as i64,
        backup,
    })
}

/// 학생별 징수 내역 (행정자료).
///
/// **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`)를 그대로 보여
/// 준다. 그래서 정산이 없거나 낡아도 조회된다 — 정산 전에 금액을 대조하는
/// 자료이기 때문이다.
///
/// v0.1.4부터 **학생당 한 줄**이다. 집계는 `repo::fee_report`가 하고 여기서는
/// 부르기만 한다.
#[tauri::command]
pub fn fee_report(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: EnrollmentFilter,
) -> AppResult<crate::model::FeeReport> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::fee_report::build(c, workspace_id, &items, &filter)
    })
}

// ─────────────────────────────────── 추가징수 · 환불 (v0.1.5)

/// 추가·취소 관리 두 탭.
#[tauri::command]
pub fn adjustment_view(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: crate::model::AdjustmentFilter,
) -> AppResult<crate::model::AdjustmentView> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::adjustment::view(c, workspace_id, &filter, &items)
    })
}

/// 사이드바 배지 — 작업공간 전체의 확인 필요 건수.
#[tauri::command]
pub fn adjustment_needs_check(db: State<'_, Db>, workspace_id: i64) -> AppResult<i64> {
    db.read(|c| repo::adjustment::needs_check_count(c, workspace_id))
}

/// 원본이 달라진 칸 목록 — [변경내역 확인] 화면.
#[tauri::command]
pub fn adjustment_diffs(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<Vec<crate::model::AdjustmentDiff>> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::adjustment::diffs(c, workspace_id, &items)
    })
}

/// `mode`: `KEEP`(기존 금액 유지) | `APPLY`(현재 금액 반영).
#[tauri::command]
pub fn adjustment_confirm(db: State<'_, Db>, ids: Vec<i64>, mode: String) -> AppResult<i64> {
    db.write(|c| repo::adjustment::confirm(c, &ids, &mode))
}

#[tauri::command]
pub fn adjustment_delete(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<usize> {
    db.write(|c| repo::adjustment::delete_many(c, &ids))
}

// ─────────────────────────────────── 쪽 나누기 · 다중 정렬 (v0.1.6)

/// 수강생 명단 한 쪽.
///
/// **전체를 보내지 않는다.** 필터로 거른 뒤 정렬하고, 그 쪽에 보일 줄만 읽는다.
/// Excel 은 이 명령을 쓰지 않으므로 쪽 나누기의 영향을 받지 않는다.
#[tauri::command]
pub fn enrollment_page(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: EnrollmentFilter,
    sort: Vec<crate::model::SortSpec>,
    page: i64,
    page_size: i64,
) -> AppResult<crate::model::EnrollmentPage> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::enrollment::list_page(c, workspace_id, &items, &filter, &sort, page, page_size)
    })
}

/// 필터 드롭다운에 채울 학년·반.
#[tauri::command]
pub fn enrollment_filter_options(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::model::EnrollmentFilterOptions> {
    db.read(|c| repo::enrollment::filter_options(c, workspace_id))
}
