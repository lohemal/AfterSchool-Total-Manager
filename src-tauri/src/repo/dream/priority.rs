//! 방과후드림 전용 우선순위 — 부서 차례와, 부서마다의 항목 차례.
//!
//! 기존 정산 우선순위(`dept_priority` · `item_priority`)와 **따로다.** 그 표들은 정산
//! 엔진의 입력이라 고치면 정산이 낡음이 된다. 여기서는 그 표를 읽기만 해서 기본값으로
//! 쓴다.
//!
//! ## 언제나 같은 차례 (결정적 대체 규칙)
//!
//! ```text
//! 부서  (방과후드림 순위 → 기존 정산 순위 → 부서명 → 반 이름 → 부서 id)
//! 항목  (그 부서의 방과후드림 순위 → 기존 정산 항목 순위 → 비용항목 차례 → 코드)
//! ```
//!
//! 설정하지 않은 것은 설정한 것 뒤에 온다. 같은 자료면 언제나 같은 차례가 나온다.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::repo;

/// 작업공간의 부서 차례를 통째로 저장한다. 목록에 없는 부서는 '미설정'이 된다.
pub fn save_dept_order(conn: &Connection, workspace_id: i64, order: &[i64]) -> AppResult<()> {
    let mine: HashSet<i64> = conn
        .prepare("SELECT id FROM department WHERE workspace_id = ?1")?
        .query_map(params![workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut seen = HashSet::new();
    for id in order {
        if !mine.contains(id) {
            return Err(AppError::invalid("이 작업공간의 부서가 아닌 것이 들어 있습니다."));
        }
        if !seen.insert(*id) {
            return Err(AppError::invalid("같은 부서가 두 번 들어 있습니다."));
        }
    }
    conn.execute(
        "DELETE FROM dream_dept_priority WHERE workspace_id = ?1",
        params![workspace_id],
    )?;
    for (i, id) in order.iter().enumerate() {
        conn.execute(
            "INSERT INTO dream_dept_priority (workspace_id, department_id, sort_order) VALUES (?1, ?2, ?3)",
            params![workspace_id, id, i as i64 + 1],
        )?;
    }
    Ok(())
}

/// 한 부서의 항목 차례를 통째로 저장한다.
pub fn save_item_order(conn: &Connection, department_id: i64, order: &[String]) -> AppResult<()> {
    let known: HashSet<String> = repo::cost_items(conn)?.into_iter().map(|i| i.code).collect();
    let mut seen = HashSet::new();
    for code in order {
        if !known.contains(code) {
            return Err(AppError::invalid("알 수 없는 비용항목이 들어 있습니다."));
        }
        if !seen.insert(code.as_str()) {
            return Err(AppError::invalid("같은 항목이 두 번 들어 있습니다."));
        }
    }
    repo::department::name_of(conn, department_id)?; // 없는 부서면 여기서 멈춘다
    conn.execute(
        "DELETE FROM dream_item_priority WHERE department_id = ?1",
        params![department_id],
    )?;
    for (i, code) in order.iter().enumerate() {
        conn.execute(
            "INSERT INTO dream_item_priority (department_id, item_code, sort_order) VALUES (?1, ?2, ?3)",
            params![department_id, code, i as i64 + 1],
        )?;
    }
    Ok(())
}

/// 방과후드림 순위가 저장되어 있는가 (화면에서 '기본값 사용 중'을 알릴 때).
pub fn has_dept_order(conn: &Connection, workspace_id: i64) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM dream_dept_priority WHERE workspace_id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn rank_map<K>(conn: &Connection, sql: &str, key: i64) -> AppResult<HashMap<K, i64>>
where
    K: rusqlite::types::FromSql + std::hash::Hash + Eq,
{
    let mut st = conn.prepare(sql)?;
    let rows = st
        .query_map(params![key], |r| Ok((r.get::<_, K>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

/// 작업공간의 모든 부서를 배분할 차례대로.
pub fn dept_order(conn: &Connection, workspace_id: i64) -> AppResult<Vec<i64>> {
    let dream: HashMap<i64, i64> = rank_map(
        conn,
        "SELECT department_id, sort_order FROM dream_dept_priority WHERE workspace_id = ?1",
        workspace_id,
    )?;
    // 기존 정산 순위 — 읽기만
    let settle: HashMap<i64, usize> = repo::priority::dept_order(conn, workspace_id)?
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();

    let mut st = conn.prepare("SELECT id, name, class_name FROM department WHERE workspace_id = ?1")?;
    let mut depts: Vec<(i64, String, String)> = st
        .query_map(params![workspace_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    depts.sort_by(|a, b| {
        let key = |d: &(i64, String, String)| {
            (
                dream.get(&d.0).copied().unwrap_or(i64::MAX),
                settle.get(&d.0).copied().unwrap_or(usize::MAX),
            )
        };
        key(a)
            .cmp(&key(b))
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    Ok(depts.into_iter().map(|d| d.0).collect())
}

/// 한 부서의 모든 (사용 중인) 비용항목을 배분할 차례대로.
pub fn item_order(conn: &Connection, department_id: i64) -> AppResult<Vec<String>> {
    let ws: i64 = conn.query_row(
        "SELECT workspace_id FROM department WHERE id = ?1",
        params![department_id],
        |r| r.get(0),
    )?;
    let dream: HashMap<String, i64> = rank_map(
        conn,
        "SELECT item_code, sort_order FROM dream_item_priority WHERE department_id = ?1",
        department_id,
    )?;
    // 기존 정산의 작업공간 항목 차례(저장된 것이 없으면 비용항목 차례) — 읽기만
    let settle: HashMap<String, usize> = repo::priority::item_order(conn, ws)?
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i))
        .collect();
    let mut items: Vec<(String, i64)> = repo::cost_items(conn)?
        .into_iter()
        .map(|i| (i.code, i.sort_order))
        .collect();
    items.sort_by(|a, b| {
        let key = |x: &(String, i64)| {
            (
                dream.get(&x.0).copied().unwrap_or(i64::MAX),
                settle.get(&x.0).copied().unwrap_or(usize::MAX),
                x.1,
            )
        };
        key(a).cmp(&key(b)).then_with(|| a.0.cmp(&b.0))
    });
    Ok(items.into_iter().map(|i| i.0).collect())
}
