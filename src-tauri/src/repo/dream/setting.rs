//! 기관코드 — 학교 단위 하나, `app_setting` 의 `dream.org_code`.
//!
//! `app_setting` 에는 자료판 트리거가 없으므로 기관코드를 바꿔도 정산이 낡음이 되지
//! 않는다. 결제파일의 기관코드가 이것과 다르면 파일 전체를 거절한다(P2).

use rusqlite::Connection;

use crate::domain::dream;
use crate::error::{AppError, AppResult};
use crate::repo::setting;

pub const ORG_CODE_KEY: &str = "dream.org_code";

pub fn org_code(conn: &Connection) -> AppResult<Option<String>> {
    setting::get(conn, ORG_CODE_KEY)
}

/// 사람이 명시적으로 넣거나 바꾼다. 형식이 틀리면 저장하지 않는다.
pub fn set_org_code(conn: &Connection, raw: &str) -> AppResult<String> {
    let v = dream::check_org_code(raw).map_err(AppError::invalid)?;
    setting::set(conn, ORG_CODE_KEY, &v)?;
    Ok(v)
}
