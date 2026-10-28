//! Windows DPAPI — 데이터 키를 **현재 Windows 사용자 계정**에 묶어 감싼다.
//!
//! 같은 PC · 같은 계정에서는 비밀번호 없이 풀리고, 다른 PC · 다른 계정에서는 풀리지
//! 않는다. 이 앱 전용 엔트로피를 함께 넣어 같은 계정의 다른 프로그램(경력증명서
//! 앱 등)이 이 키를 그대로 풀 수 없게 한다.

use crate::error::AppError;

/// 이 앱 전용 추가 엔트로피 (비밀이 아니다 — 다른 프로그램과 섞이지 않게 하는 표시)
const ENTROPY: &[u8] = b"kr.school.afterschool/dream-datakey/v1";

fn dpapi_err(what: &str) -> AppError {
    AppError::new(
        "KEY_UNAVAILABLE",
        "이 컴퓨터에서 방과후드림 개인정보 암호 키를 풀 수 없습니다. 다른 PC나 다른 Windows 계정에서 옮겨 온 자료라면 개인참조번호와 생년월일을 다시 등록해야 합니다.",
    )
    .detail(what.to_string())
}

#[cfg(windows)]
pub fn protect(secret: &[u8]) -> Result<Vec<u8>, AppError> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| dpapi_err(&format!("protect: {e}")))?;
        let blob = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
        Ok(blob)
    }
}

#[cfg(windows)]
pub fn unprotect(blob: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, AppError> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| dpapi_err(&format!("unprotect: {e}")))?;
        let plain = std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize);
        let copy = zeroize::Zeroizing::new(plain.to_vec());
        zeroize::Zeroize::zeroize(plain); // DPAPI 가 돌려준 버퍼도 지우고 놓는다
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
        Ok(copy)
    }
}

#[cfg(not(windows))]
pub fn protect(_secret: &[u8]) -> Result<Vec<u8>, AppError> {
    Err(dpapi_err("Windows 에서만 동작"))
}

#[cfg(not(windows))]
pub fn unprotect(_blob: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, AppError> {
    Err(dpapi_err("Windows 에서만 동작"))
}
