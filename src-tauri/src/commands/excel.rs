//! Excel 업로드 / 내려받기 명령.
//!
//! 업로드는 `excel_preview` → (사용자 확인) → `excel_commit` 두 단계다.
//! 미리보기에서 검사한 정상 줄만 메모리에 남고, 토큰으로 다시 꺼내 저장한다.

use std::path::PathBuf;

use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::excel::{self, ExportResult, ImportPreview, ImportResult, RowIssue, Stage};
use crate::model::StudentFilter;
use crate::repo;

/// 파일 이름에 붙일 학년도·작업공간 이름.
fn scope_labels(db: &Db, year_id: i64, workspace_id: Option<i64>) -> AppResult<Vec<String>> {
    db.read(|c| {
        let mut out = Vec::new();
        if let Ok(y) = repo::year::get_year(c, year_id) {
            out.push(y.name);
        }
        if let Some(ws) = workspace_id {
            if let Ok(w) = repo::year::get_workspace(c, ws) {
                out.push(w.name);
            }
        }
        Ok(out)
    })
}

#[tauri::command]
pub fn excel_template(db: State<'_, Db>, kind: String) -> AppResult<ExportResult> {
    let items = db.read(|c| repo::cost_items(c))?;
    excel::template(&kind, &items, &db.export_dir())
}

#[tauri::command]
pub fn excel_preview(
    db: State<'_, Db>,
    stage: State<'_, Stage>,
    kind: String,
    path: String,
    year_id: i64,
    workspace_id: Option<i64>,
    program: Option<String>,
) -> AppResult<ImportPreview> {
    let file = PathBuf::from(&path);
    if !file.exists() {
        return Err(AppError::invalid("선택한 파일을 찾지 못했습니다."));
    }

    let (mut preview, staged) = match kind.as_str() {
        "students" => excel::preview_students(&file)?,
        "eligibility" => {
            let program = program
                .ok_or_else(|| AppError::invalid("지원제도가 지정되지 않았습니다."))?;
            db.read(|c| excel::preview_eligibility(c, year_id, &program, &file))?
        }
        "departments" => {
            if workspace_id.is_none() {
                return Err(AppError::invalid("먼저 작업공간을 선택해 주세요."));
            }
            let items = db.read(|c| repo::cost_items(c))?;
            excel::preview_departments(&items, &file)?
        }
        _ => return Err(AppError::invalid("알 수 없는 업로드 종류입니다.")),
    };

    preview.token = stage.put(staged)?;
    Ok(preview)
}

#[tauri::command]
pub fn excel_commit(
    db: State<'_, Db>,
    stage: State<'_, Stage>,
    token: String,
    year_id: i64,
    workspace_id: Option<i64>,
) -> AppResult<ImportResult> {
    let staged = stage.take(&token)?;
    db.write(|c| excel::commit(c, staged, year_id, workspace_id))
}

#[tauri::command]
pub fn excel_export(
    db: State<'_, Db>,
    kind: String,
    year_id: i64,
    workspace_id: Option<i64>,
    program: Option<String>,
    filter: Option<StudentFilter>,
) -> AppResult<ExportResult> {
    let labels = scope_labels(&db, year_id, workspace_id)?;
    let scope: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let dir = db.export_dir();

    match kind.as_str() {
        "students" => {
            let f = filter.unwrap_or_default();
            db.read(|c| excel::export_students(c, year_id, &f, &scope, &dir))
        }
        "eligibility" => {
            let program =
                program.ok_or_else(|| AppError::invalid("지원제도가 지정되지 않았습니다."))?;
            db.read(|c| excel::export_eligibility(c, year_id, &program, &scope, &dir))
        }
        "departments" => {
            let ws = workspace_id
                .ok_or_else(|| AppError::invalid("먼저 작업공간을 선택해 주세요."))?;
            let items = db.read(|c| repo::cost_items(c))?;
            db.read(|c| excel::export_departments(c, ws, &items, &scope, &dir))
        }
        _ => Err(AppError::invalid("알 수 없는 내려받기 종류입니다.")),
    }
}

/// 업로드에서 걸린 줄만 따로 파일로 내려받는다 — 원본을 고치는 데 쓴다.
#[tauri::command]
pub fn excel_export_issues(
    db: State<'_, Db>,
    issues: Vec<RowIssue>,
    headers: Vec<String>,
) -> AppResult<ExportResult> {
    excel::export_issues(&issues, &headers, &db.export_dir())
}
