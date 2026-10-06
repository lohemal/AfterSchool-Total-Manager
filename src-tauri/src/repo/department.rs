//! 부서정보 — 작업공간 소속. 수강료·강사·요일이 기간마다 다르기 때문이다.
//!
//! 여기 있는 금액은 부서의 **기준** 수강료다. 학생이 실제로 내는 금액은 `charge`에
//! 따로 있고, 기준을 고쳤다고 자동으로 따라 바뀌지 않는다 (설계안 4-2, §42-3).

use std::collections::HashMap;

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
        "SELECT d.id, d.name, d.class_name, d.teacher, d.days, d.note, d.capacity,
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
                capacity: r.get(6)?,
                allowed_grades: Vec::new(),
                fees: Vec::new(),
                total: 0,
                enrollment_count: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    // 수강 가능 학년은 부서마다 다시 묻지 않고 **한 번에** 읽어 나눠 담는다.
    let mut grades = grades_of_workspace(conn, workspace_id)?;
    for d in rows.iter_mut() {
        d.fees = fees_of(conn, d.id)?;
        d.total = d.fees.iter().map(|f| f.amount).sum();
        d.allowed_grades = grades.remove(&d.id).unwrap_or_default();
    }
    Ok(rows)
}

/// 한 작업공간의 모든 부서에 대해 `부서 id → 수강 가능 학년` 을 한 번에 읽는다.
pub fn grades_of_workspace(
    conn: &Connection,
    workspace_id: i64,
) -> AppResult<HashMap<i64, Vec<i64>>> {
    let mut st = conn.prepare(
        "SELECT g.department_id, g.grade
           FROM department_allowed_grade g
           JOIN department d ON d.id = g.department_id
          WHERE d.workspace_id = ?1
          ORDER BY g.department_id, g.grade",
    )?;
    let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, grade) in rows {
        out.entry(id).or_default().push(grade);
    }
    Ok(out)
}

/// 부서 하나의 수강 가능 학년. **빈 목록은 미설정이다.**
pub fn grades_of(conn: &Connection, department_id: i64) -> AppResult<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT grade FROM department_allowed_grade
          WHERE department_id = ?1 ORDER BY grade",
    )?;
    let rows = st
        .query_map(params![department_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 정원이 업무에서 말이 되는 값인가.
///
/// 0 을 막는 까닭: '0명만 받는 반'은 업무에 없고, 미설정을 0 으로 적으면
/// 충원율이 거짓으로 100% 가 된다. DB 의 CHECK 와 같은 규칙을 여기서도 보는
/// 까닭은, 사용자에게 보일 말을 우리말로 돌려주기 위해서다.
fn check_capacity(capacity: Option<i64>) -> AppResult<()> {
    match capacity {
        Some(n) if n < 1 => Err(AppError::invalid(
            "정원은 1명 이상이어야 합니다. 정하지 않으려면 비워 두세요.",
        )),
        _ => Ok(()),
    }
}

/// 수강 가능 학년을 다시 쓴다. 빈 목록을 주면 **미설정으로 되돌린다.**
fn save_grades(conn: &Connection, department_id: i64, grades: &[i64]) -> AppResult<()> {
    let clean = crate::domain::clean_grades(grades);
    if clean.len() != grades.len() {
        // 조용히 버리지 않는다 — 1~6 밖의 학년이 왔다는 것은 부르는 쪽이
        // 무언가 잘못 알고 있다는 뜻이다.
        return Err(AppError::invalid(
            "수강 가능 학년은 1~6학년 안에서 골라 주세요.",
        ));
    }
    conn.execute(
        "DELETE FROM department_allowed_grade WHERE department_id = ?1",
        params![department_id],
    )?;
    for g in &clean {
        conn.execute(
            "INSERT INTO department_allowed_grade (department_id, grade) VALUES (?1, ?2)",
            params![department_id, g],
        )?;
    }
    Ok(())
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
        // `WHERE amount IS NOT excluded.amount` 가 없으면, 금액을 하나도 고치지
        // 않고 [저장]만 눌러도 `ws_bump_fee_u` 트리거가 돌아 작업공간 자료판이
        // 오른다. 그러면 멀쩡한 정산이 '낡음'이 되어 담당자가 까닭 없이 다시
        // 만들게 된다. 값이 그대로면 아무것도 쓰지 않는다.
        conn.execute(
            "INSERT INTO department_fee (department_id, item_code, amount) VALUES (?1, ?2, ?3)
             ON CONFLICT(department_id, item_code) DO UPDATE SET amount = excluded.amount
              WHERE department_fee.amount IS NOT excluded.amount",
            params![department_id, f.item_code, f.amount],
        )?;
    }
    Ok(())
}

pub fn create(conn: &Connection, workspace_id: i64, input: &DepartmentInput) -> AppResult<i64> {
    let name = required_text("부서명", &input.name)?;
    check_capacity(input.capacity)?;
    conn.execute(
        "INSERT INTO department (workspace_id, name, class_name, teacher, days, note, capacity)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            workspace_id,
            name,
            input.class_name.clone().unwrap_or_default().trim(),
            input.teacher.clone().unwrap_or_default().trim(),
            input.days.clone().unwrap_or_default().trim(),
            input.note.clone().unwrap_or_default(),
            input.capacity
        ],
    )?;
    let id = conn.last_insert_rowid();
    save_fees(conn, id, &input.fees)?;
    save_grades(conn, id, &input.allowed_grades)?;
    Ok(id)
}

