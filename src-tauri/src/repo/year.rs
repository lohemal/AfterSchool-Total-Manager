//! 학년도 · 작업공간.
//!
//! 작업공간의 순서는 저장하지 않는다. 목록도 정산의 선행/후행 판단도 언제나
//! `(시작일, 종료일, id)` 하나만 쓴다 (설계안 4-2).

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::model::{Workspace, WorkspaceInput, Year};
use crate::repo::{check_date, required_text};

const YEAR_COLS: &str = "id, year, name, start_date, end_date, data_version, is_current";

fn map_year(r: &rusqlite::Row) -> rusqlite::Result<Year> {
    Ok(Year {
        id: r.get(0)?,
        year: r.get(1)?,
        name: r.get(2)?,
        start_date: r.get(3)?,
        end_date: r.get(4)?,
        data_version: r.get(5)?,
        is_current: r.get::<_, i64>(6)? == 1,
    })
}

pub fn list_years(conn: &Connection) -> AppResult<Vec<Year>> {
    let sql = format!("SELECT {YEAR_COLS} FROM academic_year ORDER BY year DESC");
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map([], map_year)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn current_year(conn: &Connection) -> AppResult<Option<Year>> {
    let sql = format!("SELECT {YEAR_COLS} FROM academic_year WHERE is_current = 1");
    let row = conn.query_row(&sql, [], map_year).optional()?;
    Ok(row)
}

pub fn get_year(conn: &Connection, id: i64) -> AppResult<Year> {
    let sql = format!("SELECT {YEAR_COLS} FROM academic_year WHERE id = ?1");
    conn.query_row(&sql, params![id], map_year)
        .optional()?
        .ok_or_else(|| AppError::not_found("학년도를 찾지 못했습니다."))
}

/// 학년도를 만들고 현재 학년도로 지정한다. 지원정책 두 줄(이용권·자유수강권)을 함께 만든다.
///
/// 정책 값은 **비워 둔다** — 한도·대상학년은 시도마다 다르므로 프로그램이 임의로
/// 정하지 않고 `시스템 › 학년도 지원금 설정`에서 받는다.
pub fn create_year(conn: &Connection, year: i64, name: &str) -> AppResult<i64> {
    if !(2000..=2100).contains(&year) {
        return Err(AppError::invalid("학년도는 2000~2100 사이여야 합니다."));
    }
    let name = required_text("학년도 이름", name)?;
    let start = format!("{year}-03-01");
    let end = format!("{}-02-28", year + 1);

    conn.execute(
        "INSERT INTO academic_year (year, name, start_date, end_date) VALUES (?1, ?2, ?3, ?4)",
        params![year, name, start, end],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::new("CONFLICT", format!("{year}학년도가 이미 있습니다."))
        } else {
            e.into()
        }
    })?;
    let id = conn.last_insert_rowid();

    for program in ["VOUCHER", "FREE_VOUCHER"] {
        conn.execute(
            "INSERT INTO support_policy (year_id, program) VALUES (?1, ?2)",
            params![id, program],
        )?;
    }

    set_current_year(conn, id)?;
    Ok(id)
}

pub fn set_current_year(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("UPDATE academic_year SET is_current = 0", [])?;
    let n = conn.execute(
        "UPDATE academic_year SET is_current = 1 WHERE id = ?1",
        params![id],
    )?;
    if n == 0 {
        return Err(AppError::not_found("학년도를 찾지 못했습니다."));
    }
    Ok(())
}

pub fn update_year(conn: &Connection, id: i64, name: &str, start: &str, end: &str) -> AppResult<()> {
    let name = required_text("학년도 이름", name)?;
    check_date("시작일", start)?;
    check_date("종료일", end)?;
    if start > end {
        return Err(AppError::invalid("종료일이 시작일보다 빠릅니다."));
    }
    conn.execute(
        "UPDATE academic_year SET name = ?2, start_date = ?3, end_date = ?4 WHERE id = ?1",
        params![id, name, start, end],
    )?;
    Ok(())
}

