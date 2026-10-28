//! `dream_key` — DPAPI 로 감싼 데이터 키. 키 평문은 DB 에 없다.
//!
//! * 처음 개인정보를 저장할 때 키를 하나 만든다(그전에는 표가 비어 있다).
//! * 쓰는 키는 가장 최근 것 하나. 암호문마다 어느 키로 봉인했는지(`key_id`)를 적는다.
//!
//! 키를 DB 안에 두는 까닭: 백업은 DB 전체를 SQLite 백업 API 로 복사하고, 복원은 DB 를
//! 통째로 갈아 끼운다. 키가 DB 밖 파일에 있으면 옛 백업을 되살렸을 때 암호문과 키가
//! 어긋날 수 있다. DB 안에 있으면 늘 짝이 맞고, 그래도 DPAPI 사본이라 다른 PC ·
//! 다른 Windows 계정에서는 풀리지 않는다.

use rusqlite::{params, Connection, OptionalExtension};

use super::{dpapi, DataKey, KEY_LEN};
use crate::error::{AppError, AppResult};

pub const ALGORITHM: &str = "AES-256-GCM";

fn unwrap(key_id: String, blob: &[u8]) -> AppResult<DataKey> {
    let plain = dpapi::unprotect(blob)?;
    if plain.len() != KEY_LEN {
        return Err(AppError::new("KEY_UNAVAILABLE", "방과후드림 개인정보 암호 키가 손상되었습니다.").detail("length"));
    }
    let mut b = [0u8; KEY_LEN];
    b.copy_from_slice(&plain);
    Ok(DataKey::new(key_id, b))
}

/// 이 키 번호의 키를 푼다.
pub fn get(conn: &Connection, key_id: &str) -> AppResult<DataKey> {
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT dpapi_blob FROM dream_key WHERE key_id = ?1",
            [key_id],
            |r| r.get(0),
        )
        .optional()?;
    let blob = blob.ok_or_else(|| {
        AppError::new("KEY_UNAVAILABLE", "방과후드림 개인정보 암호 키를 찾을 수 없습니다.").detail("missing key_id")
    })?;
    unwrap(key_id.to_string(), &blob)
}

/// 지금 쓰는 키(만들지 않는다). 아직 없으면 None — 저장된 개인정보도 없다는 뜻이다.
pub fn current(conn: &Connection) -> AppResult<Option<DataKey>> {
    let row: Option<(String, Vec<u8>)> = conn
        .query_row(
            "SELECT key_id, dpapi_blob FROM dream_key ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(id, blob)| unwrap(id, &blob)).transpose()
}

/// 지금 쓰는 키. 없으면 만든다(쓰기 트랜잭션 안에서 부른다).
pub fn current_or_create(conn: &Connection) -> AppResult<DataKey> {
    if let Some(k) = current(conn)? {
        return Ok(k);
    }
    let key = DataKey::generate(uuid::Uuid::new_v4().to_string());
    let blob = dpapi::protect(key.expose())?;
    conn.execute(
        "INSERT INTO dream_key (key_id, algorithm, dpapi_blob) VALUES (?1, ?2, ?3)",
        params![key.key_id, ALGORITHM, blob],
    )?;
    Ok(key)
}
