//! Excel 업로드 / 내려받기 명령.
//!
//! 업로드는 `excel_preview` → (사용자 확인) → `excel_commit` 두 단계다.
//! 미리보기에서 검사한 정상 줄만 메모리에 남고, 토큰으로 다시 꺼내 저장한다.

use std::path::PathBuf;

use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::excel::{self, ExportResult, ImportPreview, ImportResult, RowIssue, Stage, Staged};
use crate::model::{EnrollmentFilter, StudentFilter};
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
        "enrollments" => {
            let ws = workspace_id
                .ok_or_else(|| AppError::invalid("먼저 작업공간을 선택해 주세요."))?;
            db.read(|c| excel::preview_enrollments(c, year_id, ws, &file))?
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
    enrollment_filter: Option<EnrollmentFilter>,
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
        "enrollments" => {
            let ws = workspace_id
                .ok_or_else(|| AppError::invalid("먼저 작업공간을 선택해 주세요."))?;
            let items = db.read(|c| repo::cost_items(c))?;
            let f = enrollment_filter.unwrap_or_default();
            db.read(|c| excel::export_enrollments(c, ws, &items, &f, &scope, &dir))
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

// ─────────────────────────────────── 부서별 금액 Excel 수정 (v0.1.4)

/// 고른 부서의 지금 수강생·지금 금액을 양식으로 낸다.
#[tauri::command]
pub fn dept_fee_template(
    db: State<'_, Db>,
    workspace_id: i64,
    department_id: i64,
) -> AppResult<ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        let items = repo::cost_items(c)?;
        let ws = repo::year::get_workspace(c, workspace_id)?;
        let year = repo::year::get_year(c, ws.year_id)?;
        let scope = [year.name.as_str(), ws.name.as_str()];
        excel::dept_fees::template(c, workspace_id, department_id, &items, &scope, &dir)
    })
}

/// 파일을 읽어 무엇이 어떻게 바뀌는지 보여 준다. **DB는 건드리지 않는다.**
///
/// 오류가 하나라도 있으면 `token`을 만들지 않는다 — 절반만 반영되는 일이
/// 없어야 하기 때문이다.
#[tauri::command]
pub fn dept_fee_preview(
    db: State<'_, Db>,
    stage: State<'_, Stage>,
    workspace_id: i64,
    department_id: i64,
    path: String,
) -> AppResult<crate::model::FeePreview> {
    let file = PathBuf::from(&path);
    if !file.exists() {
        return Err(AppError::invalid("선택한 파일을 찾지 못했습니다."));
    }
    let (mut preview, edits) = db.read(|c| {
        let items = repo::cost_items(c)?;
        excel::dept_fees::preview(c, workspace_id, department_id, &items, &file)
    })?;

    if preview.errors.is_empty() && !edits.is_empty() {
        preview.token = stage.put(Staged::DeptFees {
            department_id,
            edits,
        })?;
    }
    Ok(preview)
}

/// 미리보기에서 확인한 변경만 한 트랜잭션으로 쓴다.
#[tauri::command]
pub fn dept_fee_apply(
    db: State<'_, Db>,
    stage: State<'_, Stage>,
    workspace_id: i64,
    token: String,
    reason: String,
) -> AppResult<crate::model::FeeApplyResult> {
    let staged = stage.take(&token)?;
    let Staged::DeptFees {
        department_id,
        edits,
    } = staged
    else {
        return Err(AppError::invalid(
            "금액 수정 자료가 아닙니다. 파일을 다시 불러와 주세요.",
        ));
    };
    db.write(|c| {
        let items = repo::cost_items(c)?;
        excel::dept_fees::apply(c, workspace_id, department_id, &edits, &reason, &items)
    })
}

/// 학생별 징수 내역 내려받기 (행정자료, v0.1.3).
///
/// **화면 필터를 그대로 반영한다.** `cond`는 화면이 만들어 보내는 사람이 읽을
/// 조건 문구(예: `2학년·가람반`)로, 파일 이름과 시트 첫 줄에 들어간다.
/// 정산 최신을 요구하지 않는다 — 정산 전에 금액을 대조하는 자료이기 때문이다.
#[tauri::command]
pub fn fee_report_export(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: crate::model::EnrollmentFilter,
    cond: String,
) -> AppResult<excel::ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        let items = repo::cost_items(c)?;
        let ws = repo::year::get_workspace(c, workspace_id)?;
        let year = repo::year::get_year(c, ws.year_id)?;
        let scope = [year.name.as_str(), ws.name.as_str()];
        excel::export_fee_report(c, workspace_id, &items, &filter, &scope, &cond, &dir)
    })
}

