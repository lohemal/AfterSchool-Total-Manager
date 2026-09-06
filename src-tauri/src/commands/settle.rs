//! 정산 · 우선순위 · 학생별 예외 한도 명령 (Phase 3).

use tauri::State;

use crate::db::Db;
use crate::domain::Program;
use crate::error::{AppError, AppResult};
use crate::model::{
    GenerateResult, Grant, GrantInput, Issue, PriorityRow, ProgramRow, SelfPayRow, SettlementStatus,
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

#[tauri::command]
pub fn settlement_self_pay(db: State<'_, Db>, workspace_id: i64) -> AppResult<Vec<SelfPayRow>> {
    db.read(|c| {
        let items = repo::cost_items(c)?;
        repo::settle::self_pay_rows(c, workspace_id, &items)
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
