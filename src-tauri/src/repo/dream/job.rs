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

/// 처리 이력 한 건의 기준값 — 다음 단계로 갈 때 지금 상태와 맞대어 본다.
#[derive(Debug, Clone)]
pub struct JobHead {
    pub id: i64,
    pub workspace_id: i64,
    pub status: String,
    pub source_name: String,
    pub source_sha256: String,
    pub settlement_id: Option<i64>,
    pub ws_data_version: Option<i64>,
    pub year_data_version: Option<i64>,
}

pub fn get(conn: &Connection, job_id: i64) -> AppResult<JobHead> {
    conn.query_row(
        "SELECT id, workspace_id, status, source_name, source_sha256, settlement_id,
                ws_data_version, year_data_version
           FROM dream_job WHERE id = ?1",
        params![job_id],
        |r| {
            Ok(JobHead {
                id: r.get(0)?,
                workspace_id: r.get(1)?,
                status: r.get(2)?,
                source_name: r.get(3)?,
                source_sha256: r.get(4)?,
                settlement_id: r.get(5)?,
                ws_data_version: r.get(6)?,
                year_data_version: r.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("처리 이력을 찾지 못했습니다. 매칭 및 검증을 다시 시작해 주세요."))
}

/// 다시 검증할 때 줄 기록을 통째로 새로 쓴다 — 늘 원본에서 다시 계산한 결과만 남긴다.
pub fn clear_rows(conn: &Connection, job_id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM dream_job_row WHERE job_id = ?1", params![job_id])?;
    Ok(())
}

/// 이 작업공간에서 아직 끝나지 않은 작업의 배분 결과(after)를 비운다 — 우선순위를 바꿨을 때.
/// 돌려주는 값은 결과가 있었던 작업 id 들. 그 작업마다 무효로 했다는 기록을 남긴다.
pub fn clear_after(conn: &Connection, workspace_id: i64, why: &str) -> AppResult<Vec<i64>> {
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT DISTINCT r.job_id FROM dream_job_row r JOIN dream_job j ON j.id = r.job_id
              WHERE j.workspace_id = ?1 AND j.status IN ('OPEN', 'BLOCKED', 'READY')
                AND (r.after_o IS NOT NULL OR r.after_p IS NOT NULL OR r.after_q IS NOT NULL)
              ORDER BY r.job_id",
        )?
        .query_map(params![workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in &ids {
        conn.execute(
            "UPDATE dream_job_row SET after_o = NULL, after_p = NULL, after_q = NULL WHERE job_id = ?1",
            params![id],
        )?;
        add_event(conn, *id, "ALLOCATION_INVALIDATED", why)?;
    }
    Ok(ids)
}

/// 가장 최근의 그 종류 기록(`detail`).
pub fn last_event(conn: &Connection, job_id: i64, kind: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT detail FROM dream_job_event WHERE job_id = ?1 AND kind = ?2 ORDER BY id DESC LIMIT 1",
            params![job_id, kind],
            |r| r.get(0),
        )
        .optional()?)
}

/// 행 번호와 그 줄의 저장된 O · P · Q(하나라도 비었으면 None).
pub type StoredAfter = (u32, Option<(i64, i64, i64)>);

/// 줄마다 저장된 배분 결과(행 번호 차례).
pub fn stored_after(conn: &Connection, job_id: i64) -> AppResult<Vec<StoredAfter>> {
    let mut st = conn.prepare(
        "SELECT source_row, after_o, after_p, after_q FROM dream_job_row WHERE job_id = ?1 ORDER BY source_row",
    )?;
    let rows = st
        .query_map(params![job_id], |r| {
            let (o, p, q): (Option<i64>, Option<i64>, Option<i64>) = (r.get(1)?, r.get(2)?, r.get(3)?);
            Ok((r.get::<_, u32>(0)?, o.zip(p).zip(q).map(|((o, p), q)| (o, p, q))))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// 이 작업공간에서 끝나지 않은 작업을 모두 닫는다(FAILED) — 적용 학년처럼 검증 대상 자체가
/// 바뀌어 그 작업의 매칭 · 배분을 다시 쓸 수 없을 때. 배분 결과도 비운다. 기록은 지우지 않는다.
pub fn close_open(conn: &Connection, workspace_id: i64, why: &str) -> AppResult<usize> {
    clear_after(conn, workspace_id, why)?;
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM dream_job WHERE workspace_id = ?1 AND status IN ('OPEN', 'BLOCKED', 'READY')")?
        .query_map(params![workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in &ids {
        set_status(conn, *id, "FAILED", None)?;
        add_event(conn, *id, "FAILED", why)?;
    }
    Ok(ids.len())
}
