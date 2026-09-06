//! SQL 계층. 표 하나(또는 한 묶음)당 한 파일.
//!
//! 쓰기 함수는 `rusqlite::Transaction`을 `&Connection`으로 받는다
//! (`Transaction`이 `Deref<Target = Connection>`이므로 그대로 넘길 수 있다).

pub mod change_log;
pub mod department;
pub mod eligibility;
pub mod enrollment;
pub mod policy;
pub mod proposal;
pub mod priority;
pub mod setting;
pub mod settle;
pub mod student;
pub mod year;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::model::CostItem;

pub fn cost_items(conn: &Connection) -> AppResult<Vec<CostItem>> {
    let mut st = conn.prepare(
        "SELECT code, name, sort_order FROM cost_item WHERE is_active = 1 ORDER BY sort_order",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(CostItem {
                code: r.get(0)?,
                name: r.get(1)?,
                sort_order: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `YYYY-MM-DD` 형식인지 확인한다. 화면에서 막지만 IPC로도 한 번 더 본다.
pub fn check_date(label: &str, value: &str) -> AppResult<()> {
    if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
        return Err(AppError::invalid(format!(
            "{label}의 날짜 형식이 올바르지 않습니다. (예: 2026-03-01)"
        )));
    }
    Ok(())
}

/// 빈 문자열을 허용하지 않는 입력값을 다듬는다.
pub fn required_text(label: &str, value: &str) -> AppResult<String> {
    let t = value.trim();
    if t.is_empty() {
        return Err(AppError::invalid(format!("{label}을(를) 입력해 주세요.")));
    }
    Ok(t.to_string())
}

#[cfg(test)]
mod flow_tests;

#[cfg(test)]
mod enrollment_tests;

#[cfg(test)]
mod settle_tests;

#[cfg(test)]
mod proposal_tests;