/// 부서를 고친다.
///
/// ## 왜 고치기 전 값을 먼저 읽는가
///
/// 기준금액 · 정원 · 수강 가능 학년이 바뀌면 **변경이력에 남긴다**. v0.1.6
/// 운영 중에 "부서 기준금액이 언제 왜 바뀌었는지" 를 아무도 되짚을 수 없어
/// 금액 조사에 시간이 걸렸다. 셋은 서로 다른 사건이므로 줄도 따로 남긴다.
///
/// 값이 **실제로 바뀐 것만** 남긴다. 아무것도 고치지 않고 [저장]을 눌렀을 때
/// 이력이 쌓이면 이력을 믿을 수 없게 된다.
pub fn update(conn: &Connection, id: i64, input: &DepartmentInput) -> AppResult<()> {
    let name = required_text("부서명", &input.name)?;
    check_capacity(input.capacity)?;

    let before_fees = fees_of(conn, id)?;
    let before_grades = grades_of(conn, id)?;
    let before_capacity: Option<i64> = conn
        .query_row(
            "SELECT capacity FROM department WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    let n = conn.execute(
        "UPDATE department
            SET name = ?2, class_name = ?3, teacher = ?4, days = ?5, note = ?6, capacity = ?7
          WHERE id = ?1",
        params![
            id,
            name,
            input.class_name.clone().unwrap_or_default().trim(),
            input.teacher.clone().unwrap_or_default().trim(),
            input.days.clone().unwrap_or_default().trim(),
            input.note.clone().unwrap_or_default(),
            input.capacity
        ],
    )?;
    if n == 0 {
        return Err(AppError::not_found("부서를 찾지 못했습니다."));
    }
    save_fees(conn, id, &input.fees)?;
    save_grades(conn, id, &input.allowed_grades)?;

    log_edits(
        conn,
        id,
        &before_fees,
        before_capacity,
        &before_grades,
        input,
    )
}

/// 바뀐 것만 변경이력에 적는다.
fn log_edits(
    conn: &Connection,
    id: i64,
    before_fees: &[Fee],
    before_capacity: Option<i64>,
    before_grades: &[i64],
    input: &DepartmentInput,
) -> AppResult<()> {
    use crate::repo::change_log as log;

    let Some((year_id, workspace_id)) = conn
        .query_row(
            "SELECT w.year_id, w.id FROM department d JOIN workspace w ON w.id = d.workspace_id
              WHERE d.id = ?1",
            params![id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
    else {
        return Ok(());
    };
    let (dname, dclass) = name_of(conn, id)?;
    let target = format!("{dname}{dclass}");

    // 이력 문구는 `DEPT_APPLY` 와 같은 함수로 만든다 — 같은 부서의 금액이
    // 화면마다 다른 모양으로 적히면 이력을 나란히 읽을 수 없다.
    let items = crate::repo::cost_items(conn)?;
    let fee_text = |f: &[Fee]| crate::repo::enrollment::fees_text(&items, f);

    let after_fees = fees_of(conn, id)?;
    if fee_text(before_fees) != fee_text(&after_fees) {
        log::write(
            conn,
            year_id,
            Some(workspace_id),
            log::DEPT_FEE_EDIT,
            None,
            Some(id),
            &target,
            &fee_text(before_fees),
            &fee_text(&after_fees),
            "",
        )?;
    }

    if before_capacity != input.capacity {
        log::write(
            conn,
            year_id,
            Some(workspace_id),
            log::DEPT_CAPACITY_EDIT,
            None,
            Some(id),
            &target,
            &capacity_text(before_capacity),
            &capacity_text(input.capacity),
            "",
        )?;
    }

    let after_grades = crate::domain::clean_grades(&input.allowed_grades);
    if before_grades != after_grades.as_slice() {
        log::write(
            conn,
            year_id,
            Some(workspace_id),
            log::DEPT_ALLOWED_GRADE_EDIT,
            None,
            Some(id),
            &target,
            &crate::domain::allowed_grade_text(before_grades),
            &crate::domain::allowed_grade_text(&after_grades),
            "",
        )?;
    }
    Ok(())
}

fn capacity_text(capacity: Option<i64>) -> String {
    match capacity {
        Some(n) => format!("정원 {n}명"),
        None => "정원 미설정".to_string(),
    }
}


pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    crate::repo::adjustment::block_for_departments(conn, ids)?;
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM department WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub fn delete_all(conn: &Connection, workspace_id: i64) -> AppResult<usize> {
    crate::repo::adjustment::block_for_workspace(conn, workspace_id)?;
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
