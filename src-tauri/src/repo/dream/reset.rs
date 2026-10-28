//! 방과후드림 개인정보 키 상태와 초기화.
//!
//! 이 PC · 이 Windows 사용자에서 키를 풀 수 없으면(다른 PC 로 옮긴 DB 등) 저장된
//! 개인참조번호 · 생년월일은 되살릴 수 없다. 복구 비밀번호는 두지 않는다 — 초기화한 뒤
//! 새로 등록한다(사용자 결정).
//!
//! ## 초기화가 지우는 것과 남기는 것
//!
//! ```text
//! 지움   dream_student_identity 전부 · 풀 수 없는 dream_key 전부
//! 남김   학생 · 수강 · charge · 정산 · 기관코드 · 강좌 매핑 · 방과후드림 우선순위 ·
//!        결제 기준월 · 처리 이력
//! ```
//!
//! ## 반쪽 상태를 남기지 않는다
//!
//! 새 키를 만들고 DPAPI 로 감싸는 일(실패할 수 있는 일)을 **DB 를 건드리기 전에** 먼저
//! 한다. 그다음 한 트랜잭션 안에서 지우고 · 새 키를 넣고 · 넣은 키가 이 PC 에서 다시
//! 풀리는지까지 확인한다. 어느 단계든 실패하면 트랜잭션이 통째로 되돌아가 이전 상태 그대로다.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::crypto::{dpapi, keys, DataKey};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KeyStatus {
    /// 키가 있고 이 PC 에서 풀린다
    Ready,
    /// 아직 개인정보를 저장한 적이 없다(키 없음)
    NotInitialized,
    /// 키가 있으나 이 PC · 이 Windows 사용자에서 풀리지 않는다
    KeyUnavailable,
}

/// 키를 **모두** 풀어 본다. 하나라도 안 풀리면 KEY_UNAVAILABLE.
pub fn key_status(conn: &Connection) -> AppResult<KeyStatus> {
    let ids: Vec<String> = conn
        .prepare("SELECT key_id FROM dream_key ORDER BY id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if ids.is_empty() {
        return Ok(KeyStatus::NotInitialized);
    }
    for id in &ids {
        match keys::get(conn, id) {
            Ok(_) => {}
            Err(e) if e.code == "KEY_UNAVAILABLE" => return Ok(KeyStatus::KeyUnavailable),
            Err(e) => return Err(e),
        }
    }
    Ok(KeyStatus::Ready)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResetResult {
    /// 지운 학생 개인정보 줄 수
    pub removed_identities: usize,
    pub removed_keys: usize,
}

/// 개인정보 초기화. **KEY_UNAVAILABLE 일 때만** 한다 — 멀쩡한 키로 저장한 자료를
/// 실수로 지우지 않게. 쓰기 트랜잭션 안에서 부른다(`Db::write`).
pub fn reset_identity(conn: &Connection) -> AppResult<ResetResult> {
    if key_status(conn)? != KeyStatus::KeyUnavailable {
        return Err(AppError::invalid(
            "개인정보 암호 키를 이 PC 에서 쓸 수 있는 상태라 초기화하지 않습니다.",
        ));
    }

    // 1) 실패할 수 있는 일을 먼저 — 새 키와 DPAPI 사본
    let key = DataKey::generate(uuid::Uuid::new_v4().to_string());
    let blob = dpapi::protect(key.expose())?;

    // 2) 한 트랜잭션 안에서 지우고 넣는다
    let removed_identities = conn.execute("DELETE FROM dream_student_identity", [])?;
    let removed_keys = conn.execute("DELETE FROM dream_key", [])?;
    conn.execute(
        "INSERT INTO dream_key (key_id, algorithm, dpapi_blob) VALUES (?1, ?2, ?3)",
        params![key.key_id, keys::ALGORITHM, blob],
    )?;

    // 3) 넣은 키가 정말 이 PC 에서 풀리는지 — 안 되면 되돌린다
    let back = keys::get(conn, &key.key_id)?;
    if back.expose() != key.expose() {
        return Err(AppError::new("KEY_UNAVAILABLE", "새 암호 키를 확인하지 못했습니다."));
    }
    log::info!("방과후드림 개인정보 초기화: 개인정보 {removed_identities}건 · 키 {removed_keys}개 삭제, 새 키 생성");
    Ok(ResetResult {
        removed_identities,
        removed_keys,
    })
}