// ─────────────────────────────────── 추가징수 · 환불 Excel (v0.1.5)

/// 화면에서 고른 발생일 기간·조건 그대로 한 파일 세 장을 만든다.
///
/// 확인이 필요한 기록이 하나라도 있으면 `excel::adjustment` 가 막는다.
#[tauri::command]
pub fn adjustment_export(
    db: State<'_, Db>,
    workspace_id: i64,
    filter: crate::model::AdjustmentFilter,
    cond: String,
) -> AppResult<ExportResult> {
    let dir = db.export_dir();
    db.read(|c| {
        let items = repo::cost_items(c)?;
        let ws = repo::year::get_workspace(c, workspace_id)?;
        let year = repo::year::get_year(c, ws.year_id)?;
        let scope = [year.name.as_str(), ws.name.as_str()];
        let view = repo::adjustment::view(c, workspace_id, &filter, &items)?;
        excel::adjustment::write_adjustments(&view, &items, &scope, &cond, &dir)
    })
}

// ─────────────────────────────────── 업무파일 저장 위치 (v0.1.5)
//
// 만들어진 파일을 사람이 고른 자리로 옮긴다. 파일을 만드는 서비스는 그대로
// 두고 — 그래야 지금까지의 시험이 그대로 쓰인다 — 마지막 한 걸음만 여기서
// 처리한다.

/// 앱이 만든 파일만 옮긴다. 밖에서 아무 경로나 들어오지 못하게 막는다.
fn must_be_ours(db: &Db, from: &std::path::Path) -> AppResult<()> {
    let ours = db.export_dir();
    let ok = from
        .canonicalize()
        .ok()
        .zip(ours.canonicalize().ok())
        .map(|(f, o)| f.starts_with(&o))
        .unwrap_or(false);
    if !ok {
        return Err(AppError::invalid("앱이 만든 파일이 아닙니다."));
    }
    Ok(())
}

/// 만들어 둔 파일을 사람이 고른 자리로 옮기고, 그 폴더를 기억한다.
#[tauri::command]
pub fn export_deliver(db: State<'_, Db>, from: String, to: String) -> AppResult<String> {
    let src = PathBuf::from(&from);
    let dst = PathBuf::from(&to);
    must_be_ours(&db, &src)?;
    if !src.exists() {
        return Err(AppError::invalid("만들어 둔 파일을 찾지 못했습니다."));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 다른 드라이브로 옮길 때는 rename 이 듣지 않는다 — 그때는 복사하고 지운다.
    if std::fs::rename(&src, &dst).is_err() {
        std::fs::copy(&src, &dst)?;
        let _ = std::fs::remove_file(&src);
    }

    if let Some(parent) = dst.parent().and_then(|p| p.to_str()) {
        let _ = db.write(|c| repo::setting::set(c, LAST_SAVE_DIR, parent));
    }
    Ok(dst.to_string_lossy().to_string())
}

/// 저장을 그만두었을 때 — 만들어 둔 임시 파일을 지운다. 아무것도 남기지 않는다.
#[tauri::command]
pub fn export_discard(db: State<'_, Db>, path: String) -> AppResult<()> {
    let p = PathBuf::from(&path);
    if must_be_ours(&db, &p).is_ok() {
        let _ = std::fs::remove_file(&p);
    }
    Ok(())
}

/// 업무파일을 마지막으로 저장한 폴더. 저장 창의 처음 자리로 쓴다.
pub const LAST_SAVE_DIR: &str = "export.last_dir";

#[tauri::command]
pub fn export_last_dir(db: State<'_, Db>) -> AppResult<Option<String>> {
    let saved = db.read(|c| repo::setting::get(c, LAST_SAVE_DIR))?;
    // 폴더가 사라졌으면 없는 것으로 친다 — 없는 자리를 열면 저장 창이 어색해진다.
    Ok(saved.filter(|p| std::path::Path::new(p).is_dir()))
}

/// 저장한 파일이 있는 폴더를 탐색기로 연다.
#[tauri::command]
pub fn reveal_file(path: String) -> AppResult<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::invalid("파일을 찾지 못했습니다."));
    }
    #[cfg(target_os = "windows")]
    {
        // `/select,` 는 폴더를 열면서 그 파일을 고른 채로 보여 준다.
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(dir) = p.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
    Ok(())
}
