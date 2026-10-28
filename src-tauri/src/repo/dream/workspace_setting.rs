//! 작업공간별 방과후드림 결제 기준월(`YYYYMM`) — `dream_workspace_setting`(010).
//!
//! 결제파일 I열(수강월)과 맞대어 볼 값이다. 작업공간의 시작일 · 끝일로 짐작하지 않는다.
//! 이 표에는 자료판 트리거가 없으므로 바꿔도 정산이 낡음이 되지 않는다.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::dream;
use crate::error::{AppError, AppResult};

pub fn billing_month(conn: &Connection, workspace_id: i64) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT billing_month FROM dream_workspace_setting WHERE workspace_id = ?1",
            params![workspace_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// 사람이 정한다. 형식이 틀리면 저장하지 않는다.
pub fn set_billing_month(conn: &Connection, workspace_id: i64, raw: &str) -> AppResult<String> {
    let v = dream::check_billing_month(raw).map_err(AppError::invalid)?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM workspace WHERE id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )?;
    if n == 0 {
        return Err(AppError::not_found("작업공간을 찾지 못했습니다."));
    }
    conn.execute(
        "INSERT INTO dream_workspace_setting (workspace_id, billing_month) VALUES (?1, ?2)
         ON CONFLICT(workspace_id) DO UPDATE
           SET billing_month = excluded.billing_month, updated_at = datetime('now', 'localtime')",
        params![workspace_id, v],
    )?;
    Ok(v)
}
