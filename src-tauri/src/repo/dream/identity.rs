//! 학생 개인참조번호 · 생년월일.
//!
//! DB 에는 AES-256-GCM 암호문 · nonce 와, 개인참조번호를 정확히 찾기 위한 HMAC 값만
//! 있다. 평문은 함수 안에서만 잠깐 쓰고, 돌려줄 때는 `Zeroizing` 으로 감싸 다 쓰면
//! 지워지게 한다. 오류 문장에는 값을 넣지 않는다.
//!
//! 같은 학년도에서 같은 개인참조번호는 한 학생에게만 줄 수 있다 — 찾기용 값으로 막는다.

use rusqlite::{params, Connection, OptionalExtension};
use zeroize::Zeroizing;

use crate::crypto::{self, DataKey, Sealed};
use crate::domain::dream;
use crate::error::{AppError, AppResult};

#[derive(Clone, Copy)]
enum Field {
    Ref,
    Birth,
}

impl Field {
    fn tag(self) -> &'static str {
        match self {
            Field::Ref => "ref",
            Field::Birth => "birth",
        }
    }
}

/// 암호문을 이 학생 · 이 칸 · 이 키에 묶는다. 다른 줄 · 다른 칸으로 옮기면 열리지 않는다.
fn aad(field: Field, student_id: i64, key_id: &str) -> Vec<u8> {
    format!("afterschool/dream/{}/v1|student={student_id}|key={key_id}", field.tag()).into_bytes()
}

/// 등록 여부만 — 복호화하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityState {
    pub has_ref: bool,
    pub has_birth: bool,
}

struct Row {
    key_id: String,
    ref_sealed: Option<Sealed>,
    birth_sealed: Option<Sealed>,
}

fn load(conn: &Connection, student_id: i64) -> AppResult<Option<Row>> {
    Ok(conn
        .query_row(
            "SELECT key_id, ref_nonce, ref_cipher, birth_nonce, birth_cipher
               FROM dream_student_identity WHERE student_id = ?1",
            params![student_id],
            |r| {
                let sealed = |n: Option<Vec<u8>>, c: Option<Vec<u8>>| match (n, c) {
                    (Some(nonce), Some(ciphertext)) => Some(Sealed { nonce, ciphertext }),
                    _ => None,
                };
                Ok(Row {
                    key_id: r.get(0)?,
                    ref_sealed: sealed(r.get(1)?, r.get(2)?),
                    birth_sealed: sealed(r.get(3)?, r.get(4)?),
                })
            },
        )
        .optional()?)
}

