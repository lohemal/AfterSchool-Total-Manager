//! 부서정보 — 작업공간 소속. 수강료·강사·요일이 기간마다 다르기 때문이다.
//!
//! 여기 있는 금액은 부서의 **기준** 수강료다. 학생이 실제로 내는 금액은 `charge`에
//! 따로 있고, 기준을 고쳤다고 자동으로 따라 바뀌지 않는다 (설계안 4-2, §42-3).

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::model::{Department, DepartmentInput, Fee};
use crate::repo::required_text;

/// Excel 업로드 한 줄.
#[derive(Debug, Clone)]
pub struct DepartmentRow {
    pub name: String,
    pub class_name: String,
    pub teacher: String,
    pub days: String,
    pub fees: Vec<Fee>,
}

/// 부서 이름표만 필요한 곳에서 쓴다 (`로봇과학`, `A반`).
pub fn name_of(conn: &Connection, department_id: i64) -> AppResult<(String, String)> {
    conn.query_row(
        "SELECT name, class_name FROM department WHERE id = ?1",
        params![department_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| crate::error::AppError::not_found("부서를 찾지 못했습니다."))
}

pub fn list(conn: &Connection, workspace_id: i64, query: Option<&str>) -> AppResult<Vec<Department>> {
    let like = query
        .map(|q| q.trim())
        .filter(|q| !q.is_empty())
        .map(|q| format!("%{q}%"));

    let mut st = conn.prepare(
        "SELECT d.id, d.name, d.class_name, d.teacher, d.days, d.note,
                (SELECT COUNT(*) FROM enrollment e
                  WHERE e.department_id = d.id AND e.status = 'ACTIVE')
           FROM department d
          WHERE d.workspace_id = ?1
            AND (?2 IS NULL OR d.name LIKE ?2 OR d.class_name LIKE ?2 OR d.teacher LIKE ?2)
          ORDER BY d.name, d.class_name",
    )?;
    let mut rows = st
        .query_map(params![workspace_id, like], |r| {
            Ok(Department {
                id: r.get(0)?,
                name: r.get(1)?,
                class_name: r.get(2)?,
                teacher: r.get(3)?,
                days: r.get(4)?,
                note: r.get(5)?,
                fees: Vec::new(),
                total: 0,
                enrollment_count: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for d in rows.iter_mut() {
        d.fees = fees_of(conn, d.id)?;
        d.total = d.fees.iter().map(|f| f.amount).sum();
    }
    Ok(rows)
}

pub fn fees_of(conn: &Connection, department_id: i64) -> AppResult<Vec<Fee>> {
    let mut st = conn.prepare(
        "SELECT f.item_code, f.amount
           FROM department_fee f JOIN cost_item c ON c.code = f.item_code
          WHERE f.department_id = ?1
          ORDER BY c.sort_order",
    )?;
    let rows = st
        .query_map(params![department_id], |r| {
            Ok(Fee {
                item_code: r.get(0)?,
                amount: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn save_fees(conn: &Connection, department_id: i64, fees: &[Fee]) -> AppResult<()> {
    for f in fees {
        if f.amount < 0 {
            return Err(AppError::invalid("금액은 0원 이상이어야 합니다."));
        }
        let known: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cost_item WHERE code = ?1",
            params![f.item_code],
            |r| r.get(0),
        )?;
        if known == 0 {
            return Err(AppError::invalid(format!(
                "알 수 없는 비용항목입니다: {}",
                f.item_code
            )));
        }
        conn.execute(
            "INSERT INTO department_fee (department_id, item_code, amount) VALUES (?1, ?2, ?3)
             ON CONFLICT(department_id, item_code) DO UPDATE SET amount = excluded.amount",
            params![department_id, f.item_code, f.amount],
        )?;
    }
    Ok(())
}

pub fn create(conn: &Connection, workspace_id: i64, input: &DepartmentInput) -> AppResult<i64> {
    let name = required_text("부서명", &input.name)?;
    conn.execute(
        "INSERT INTO department (workspace_id, name, class_name, teacher, days, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            workspace_id,
            name,
            input.class_name.clone().unwrap_or_default().trim(),
            input.teacher.clone().unwrap_or_default().trim(),
            input.days.clone().unwrap_or_default().trim(),
            input.note.clone().unwrap_or_default()
        ],
    )?;
    let id = conn.last_insert_rowid();
    save_fees(conn, id, &input.fees)?;
    Ok(id)
}

pub fn update(conn: &Connection, id: i64, input: &DepartmentInput) -> AppResult<()> {
    let name = required_text("부서명", &input.name)?;
    let n = conn.execute(
        "UPDATE department SET name = ?2, class_name = ?3, teacher = ?4, days = ?5, note = ?6
          WHERE id = ?1",
        params![
            id,
            name,
            input.class_name.clone().unwrap_or_default().trim(),
            input.teacher.clone().unwrap_or_default().trim(),
            input.days.clone().unwrap_or_default().trim(),
            input.note.clone().unwrap_or_default()
        ],
    )?;
    if n == 0 {
        return Err(AppError::not_found("부서를 찾지 못했습니다."));
    }
    save_fees(conn, id, &input.fees)?;
    Ok(())
}

pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM department WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub fn delete_all(conn: &Connection, workspace_id: i64) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM department WHERE workspace_id = ?1",
        params![workspace_id],
    )?)
}

/// Excel 업로드 반영. `부서명+반명`이 같으면 갱신하고, 없으면 새로 만든다.
pub fn upsert_bulk(
    conn: &Connection,
    workspace_id: i64,
    rows: &[DepartmentRow],
) -> AppResult<(usize, usize)> {
    let mut added = 0;
    let mut updated = 0;
    for r in rows {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM department
                  WHERE workspace_id = ?1 AND name = ?2 AND class_name = ?3",
                params![workspace_id, r.name, r.class_name],
                |row| row.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE department SET teacher = ?2, days = ?3 WHERE id = ?1",
                    params![id, r.teacher, r.days],
                )?;
                updated += 1;
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO department (workspace_id, name, class_name, teacher, days)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![workspace_id, r.name, r.class_name, r.teacher, r.days],
                )?;
                added += 1;
                conn.last_insert_rowid()
            }
        };
        save_fees(conn, id, &r.fees)?;
    }
    Ok((added, updated))
}

/// `부서명 + 반명`으로 찾는다. 수강 데이터 업로드 검증에서 쓴다.
pub fn find_by_name(
    conn: &Connection,
    workspace_id: i64,
    name: &str,
    class_name: &str,
) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM department WHERE workspace_id = ?1 AND name = ?2 AND class_name = ?3",
            params![workspace_id, name, class_name],
            |r| r.get(0),
        )
        .optional()?)
}
