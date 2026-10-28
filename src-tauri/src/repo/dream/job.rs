//! 방과후드림 결제파일 처리 이력 — 30일 보관.
//!
//! 원본 · 결과 파일은 DB 에 넣지 않는다. 파일 이름(경로 없음) · SHA-256 · 크기와,
//! 그때 기준으로 삼은 정산을 값으로 적는다. 줄에는 내부 id · 파일 행 번호 · 금액 ·
//! 상태 · 오류 코드만 — **학생 이름 · 개인참조번호 · 생년월일은 적지 않는다.**
//!
//! 정리 대상은 `dream_job`(과 함께 지워지는 `dream_job_row` · `dream_job_event`)뿐이다.
//! 개인참조번호 · 강좌 매핑 · 우선순위 · 기관코드는 설정이라 지우지 않는다.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::dream::JOB_RETENTION_DAYS;
use crate::error::{AppError, AppResult};
use crate::repo::dream::amount::FreshSettlement;

pub const STATUSES: [&str; 6] = ["OPEN", "BLOCKED", "READY", "EXPORTED", "FAILED", "CANCELLED"];

/// 처리를 시작할 때 적는 것.
pub struct NewJob<'a> {
    pub workspace_id: i64,
    /// 파일 이름만. 경로가 들어 있으면 거절한다(개인 폴더 이름이 남지 않게).
    pub source_name: &'a str,
    pub source_sha256: &'a str,
    pub source_size: i64,
    /// 기준으로 삼은 정산. 정산을 확인하기 전이면 None.
    pub settlement: Option<&'a FreshSettlement>,
    pub app_version: &'a str,
}

/// 파일 한 줄의 기록.
#[derive(Debug, Clone, Default)]
pub struct RowRecord {
    pub source_row: i64,
    pub student_id: Option<i64>,
    pub department_id: Option<i64>,
    /// 파일의 항목구분코드(`01` 등) — 개인정보가 아니다
    pub dream_item_code: String,
    pub item_code: Option<String>,
    pub amount_n: Option<i64>,
    pub before: Option<(i64, i64, i64)>,
    pub after: Option<(i64, i64, i64)>,
    /// `PENDING` | `MATCHED` | `NEEDS_REVIEW` | `ERROR`
    pub match_status: String,
    pub reason_code: String,
}

pub fn create(conn: &Connection, job: &NewJob) -> AppResult<i64> {
    let name = job.source_name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err(AppError::invalid("파일 이름만 적어 주세요(경로 제외)."));
    }
    if job.source_sha256.len() != 64 || !job.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::invalid("파일 지문(SHA-256)이 올바르지 않습니다."));
    }
    let s = job.settlement;
    conn.execute(
        "INSERT INTO dream_job
           (workspace_id, source_name, source_sha256, source_size, settlement_id, settlement_created,
            ws_data_version, year_data_version, settle_state, app_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            job.workspace_id,
            name,
            job.source_sha256.to_ascii_lowercase(),
            job.source_size,
            s.map(|x| x.settlement_id),
            s.map(|x| x.created_at.clone()),
            s.map(|x| x.ws_data_version),
            s.map(|x| x.year_data_version),
            s.map(|_| "FRESH"),
            job.app_version,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_row(conn: &Connection, job_id: i64, r: &RowRecord) -> AppResult<()> {
    let split = |v: Option<(i64, i64, i64)>| match v {
        Some((o, p, q)) => (Some(o), Some(p), Some(q)),
        None => (None, None, None),
    };
    let (bo, bp, bq) = split(r.before);
    let (ao, ap, aq) = split(r.after);
    let status = if r.match_status.is_empty() { "PENDING" } else { r.match_status.as_str() };
    conn.execute(
        "INSERT INTO dream_job_row
           (job_id, source_row, student_id, department_id, dream_item_code, item_code, amount_n,
            before_o, before_p, before_q, after_o, after_p, after_q, match_status, reason_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            job_id,
            r.source_row,
            r.student_id,
            r.department_id,
            r.dream_item_code,
            r.item_code,
            r.amount_n,
            bo,
            bp,
            bq,
            ao,
            ap,
            aq,
            status,
            r.reason_code,
        ],
    )?;
    Ok(())
}

/// 일어난 일 하나. `detail` 에는 건수 · 오류 코드만 적는다 — 호출하는 쪽의 책임이다.
pub fn add_event(conn: &Connection, job_id: i64, kind: &str, detail: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO dream_job_event (job_id, kind, detail) VALUES (?1, ?2, ?3)",
        params![job_id, kind, detail],
    )?;
    Ok(())
}

/// 상태를 바꾼다. 끝난 상태(EXPORTED · FAILED · CANCELLED)면 끝난 시각을 적는다.
pub fn set_status(conn: &Connection, job_id: i64, status: &str, result_sha256: Option<&str>) -> AppResult<()> {
    if !STATUSES.contains(&status) {
        return Err(AppError::invalid("알 수 없는 처리 상태입니다."));
    }
    let done = matches!(status, "EXPORTED" | "FAILED" | "CANCELLED");
    let n = conn.execute(
        "UPDATE dream_job
            SET status = ?2,
                result_sha256 = COALESCE(?3, result_sha256),
                finished_at = CASE WHEN ?4 THEN datetime('now', 'localtime') ELSE finished_at END
          WHERE id = ?1",
        params![job_id, status, result_sha256, done],
    )?;
    if n == 0 {
        return Err(AppError::not_found("처리 이력을 찾지 못했습니다."));
    }
    Ok(())
}

pub fn status(conn: &Connection, job_id: i64) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT status FROM dream_job WHERE id = ?1", params![job_id], |r| r.get(0))
        .optional()?)
}

/// 30일이 지난 처리 이력을 지운다. 돌려주는 값은 지운 작업 수.
///
/// 끝난 작업은 끝난 시각, 끝나지 않고 버려진 작업은 만든 시각부터 센다.
/// `now` 는 `YYYY-MM-DD HH:MM:SS`(현지 시각) — 시험에서 날짜를 정할 수 있게 받는다.
/// 색인(`dream_job_age_ix`) 한 번이면 되는 가벼운 일이라 화면에 들어올 때마다 불러도 된다.
pub fn cleanup_expired(conn: &Connection, now: &str) -> AppResult<usize> {
    let cutoff: String = conn.query_row(
        "SELECT datetime(?1, ?2)",
        params![now, format!("-{JOB_RETENTION_DAYS} days")],
        |r| r.get(0),
    )?;
    Ok(conn.execute(
        "DELETE FROM dream_job WHERE COALESCE(finished_at, created_at) < ?1",
        params![cutoff],
    )?)
}
