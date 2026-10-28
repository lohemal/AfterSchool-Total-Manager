//! 방과후드림 강좌명코드 ↔ 부서 (작업공간 단위).
//!
//! 부서는 작업공간마다 새 id 이므로 매핑도 작업공간마다 따로 확정한다. 초기 정책은
//! **강좌코드 하나 ↔ 부서 하나**다. 같은 작업공간에서 한 코드를 두 부서에 잇거나,
//! 한 부서에 두 코드를 잇지 않는다. 기존 `department` 표는 고치지 않는다.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::dream;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseMap {
    pub department_id: i64,
    pub course_code: String,
    pub confirmed_at: String,
}

fn workspace_of(conn: &Connection, department_id: i64) -> AppResult<i64> {
    conn.query_row(
        "SELECT workspace_id FROM department WHERE id = ?1",
        params![department_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("부서를 찾지 못했습니다."))
}

/// 이 부서의 강좌코드를 확정한다(있으면 바꾼다).
pub fn set(conn: &Connection, department_id: i64, raw: &str) -> AppResult<()> {
    let code = dream::check_course_code(raw).map_err(AppError::invalid)?;
    let ws = workspace_of(conn, department_id)?;

    let holder: Option<i64> = conn
        .query_row(
            "SELECT department_id FROM dream_course_map WHERE workspace_id = ?1 AND course_code = ?2",
            params![ws, code],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(other) = holder {
        if other != department_id {
            return Err(AppError::new(
                "CONFLICT",
                "이 강좌명코드는 이 작업공간의 다른 부서에 이미 연결되어 있습니다.",
            ));
        }
        return Ok(()); // 같은 값 — 할 일이 없다
    }

    conn.execute(
        "INSERT INTO dream_course_map (workspace_id, department_id, course_code)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(department_id) DO UPDATE
           SET course_code = excluded.course_code,
               confirmed_at = datetime('now', 'localtime')",
        params![ws, department_id, code],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, department_id: i64) -> AppResult<()> {
    conn.execute(
        "DELETE FROM dream_course_map WHERE department_id = ?1",
        params![department_id],
    )?;
    Ok(())
}

/// 작업공간의 확정 매핑 — 강좌코드 차례.
pub fn list(conn: &Connection, workspace_id: i64) -> AppResult<Vec<CourseMap>> {
    let mut st = conn.prepare(
        "SELECT department_id, course_code, confirmed_at FROM dream_course_map
          WHERE workspace_id = ?1 ORDER BY course_code",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok(CourseMap {
                department_id: r.get(0)?,
                course_code: r.get(1)?,
                confirmed_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 강좌코드로 확정된 부서를 찾는다. 이름으로 짐작하지 않는다.
pub fn find(conn: &Connection, workspace_id: i64, raw: &str) -> AppResult<Option<i64>> {
    let code = dream::check_course_code(raw).map_err(AppError::invalid)?;
    Ok(conn
        .query_row(
            "SELECT department_id FROM dream_course_map WHERE workspace_id = ?1 AND course_code = ?2",
            params![workspace_id, code],
            |r| r.get(0),
        )
        .optional()?)
}
