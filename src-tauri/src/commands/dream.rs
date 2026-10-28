//! 방과후드림 결제관리 명령 (P2 파일 읽기 · 검증 · 미리보기, P3 매칭, P4 우선순위 · 배분).
//!
//! 결제파일은 **읽기만** 한다(P4 까지 원본 · 결과 Excel 을 쓰지 않는다). DB 에 쓰는 것은
//! 기관코드 · 결제 기준월 · (화면 진입 때) 30일 지난 처리 이력 정리 · (사람이 고를 때)
//! 개인정보 초기화 · 개인참조번호 · 강좌 연결 · 방과후드림 우선순위 · 처리 이력(배분 결과
//! 포함)뿐이다. 모두 `dream_*` 표라 정산 자료판과 무관하다.

use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::repo;
use crate::repo::dream::file_check::{self, FileCheck};
use crate::repo::dream::reset::{self, KeyStatus, ResetResult};
use crate::repo::dream::{job, setting, workspace_setting};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamEnter {
    pub key_status: KeyStatus,
    /// 이번에 정리한 처리 이력 수
    pub cleaned: usize,
    /// 정리에 실패했다면 그 까닭(개인정보 없음). 화면은 알리되 막지 않는다.
    pub cleanup_error: Option<String>,
}

/// 화면에 들어올 때 — 30일 지난 처리 이력을 정리하고 키 상태를 본다.
///
/// 정리가 실패해도 화면은 열린다(정산 · 다른 기능과 무관). 실패는 로그와 화면에 알린다.
#[tauri::command]
pub fn dream_enter(db: State<'_, Db>) -> AppResult<DreamEnter> {
    let (cleaned, cleanup_error) = match db.write(|c| {
        let now: String = c.query_row("SELECT datetime('now', 'localtime')", [], |r| r.get(0))?;
        job::cleanup_expired(c, &now)
    }) {
        Ok(n) => {
            if n > 0 {
                log::info!("방과후드림 처리 이력 정리: {n}건");
            }
            (n, None)
        }
        Err(e) => {
            log::warn!("방과후드림 처리 이력 정리 실패: {} {:?}", e.code, e.detail);
            (0, Some(e.message))
        }
    };
    let key_status = db.read(reset::key_status)?;
    Ok(DreamEnter {
        key_status,
        cleaned,
        cleanup_error,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamSettings {
    pub year: i64,
    pub year_name: String,
    pub workspace_name: String,
    pub org_code: Option<String>,
    pub billing_month: Option<String>,
}

#[tauri::command]
pub fn dream_settings(db: State<'_, Db>, workspace_id: i64) -> AppResult<DreamSettings> {
    db.read(|c| {
        let ws = repo::year::get_workspace(c, workspace_id)?;
        let year = repo::year::get_year(c, ws.year_id)?;
        Ok(DreamSettings {
            year: year.year,
            year_name: year.name,
            workspace_name: ws.name,
            org_code: setting::org_code(c)?,
            billing_month: workspace_setting::billing_month(c, workspace_id)?,
        })
    })
}

#[tauri::command]
pub fn dream_set_org_code(db: State<'_, Db>, code: String) -> AppResult<String> {
    db.write(|c| setting::set_org_code(c, &code))
}

#[tauri::command]
pub fn dream_set_billing_month(db: State<'_, Db>, workspace_id: i64, month: String) -> AppResult<String> {
    db.write(|c| workspace_setting::set_billing_month(c, workspace_id, &month))
}

/// 결제파일을 읽어 검사한다. **아무것도 저장하지 않는다** — 처리 이력도 만들지 않는다.
/// 원본은 고른 자리에서 한 번 읽기만 하고 복사하지 않는다.
#[tauri::command]
pub fn dream_check_file(db: State<'_, Db>, workspace_id: i64, path: String) -> AppResult<FileCheck> {
    let p = Path::new(&path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::invalid("파일을 골라 주세요."))?;
    // 너무 큰 파일은 읽기 전에 거른다(크기 검사는 파서도 다시 한다)
    let size = std::fs::metadata(p)
        .map_err(|e| AppError::new("FILE_READ_FAILED", "파일을 열지 못했습니다. 다른 프로그램에서 열려 있지 않은지 확인해 주세요.").detail(e.kind().to_string()))?
        .len();
    if size as usize > crate::excel::dream_file::MAX_BYTES {
        return db.read(|c| file_check::too_large(c, workspace_id, &name, size));
    }
    let bytes = std::fs::read(p).map_err(|e| {
        AppError::new("FILE_READ_FAILED", "파일을 읽지 못했습니다. 다른 프로그램에서 열려 있지 않은지 확인해 주세요.")
            .detail(e.kind().to_string())
    })?;
    db.read(|c| file_check::check(c, workspace_id, &name, &bytes))
}

#[tauri::command]
pub fn dream_key_status(db: State<'_, Db>) -> AppResult<KeyStatus> {
    db.read(reset::key_status)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamReset {
    pub result: ResetResult,
    /// 초기화 직전 자동백업 이름 — 원래 PC 로 되돌아가면 이 백업으로 되살릴 수 있다
    pub backup: String,
}

/// 개인정보 초기화. 키를 쓸 수 없을 때만 되고, **직전에 자동백업을 남긴다.**
#[tauri::command]
pub fn dream_reset_identity(db: State<'_, Db>) -> AppResult<DreamReset> {
    if db.read(reset::key_status)? != KeyStatus::KeyUnavailable {
        return Err(AppError::invalid(
            "개인정보 암호 키를 이 PC 에서 쓸 수 있는 상태라 초기화하지 않습니다.",
        ));
    }
    let backup = crate::commands::system::guard_bulk_delete(&db, "방과후드림 개인정보 초기화")?;
    let result = db.write(|c| reset::reset_identity(c))?;
    Ok(DreamReset { result, backup })
}

// ─────────────────────────────────────────────── P3 — 매칭 및 검증

use crate::repo::dream::session::{self, Session};

/// 고른 자리에서 파일을 한 번 읽는다(복사하지 않는다).
fn read_source(path: &str) -> AppResult<(String, Vec<u8>)> {
    let p = Path::new(path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::invalid("파일을 골라 주세요."))?;
    let size = std::fs::metadata(p)
        .map_err(|e| AppError::new("FILE_READ_FAILED", "파일을 열지 못했습니다.").detail(e.kind().to_string()))?
        .len();
    if size as usize > crate::excel::dream_file::MAX_BYTES {
        return Err(AppError::invalid("파일이 너무 큽니다."));
    }
    let bytes = std::fs::read(p)
        .map_err(|e| AppError::new("FILE_READ_FAILED", "파일을 읽지 못했습니다.").detail(e.kind().to_string()))?;
    Ok((name, bytes))
}

/// 작업이 더는 쓸 수 없게 됐으면(파일 · 정산이 바뀜) 실패로 닫고 그 오류를 돌려준다.
fn close_if_stale(db: &Db, job_id: i64, e: AppError) -> AppError {
    if matches!(e.code.as_str(), "FILE_CHANGED" | "SETTLEMENT_CHANGED" | "SETTLEMENT_NOT_FRESH") {
        let code = e.code.clone();
        let _ = db.write(|c| session::fail(c, job_id, &code));
    }
    e
}

/// [매칭 및 검증 시작] — 파일 지문 · 정산 FRESH 를 다시 확인하고 처리 이력을 만든다.
#[tauri::command]
pub fn dream_start(db: State<'_, Db>, workspace_id: i64, path: String, sha256: String) -> AppResult<Session> {
    let (name, bytes) = read_source(&path)?;
    db.write(|c| session::start(c, workspace_id, &name, &bytes, &sha256, env!("CARGO_PKG_VERSION")))
}

#[tauri::command]
pub fn dream_revalidate(db: State<'_, Db>, job_id: i64, path: String) -> AppResult<Session> {
    let (_, bytes) = read_source(&path)?;
    db.write(|c| session::revalidate(c, job_id, &bytes))
        .map_err(|e| close_if_stale(&db, job_id, e))
}

/// 개인참조번호 확인 · 등록(여럿 한꺼번에) → 다시 검증.
#[tauri::command]
pub fn dream_confirm_identities(
    db: State<'_, Db>,
    job_id: i64,
    path: String,
    picks: Vec<(u32, i64)>,
) -> AppResult<Session> {
    let (_, bytes) = read_source(&path)?;
    db.write(|c| {
        session::confirm_identities(c, job_id, &bytes, &picks)?;
        session::revalidate(c, job_id, &bytes)
    })
    .map_err(|e| close_if_stale(&db, job_id, e))
}

#[tauri::command]
pub fn dream_confirm_births(db: State<'_, Db>, job_id: i64, path: String, keys: Vec<u32>) -> AppResult<Session> {
    let (_, bytes) = read_source(&path)?;
    db.write(|c| {
        session::confirm_births(c, job_id, &bytes, &keys)?;
        session::revalidate(c, job_id, &bytes)
    })
    .map_err(|e| close_if_stale(&db, job_id, e))
}

#[tauri::command]
pub fn dream_confirm_course(
    db: State<'_, Db>,
    job_id: i64,
    path: String,
    course_code: String,
    department_id: i64,
) -> AppResult<Session> {
    let (_, bytes) = read_source(&path)?;
    db.write(|c| {
        session::confirm_course(c, job_id, &bytes, &course_code, department_id)?;
        session::revalidate(c, job_id, &bytes)
    })
    .map_err(|e| close_if_stale(&db, job_id, e))
}

// ─────────────────────────────────────────────── P4 — 우선순위 · 배분

use crate::repo::dream::allocation::{self, Allocation, PriorityView};

#[tauri::command]
pub fn dream_priority(db: State<'_, Db>, workspace_id: i64) -> AppResult<PriorityView> {
    db.read(|c| allocation::priority_view(c, workspace_id))
}

/// 방과후드림 부서 차례 저장 — 기존 정산 순위 · 자료판은 그대로다. 바뀌었으면 끝나지 않은
/// 작업의 배분 결과를 비운다(다시 계산해야 한다).
#[tauri::command]
pub fn dream_save_dept_priority(db: State<'_, Db>, workspace_id: i64, order: Vec<i64>) -> AppResult<PriorityView> {
    db.write(|c| {
        allocation::save_dept_priority(c, workspace_id, &order)?;
        allocation::priority_view(c, workspace_id)
    })
}

#[tauri::command]
pub fn dream_save_item_priority(
    db: State<'_, Db>,
    workspace_id: i64,
    department_id: i64,
    order: Vec<String>,
) -> AppResult<PriorityView> {
    db.write(|c| {
        let ws: i64 = c.query_row("SELECT workspace_id FROM department WHERE id = ?1", [department_id], |r| r.get(0))?;
        if ws != workspace_id {
            return Err(AppError::invalid("이 작업공간의 부서가 아닙니다."));
        }
        allocation::save_item_priority(c, department_id, &order)?;
        allocation::priority_view(c, workspace_id)
    })
}

/// [배분 계산] — 파일 지문 · 정산 · P3 전체를 다시 확인한 뒤 계산하고, 모두 통과해야 저장한다.
#[tauri::command]
pub fn dream_allocate(db: State<'_, Db>, job_id: i64, path: String) -> AppResult<Allocation> {
    let (_, bytes) = read_source(&path)?;
    db.write(|c| allocation::allocate(c, job_id, &bytes))
        .map_err(|e| close_if_stale(&db, job_id, e))
}

/// 저장된 배분 결과가 지금도 유효한가(아무것도 적지 않는다).
#[tauri::command]
pub fn dream_allocation(db: State<'_, Db>, job_id: i64, path: String) -> AppResult<Allocation> {
    let (_, bytes) = read_source(&path)?;
    db.read(|c| allocation::verify(c, job_id, &bytes))
}
