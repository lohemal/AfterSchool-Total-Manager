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

// ─────────────────────────────────────────────── 적용 학년 (P6, 011)

/// 방과후드림 적용 학년(오름차순). 비어 있으면 **미설정** — 매칭 · 배분 · 결과 파일을 만들지 않는다.
pub fn applied_grades(conn: &Connection, workspace_id: i64) -> AppResult<Vec<i64>> {
    let mut st = conn.prepare("SELECT grade FROM dream_applied_grade WHERE workspace_id = ?1 ORDER BY grade")?;
    let rows = st
        .query_map(params![workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(rows)
}

/// 적용 학년 저장. 1개 이상, 1~9 학년(학생 학년과 같은 범위), 중복은 하나로. 집합으로 견주어
/// 실제로 바뀌었으면 `true` — 차례만 다른 같은 학년이면 바뀐 것이 아니다.
pub fn set_applied_grades(conn: &Connection, workspace_id: i64, grades: &[i64]) -> AppResult<(Vec<i64>, bool)> {
    let mut v: Vec<i64> = grades.to_vec();
    v.sort_unstable();
    v.dedup();
    if v.is_empty() {
        return Err(AppError::new("APPLIED_GRADES_NOT_SET", "방과후드림 적용 학년을 1개 이상 선택해 주세요."));
    }
    if v.iter().any(|g| !(1..=9).contains(g)) {
        return Err(AppError::invalid("학년은 1~9 사이여야 합니다."));
    }
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM workspace WHERE id = ?1", params![workspace_id], |r| r.get(0))?;
    if n == 0 {
        return Err(AppError::not_found("작업공간을 찾지 못했습니다."));
    }
    let before = applied_grades(conn, workspace_id)?;
    if before == v {
        return Ok((v, false));
    }
    conn.execute("DELETE FROM dream_applied_grade WHERE workspace_id = ?1", params![workspace_id])?;
    for g in &v {
        conn.execute(
            "INSERT INTO dream_applied_grade (workspace_id, grade) VALUES (?1, ?2)",
            params![workspace_id, g],
        )?;
    }
    Ok((v, true))
}

/// 제안용 — 같은 학년도의 앞선 작업공간 가운데 가장 가까운 곳의 적용 학년. 저장하지 않는다.
pub fn suggested_grades(conn: &Connection, workspace_id: i64) -> AppResult<Option<(String, Vec<i64>)>> {
    let ws = crate::repo::year::get_workspace(conn, workspace_id)?;
    for prev in crate::repo::year::workspaces_before(conn, &ws)?.iter().rev() {
        let g = applied_grades(conn, prev.id)?;
        if !g.is_empty() {
            return Ok(Some((prev.name.clone(), g)));
        }
    }
    Ok(None)
}

/// `[3, 4]` → `3학년 · 4학년`
pub fn grades_label(grades: &[i64]) -> String {
    grades.iter().map(|g| format!("{g}학년")).collect::<Vec<_>>().join(" · ")
}
