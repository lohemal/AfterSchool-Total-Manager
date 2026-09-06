//! 차감 우선순위 — 부서와 비용항목. 둘 다 **작업공간 소속**이다 (설계안 0-6).
//!
//! 부서 목록에는 그 작업공간에서 **이용권 대상 학생이 실제 수강 중인 부서**만
//! 올린다. 쓰지도 않을 부서를 늘어놓으면 순서를 정하기 어려워지기 때문이다.

use rusqlite::{params, Connection};

use crate::domain::{eligibility_active, grade_matches, parse_grades};
use crate::error::AppResult;
use crate::model::PriorityRow;
use crate::repo::enrollment::dept_label;

/// 저장된 부서 순서 + 지금 필요한 부서를 합쳐 돌려준다.
///
/// 저장된 순서를 앞에 두고, 새로 생긴 부서는 뒤에 붙인다.
/// 이제 수강생이 없는 부서는 목록에서 빠진다(저장된 행은 그대로 두어도 무해하다).
pub fn dept_list(conn: &Connection, workspace_id: i64) -> AppResult<Vec<PriorityRow>> {
    let (year_id, ws_start, ws_end): (i64, String, String) = conn.query_row(
        "SELECT year_id, start_date, end_date FROM workspace WHERE id = ?1",
        params![workspace_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let targets = parse_grades(
        &conn
            .query_row(
                "SELECT target_grades FROM support_policy WHERE year_id = ?1 AND program = 'VOUCHER'",
                params![year_id],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default(),
    );

    // 이용권 자격이 이 작업공간 기간에 유효한 학생들
    let mut st = conn.prepare(
        "SELECT DISTINCT e.department_id, d.name, d.class_name, s.id, s.grade,
                el.valid_from, el.valid_to
           FROM enrollment e
           JOIN department d ON d.id = e.department_id
           JOIN student s    ON s.id = e.student_id
           JOIN support_eligibility el
                  ON el.student_id = s.id AND el.year_id = ?1 AND el.program = 'VOUCHER'
          WHERE e.workspace_id = ?2 AND e.status = 'ACTIVE'",
    )?;
    let raw = st
        .query_map(params![year_id, workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let mut found: std::collections::HashMap<i64, (String, std::collections::HashSet<i64>)> =
        std::collections::HashMap::new();
    for (dept_id, name, class_name, student_id, grade, from, to) in raw {
        if !grade_matches(&targets, grade) {
            continue;
        }
        if !eligibility_active(from.as_deref(), to.as_deref(), &ws_start, &ws_end) {
            continue;
        }
        found
            .entry(dept_id)
            .or_insert_with(|| (dept_label(&name, &class_name), Default::default()))
            .1
            .insert(student_id);
    }

    // 저장된 순서
    let mut st = conn.prepare(
        "SELECT department_id, sort_order FROM dept_priority
          WHERE workspace_id = ?1 ORDER BY sort_order",
    )?;
    let saved: Vec<(i64, i64)> = st
        .query_map(params![workspace_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let mut out: Vec<PriorityRow> = Vec::new();
    for (dept_id, _) in &saved {
        if let Some((label, students)) = found.remove(dept_id) {
            out.push(PriorityRow {
                key: dept_id.to_string(),
                label,
                sort_order: out.len() as i64 + 1,
                voucher_students: students.len() as i64,
            });
        }
    }
    // 저장되지 않은 부서는 이름 순으로 뒤에 붙인다
    let mut rest: Vec<(i64, String, i64)> = found
        .into_iter()
        .map(|(id, (label, students))| (id, label, students.len() as i64))
        .collect();
    rest.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    for (id, label, n) in rest {
        out.push(PriorityRow {
            key: id.to_string(),
            label,
            sort_order: out.len() as i64 + 1,
            voucher_students: n,
        });
    }
    Ok(out)
}

pub fn dept_save(conn: &Connection, workspace_id: i64, order: &[i64]) -> AppResult<()> {
    conn.execute(
        "DELETE FROM dept_priority WHERE workspace_id = ?1",
        params![workspace_id],
    )?;
    for (i, id) in order.iter().enumerate() {
        conn.execute(
            "INSERT INTO dept_priority (workspace_id, department_id, sort_order)
             VALUES (?1, ?2, ?3)",
            params![workspace_id, id, i as i64 + 1],
        )?;
    }
    Ok(())
}

/// 저장된 항목 순서. 없으면 `cost_item.sort_order`가 기본이다.
pub fn item_list(conn: &Connection, workspace_id: i64) -> AppResult<Vec<PriorityRow>> {
    let mut st = conn.prepare(
        "SELECT c.code, c.name,
                COALESCE(p.sort_order, 1000 + c.sort_order) AS ord
           FROM cost_item c
           LEFT JOIN item_priority p
                  ON p.item_code = c.code AND p.workspace_id = ?1
          WHERE c.is_active = 1
          ORDER BY ord, c.sort_order",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok(PriorityRow {
                key: r.get(0)?,
                label: r.get(1)?,
                sort_order: 0,
                voucher_students: 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, mut r)| {
            r.sort_order = i as i64 + 1;
            r
        })
        .collect())
}

pub fn item_save(conn: &Connection, workspace_id: i64, order: &[String]) -> AppResult<()> {
    conn.execute(
        "DELETE FROM item_priority WHERE workspace_id = ?1",
        params![workspace_id],
    )?;
    for (i, code) in order.iter().enumerate() {
        conn.execute(
            "INSERT INTO item_priority (workspace_id, item_code, sort_order)
             VALUES (?1, ?2, ?3)",
            params![workspace_id, code, i as i64 + 1],
        )?;
    }
    Ok(())
}

/// 정산 엔진에 넘길 부서 차례. 저장된 것이 없으면 빈 목록(=부서 id 순).
pub fn dept_order(conn: &Connection, workspace_id: i64) -> AppResult<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT department_id FROM dept_priority WHERE workspace_id = ?1 ORDER BY sort_order",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 정산 엔진에 넘길 항목 차례. 저장된 것이 없으면 `cost_item.sort_order`.
pub fn item_order(conn: &Connection, workspace_id: i64) -> AppResult<Vec<String>> {
    Ok(item_list(conn, workspace_id)?
        .into_iter()
        .map(|r| r.key)
        .collect())
}

pub fn has_dept_priority(conn: &Connection, workspace_id: i64) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM dept_priority WHERE workspace_id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}
