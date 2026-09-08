//! 정산 · 우선순위 · 학생별 예외 한도 명령 (Phase 3).

use tauri::State;

use crate::db::Db;
use crate::domain::Program;
use crate::error::{AppError, AppResult};
use crate::model::{
    GenerateResult, Grant, GrantInput, Issue, PriorityRow, ProgramRow, SettlementStatus,
    Summary, SupportState,
};
use crate::repo;

fn program_of(code: &str) -> AppResult<Program> {
    Program::from_code(code).ok_or_else(|| AppError::invalid("알 수 없는 지원제도입니다."))
}

// ─────────────────────────────────────────────── 정산

#[tauri::command]
pub fn settlement_status(db: State<'_, Db>, workspace_id: i64) -> AppResult<SettlementStatus> {
    db.read(|c| repo::settle::status(c, workspace_id))
}

#[tauri::command]
pub fn settlement_validate(db: State<'_, Db>, workspace_id: i64) -> AppResult<Vec<Issue>> {
    db.read(|c| repo::settle::validate(c, workspace_id))
}

/// **사람이 눌렀을 때만** 실행된다. 화면을 열 때 부르지 않는다.
#[tauri::command]
pub fn settlement_generate(db: State<'_, Db>, workspace_id: i64) -> AppResult<GenerateResult> {
    db.write(|c| repo::settle::generate(c, workspace_id))
}

#[tauri::command]
pub fn settlement_summary(db: State<'_, Db>, workspace_id: i64) -> AppResult<Option<Summary>> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::settle::summary(c, workspace_id, &items)
    })
}

/// 수익자 탭 — 학생별 합계 + 부서별 상세 (v0.1.4).
///
/// 이용권·자유수강권과 같은 모양이다. 금액은 정산 스냅샷의 학부모 부담 배분액만
/// 모은 것으로, 원본 `charge` 전체가 아니다.
#[tauri::command]
pub fn settlement_self_pay(
    db: State<'_, Db>,
    workspace_id: i64,
) -> AppResult<crate::model::SelfPayReport> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::settle::self_pay_report(c, workspace_id, &items)
    })
}

#[tauri::command]
pub fn settlement_program(
    db: State<'_, Db>,
    workspace_id: i64,
    program: String,
) -> AppResult<Vec<ProgramRow>> {
    let p = program_of(&program)?;
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::settle::program_rows(c, workspace_id, p, &items)
    })
}

#[tauri::command]
pub fn settlement_student_supports(
    db: State<'_, Db>,
    workspace_id: i64,
    student_id: i64,
) -> AppResult<Vec<SupportState>> {
    db.read(|c| repo::settle::student_supports(c, workspace_id, student_id))
}

// ─────────────────────────────────────────────── 차감 우선순위

#[tauri::command]
pub fn priority_dept_list(db: State<'_, Db>, workspace_id: i64) -> AppResult<Vec<PriorityRow>> {
    db.read(|c| repo::priority::dept_list(c, workspace_id))
}

#[tauri::command]
pub fn priority_dept_save(
    db: State<'_, Db>,
    workspace_id: i64,
    order: Vec<i64>,
) -> AppResult<()> {
    db.write(|c| repo::priority::dept_save(c, workspace_id, &order))
}

#[tauri::command]
pub fn priority_item_list(db: State<'_, Db>, workspace_id: i64) -> AppResult<Vec<PriorityRow>> {
    db.read(|c| repo::priority::item_list(c, workspace_id))
}

#[tauri::command]
pub fn priority_item_save(
    db: State<'_, Db>,
    workspace_id: i64,
    order: Vec<String>,
) -> AppResult<()> {
    db.write(|c| repo::priority::item_save(c, workspace_id, &order))
}

// ─────────────────────────────────────────────── 학생별 예외 한도

#[tauri::command]
pub fn grant_list(db: State<'_, Db>, year_id: i64, program: String) -> AppResult<Vec<Grant>> {
    program_of(&program)?;
    db.read(|c| repo::policy::grant_list(c, year_id, &program))
}

#[tauri::command]
pub fn grant_save(db: State<'_, Db>, year_id: i64, input: GrantInput) -> AppResult<i64> {
    db.write(|c| repo::policy::grant_save(c, year_id, &input))
}

#[tauri::command]
pub fn grant_delete(db: State<'_, Db>, ids: Vec<i64>) -> AppResult<usize> {
    db.write(|c| repo::policy::grant_delete(c, &ids))
}

// ─────────────────────────────────────────────── 행정자료 (Phase 4)

use crate::excel::{self, ExportResult};
use crate::model::{CostItem, Proposal, ProposalKind, StudentAllocRow};

/// 품의로 뽑을 수 있는 종류 (강사료 · 수용비 · 교재비 · 재료비 · 교재·재료비).
#[tauri::command]
pub fn proposal_kinds(db: State<'_, Db>) -> AppResult<Vec<ProposalKind>> {
    db.read(|c| {
        let items: Vec<CostItem> = repo::cost_items(c)?;
        Ok(repo::proposal::kinds(&items))
    })
}

/// 화면 미리보기. Excel과 **같은 집계 결과**를 쓴다.
#[tauri::command]
pub fn proposal_preview(
    db: State<'_, Db>,
    workspace_id: i64,
    kind: String,
) -> AppResult<Proposal> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::proposal::build(c, workspace_id, &kind, &items)
    })
}

#[tauri::command]
pub fn proposal_export(
    db: State<'_, Db>,
    workspace_id: i64,
    kind: String,
) -> AppResult<ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        let items = repo::cost_items(c)?;
        let p = repo::proposal::build(c, workspace_id, &kind, &items)?;
        excel::admin::write_proposal(&p, &dir)
    })
}

/// 학생 한 명의 부서별 정산 상세 (팝업).
#[tauri::command]
pub fn settlement_student_allocs(
    db: State<'_, Db>,
    workspace_id: i64,
    student_id: i64,
) -> AppResult<Vec<StudentAllocRow>> {
    db.read(|c| repo::settle::student_allocs(c, workspace_id, student_id))
}

/// 정산 결과 Excel. **최신 유효 정산일 때만 만든다.**
///
/// `kind`: `self_pay` | `voucher` | `free_voucher`
#[tauri::command]
pub fn settlement_export(
    db: State<'_, Db>,
    workspace_id: i64,
    kind: String,
) -> AppResult<ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        // 낡은 정산으로 파일을 만들지 않는다 (요구사항 §1)
        repo::settle::require_fresh(c, workspace_id)?;
        let items = repo::cost_items(c)?;
        let ws = repo::year::get_workspace(c, workspace_id)?;
        let year = repo::year::get_year(c, ws.year_id)?;
        let scope = [year.name.as_str(), ws.name.as_str()];

        match kind.as_str() {
            "self_pay" => {
                let report = repo::settle::self_pay_report(c, workspace_id, &items)?;
                excel::admin::write_self_pay(&report, &items, &scope, &dir)
            }
            "voucher" => {
                let rows =
                    repo::settle::program_rows(c, workspace_id, Program::Voucher, &items)?;
                excel::admin::write_program(
                    &rows,
                    &items,
                    "방과후이용권",
                    true,
                    &scope,
                    &dir,
                )
            }
            "free_voucher" => {
                let rows =
                    repo::settle::program_rows(c, workspace_id, Program::FreeVoucher, &items)?;
                excel::admin::write_program(&rows, &items, "자유수강권", false, &scope, &dir)
            }
            _ => Err(AppError::invalid("알 수 없는 내려받기 종류입니다.")),
        }
    })
}
