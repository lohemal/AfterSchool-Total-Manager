//! 방과후드림 개인참조번호 · 생년월일 암호화.
//!
//! ```text
//!   데이터 키(32바이트 난수)
//!     └─ Windows DPAPI(현재 사용자)로 감싼 사본 ─▶ dream_key.dpapi_blob
//!
//!   개인참조번호 · 생년월일 ─ AES-256-GCM(데이터 키, 값마다 새 nonce 12바이트,
//!                             AAD = 항목 · 학생 id · 키 번호) ─▶ dream_student_identity
//!   개인참조번호 찾기용 값   ─ HMAC-SHA256(데이터 키에서 나눈 찾기 키, 번호) ─▶ ref_lookup
//! ```
//!
//! 경력증명서 앱(careercert)에서 검증한 구조를 그대로 따른다. 알고리즘을 새로
//! 만들지 않고 RustCrypto 의 `aes-gcm` · `hmac` · `sha2` 와 Windows DPAPI 만 쓴다.
//!
//! **찾기용 값을 그냥 SHA-256 으로 하지 않는 까닭**: 개인참조번호는 경우의 수가
//! 작아 DB 를 손에 넣은 사람이 하나씩 맞춰 볼 수 있다. 키를 모르면 맞춰 볼 수
//! 없게 HMAC 으로 만든다.
//!
//! **AAD 에 학생 id 를 넣는 까닭**: 암호문을 다른 학생 줄로 옮겨 붙이면 열리지
//! 않게 하려는 것이다. 개인참조번호 암호문을 생년월일 칸에 옮겨도 마찬가지다.
//!
//! 지키는 것: 키 · 평문을 로그 · 오류 · 파일에 남기지 않는다. 다 쓴 평문 버퍼는
//! 지운다(zeroize). 암호화가 실패하면 평문으로 대신 저장하지 않는다 — 오류다.

pub mod dpapi;
pub mod keys;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::AppError;

pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;

/// 풀린 데이터 키. 메모리에서 사라질 때 0 으로 지운다. `Debug` 는 값을 찍지 않는다.
#[derive(Clone)]
pub struct DataKey {
    pub key_id: String,
    bytes: Zeroizing<[u8; KEY_LEN]>,
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DataKey({}, ******)", self.key_id)
    }
}

impl DataKey {
    pub fn new(key_id: String, bytes: [u8; KEY_LEN]) -> Self {
        DataKey {
            key_id,
            bytes: Zeroizing::new(bytes),
        }
    }

    /// 새 난수 키
    pub fn generate(key_id: String) -> Self {
        let k = Aes256Gcm::generate_key(OsRng);
        let mut b = [0u8; KEY_LEN];
        b.copy_from_slice(&k);
        DataKey::new(key_id, b)
    }

    pub(crate) fn expose(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// 찾기 키 = HMAC(데이터 키, 이름표) — 암호화 키를 그대로 두 곳에 쓰지 않는다.
    fn lookup_key(&self) -> Zeroizing<Vec<u8>> {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&self.bytes[..])
            .expect("HMAC 키 길이는 무엇이든 된다");
        m.update(b"afterschool/dream/ref-lookup/v1");
        Zeroizing::new(m.finalize().into_bytes().to_vec())
    }
}

fn crypto_err(detail: &str) -> AppError {
    AppError::new("CRYPTO_FAILED", "암호화된 자료를 처리하지 못했습니다.").detail(detail.to_string())
}

/// 봉인한 결과.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed {
    pub nonce: Vec<u8>,
    /// 암호문 + 인증 태그
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sealed({} bytes)", self.ciphertext.len())
    }
}

/// AES-256-GCM 으로 봉인한다. nonce 는 매번 새 난수(96비트)라 같은 평문도 암호문이 다르다.
pub fn seal(key: &DataKey, aad: &[u8], plaintext: &[u8]) -> Result<Sealed, AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.expose()));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad })
        .map_err(|_| crypto_err("encrypt"))?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

/// 봉인을 연다. 암호문 · nonce · AAD · 키가 하나라도 다르면 실패한다(변조 감지).
pub fn open(key: &DataKey, aad: &[u8], sealed: &Sealed) -> Result<Zeroizing<Vec<u8>>, AppError> {
    if sealed.nonce.len() != NONCE_LEN {
        return Err(crypto_err("nonce length"));
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.expose()));
    cipher
        .decrypt(
            Nonce::from_slice(&sealed.nonce),
            Payload {
                msg: &sealed.ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| {
            AppError::new(
                "CRYPTO_TAMPERED",
                "암호화된 개인정보를 열 수 없습니다. 자료가 손상되었거나 다른 키로 만든 것입니다.",
            )
        })
}

/// 개인참조번호 찾기용 값 — HMAC-SHA256(찾기 키, 번호) 의 16진수 64자.
pub fn lookup(key: &DataKey, value: &str) -> String {
    let lk = key.lookup_key();
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&lk).expect("HMAC 키 길이는 무엇이든 된다");
    m.update(value.as_bytes());
    hex(&m.finalize().into_bytes())
}

/// 파일 지문 — 개인정보가 없는 비교용(원본 · 결과 파일).
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex(&Sha256::digest(data))
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod crypto_tests;