fn year_of(conn: &Connection, student_id: i64) -> AppResult<i64> {
    conn.query_row(
        "SELECT year_id FROM student WHERE id = ?1",
        params![student_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("학생을 찾지 못했습니다."))
}

/// 이 줄의 키가 지금 키와 같은지. 키를 바꾸는 기능은 아직 없으므로 다르면 멈춘다.
fn same_key(row: &Row, key: &DataKey) -> AppResult<()> {
    if row.key_id == key.key_id {
        Ok(())
    } else {
        Err(AppError::new(
            "KEY_MISMATCH",
            "이 학생의 개인정보는 다른 암호 키로 저장되어 있어 고칠 수 없습니다.",
        ))
    }
}

pub fn state(conn: &Connection, student_id: i64) -> AppResult<IdentityState> {
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT ref_cipher IS NOT NULL, birth_cipher IS NOT NULL
               FROM dream_student_identity WHERE student_id = ?1",
            params![student_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (r, b) = row.unwrap_or((0, 0));
    Ok(IdentityState {
        has_ref: r == 1,
        has_birth: b == 1,
    })
}

/// 개인참조번호를 등록하거나 바꾼다. 같은 학년도의 다른 학생이 쓰는 번호면 거절한다.
pub fn set_ref(conn: &Connection, key: &DataKey, student_id: i64, raw: &str) -> AppResult<()> {
    let value = Zeroizing::new(dream::check_ref(raw).map_err(AppError::invalid)?);
    let year_id = year_of(conn, student_id)?;
    let lookup = crypto::lookup(key, &value);

    let holder: Option<i64> = conn
        .query_row(
            "SELECT student_id FROM dream_student_identity WHERE year_id = ?1 AND ref_lookup = ?2",
            params![year_id, lookup],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(other) = holder {
        if other != student_id {
            return Err(AppError::new(
                "CONFLICT",
                "같은 학년도의 다른 학생에게 이미 등록된 개인참조번호입니다.",
            ));
        }
    }

    let sealed = crypto::seal(key, &aad(Field::Ref, student_id, &key.key_id), value.as_bytes())?;
    match load(conn, student_id)? {
        Some(row) => {
            same_key(&row, key)?;
            conn.execute(
                "UPDATE dream_student_identity
                    SET ref_nonce = ?2, ref_cipher = ?3, ref_lookup = ?4,
                        updated_at = datetime('now', 'localtime')
                  WHERE student_id = ?1",
                params![student_id, sealed.nonce, sealed.ciphertext, lookup],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO dream_student_identity
                   (student_id, year_id, key_id, ref_nonce, ref_cipher, ref_lookup)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![student_id, year_id, key.key_id, sealed.nonce, sealed.ciphertext, lookup],
            )?;
        }
    }
    Ok(())
}

/// 생년월일을 등록하거나 바꾼다(`YYMMDD`).
pub fn set_birth(conn: &Connection, key: &DataKey, student_id: i64, raw: &str) -> AppResult<()> {
    let value = Zeroizing::new(dream::check_birth(raw).map_err(AppError::invalid)?);
    let year_id = year_of(conn, student_id)?;
    let sealed = crypto::seal(key, &aad(Field::Birth, student_id, &key.key_id), value.as_bytes())?;
    match load(conn, student_id)? {
        Some(row) => {
            same_key(&row, key)?;
            conn.execute(
                "UPDATE dream_student_identity
                    SET birth_nonce = ?2, birth_cipher = ?3, updated_at = datetime('now', 'localtime')
                  WHERE student_id = ?1",
                params![student_id, sealed.nonce, sealed.ciphertext],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO dream_student_identity (student_id, year_id, key_id, birth_nonce, birth_cipher)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![student_id, year_id, key.key_id, sealed.nonce, sealed.ciphertext],
            )?;
        }
    }
    Ok(())
}

/// 한 칸을 지운다. 둘 다 비면 줄을 지운다(빈 줄을 남기지 않는다).
fn clear(conn: &Connection, student_id: i64, field: Field) -> AppResult<()> {
    let sql = match field {
        Field::Ref => {
            "UPDATE dream_student_identity SET ref_nonce = NULL, ref_cipher = NULL, ref_lookup = NULL,
                    updated_at = datetime('now', 'localtime')
              WHERE student_id = ?1 AND birth_cipher IS NOT NULL"
        }
        Field::Birth => {
            "UPDATE dream_student_identity SET birth_nonce = NULL, birth_cipher = NULL,
                    updated_at = datetime('now', 'localtime')
              WHERE student_id = ?1 AND ref_cipher IS NOT NULL"
        }
    };
    if conn.execute(sql, params![student_id])? == 0 {
        // 다른 칸이 비어 있었다 — 줄째 지운다
        let other_empty = match field {
            Field::Ref => "birth_cipher IS NULL",
            Field::Birth => "ref_cipher IS NULL",
        };
        conn.execute(
            &format!("DELETE FROM dream_student_identity WHERE student_id = ?1 AND {other_empty}"),
            params![student_id],
        )?;
    }
    Ok(())
}

pub fn clear_ref(conn: &Connection, student_id: i64) -> AppResult<()> {
    clear(conn, student_id, Field::Ref)
}

pub fn clear_birth(conn: &Connection, student_id: i64) -> AppResult<()> {
    clear(conn, student_id, Field::Birth)
}

/// 이 학생의 방과후드림 개인정보를 모두 지운다.
pub fn delete(conn: &Connection, student_id: i64) -> AppResult<()> {
    conn.execute(
        "DELETE FROM dream_student_identity WHERE student_id = ?1",
        params![student_id],
    )?;
    Ok(())
}

/// 파일에서 받은 개인참조번호로 학생을 정확히 찾는다. 모든 줄을 풀어 보지 않는다.
///
/// 찾은 학생이 그 학년도의 학생인지는 표의 열쇠(학년도, 찾기용 값)가 보증한다.
pub fn find_by_ref(conn: &Connection, key: &DataKey, year_id: i64, raw: &str) -> AppResult<Option<i64>> {
    let value = Zeroizing::new(dream::check_ref(raw).map_err(AppError::invalid)?);
    let lookup = crypto::lookup(key, &value);
    Ok(conn
        .query_row(
            "SELECT student_id FROM dream_student_identity WHERE year_id = ?1 AND ref_lookup = ?2",
            params![year_id, lookup],
            |r| r.get(0),
        )
        .optional()?)
}

fn reveal(conn: &Connection, key: &DataKey, student_id: i64, field: Field) -> AppResult<Option<Zeroizing<String>>> {
    let Some(row) = load(conn, student_id)? else {
        return Ok(None);
    };
    let sealed = match field {
        Field::Ref => row.ref_sealed.as_ref(),
        Field::Birth => row.birth_sealed.as_ref(),
    };
    let Some(sealed) = sealed else {
        return Ok(None);
    };
    same_key(&row, key)?;
    let plain = crypto::open(key, &aad(field, student_id, &row.key_id), sealed)?;
    let text = String::from_utf8(plain.to_vec())
        .map_err(|_| AppError::new("CRYPTO_TAMPERED", "암호화된 개인정보가 손상되었습니다."))?;
    Ok(Some(Zeroizing::new(text)))
}

/// 필요할 때만 푼다. 돌려준 값은 다 쓰면 지워진다.
pub fn reveal_ref(conn: &Connection, key: &DataKey, student_id: i64) -> AppResult<Option<Zeroizing<String>>> {
    reveal(conn, key, student_id, Field::Ref)
}

pub fn reveal_birth(conn: &Connection, key: &DataKey, student_id: i64) -> AppResult<Option<Zeroizing<String>>> {
    reveal(conn, key, student_id, Field::Birth)
}