pub fn delete_year(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM academic_year WHERE id = ?1", params![id])?;
    // 현재 학년도가 사라졌으면 가장 최근 학년도를 현재로 만든다.
    if current_year(conn)?.is_none() {
        if let Some(next) = conn
            .query_row(
                "SELECT id FROM academic_year ORDER BY year DESC LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            set_current_year(conn, next)?;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────── 작업공간
//
// **순서를 저장하지 않는다.** 목록 표시도, 정산의 선행/후행 판단도 언제나
// `(start_date, end_date, id)` 하나만 쓴다. 사람이 만질 수 있는 순서 컬럼을 두면
// 화면에 보이는 순서와 지원금 누적 순서가 갈릴 수 있고, 그때 나오는 오차는
// 조용해서 알아채기 어렵다.

/// 목록·누적 계산에서 공통으로 쓰는 정렬. 시작일이 같으면 종료일, 그다음 id.
pub const WS_ORDER: &str = "w.start_date, w.end_date, w.id";

const WS_SELECT: &str = "
SELECT w.id, w.year_id, w.name, w.start_date, w.end_date,
       w.data_version, w.is_current, w.note,
       (SELECT COUNT(*) FROM department d WHERE d.workspace_id = w.id),
       (SELECT COUNT(*) FROM enrollment e WHERE e.workspace_id = w.id AND e.status = 'ACTIVE')
FROM workspace w";

fn map_ws(r: &rusqlite::Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        year_id: r.get(1)?,
        name: r.get(2)?,
        start_date: r.get(3)?,
        end_date: r.get(4)?,
        data_version: r.get(5)?,
        is_current: r.get::<_, i64>(6)? == 1,
        note: r.get(7)?,
        department_count: r.get(8)?,
        enrollment_count: r.get(9)?,
    })
}

pub fn list_workspaces(conn: &Connection, year_id: i64) -> AppResult<Vec<Workspace>> {
    let sql = format!("{WS_SELECT} WHERE w.year_id = ?1 ORDER BY {WS_ORDER}");
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map(params![year_id], map_ws)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_workspace(conn: &Connection, id: i64) -> AppResult<Workspace> {
    let sql = format!("{WS_SELECT} WHERE w.id = ?1");
    conn.query_row(&sql, params![id], map_ws)
        .optional()?
        .ok_or_else(|| AppError::not_found("작업공간을 찾지 못했습니다."))
}

pub fn current_workspace(conn: &Connection, year_id: i64) -> AppResult<Option<Workspace>> {
    let sql = format!("{WS_SELECT} WHERE w.year_id = ?1 AND w.is_current = 1");
    let row = conn.query_row(&sql, params![year_id], map_ws).optional()?;
    if row.is_some() {
        return Ok(row);
    }
    // 지정된 것이 없으면 날짜상 마지막 작업공간을 쓴다.
    let sql = format!("{WS_SELECT} WHERE w.year_id = ?1 ORDER BY {WS_ORDER} DESC LIMIT 1");
    Ok(conn.query_row(&sql, params![year_id], map_ws).optional()?)
}

/// 정산에서 "이 작업공간보다 앞선" 작업공간들. 목록 화면과 같은 순서를 쓴다.
///
/// SQLite의 행 값 비교로 `(시작일, 종료일, id)` 세 값을 한 번에 견준다.
pub fn workspaces_before(conn: &Connection, workspace: &Workspace) -> AppResult<Vec<Workspace>> {
    let sql = format!(
        "{WS_SELECT}
          WHERE w.year_id = ?1
            AND (w.start_date, w.end_date, w.id) < (?2, ?3, ?4)
          ORDER BY {WS_ORDER}"
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map(
            params![
                workspace.year_id,
                workspace.start_date,
                workspace.end_date,
                workspace.id
            ],
            map_ws,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 기간이 겹치는 다른 작업공간들. 저장을 막지는 않고 화면에서 확인만 받는다.
///
/// 겹치는 운영이 실제로 있기 때문이다 (1학기와 여름방학 특강처럼).
/// 다만 날짜는 지원금 누적 순서를 정하므로 실수하기 어렵게 한 번 알려 준다.
pub fn overlapping_workspaces(
    conn: &Connection,
    year_id: i64,
    exclude_id: Option<i64>,
    start: &str,
    end: &str,
) -> AppResult<Vec<Workspace>> {
    check_date("시작일", start)?;
    check_date("종료일", end)?;
    let sql = format!(
        "{WS_SELECT}
          WHERE w.year_id = ?1
            AND w.id <> COALESCE(?2, -1)
            AND w.start_date <= ?4
            AND w.end_date   >= ?3
          ORDER BY {WS_ORDER}"
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st
        .query_map(params![year_id, exclude_id, start, end], map_ws)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn check_input(input: &WorkspaceInput) -> AppResult<String> {
    let name = required_text("작업공간명", &input.name)?;
    check_date("시작일", &input.start_date)?;
    check_date("종료일", &input.end_date)?;
    if input.start_date > input.end_date {
        return Err(AppError::invalid("종료일이 시작일보다 빠릅니다."));
    }
    Ok(name)
}

pub fn create_workspace(conn: &Connection, year_id: i64, input: &WorkspaceInput) -> AppResult<i64> {
    let name = check_input(input)?;
    conn.execute(
        "INSERT INTO workspace (year_id, name, start_date, end_date, note)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            year_id,
            name,
            input.start_date,
            input.end_date,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    let id = conn.last_insert_rowid();
    set_current_workspace(conn, id)?;
    Ok(id)
}

pub fn update_workspace(conn: &Connection, id: i64, input: &WorkspaceInput) -> AppResult<()> {
    let name = check_input(input)?;
    let n = conn.execute(
        "UPDATE workspace SET name = ?2, start_date = ?3, end_date = ?4, note = ?5 WHERE id = ?1",
        params![
            id,
            name,
            input.start_date,
            input.end_date,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    if n == 0 {
        return Err(AppError::not_found("작업공간을 찾지 못했습니다."));
    }
    Ok(())
}

pub fn delete_workspace(conn: &Connection, id: i64) -> AppResult<()> {
    let n = conn.execute("DELETE FROM workspace WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::not_found("작업공간을 찾지 못했습니다."));
    }
    Ok(())
}

pub fn set_current_workspace(conn: &Connection, id: i64) -> AppResult<()> {
    let year_id: i64 = conn
        .query_row(
            "SELECT year_id FROM workspace WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("작업공간을 찾지 못했습니다."))?;
    conn.execute(
        "UPDATE workspace SET is_current = 0 WHERE year_id = ?1",
        params![year_id],
    )?;
    conn.execute(
        "UPDATE workspace SET is_current = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}
