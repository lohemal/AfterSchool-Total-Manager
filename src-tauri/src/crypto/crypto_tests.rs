//! 암호 계층 시험 — 값은 모두 가짜다.

use super::*;

fn key(b: u8) -> DataKey {
    DataKey::new(format!("k{b}"), [b; KEY_LEN])
}

#[test]
fn 봉인하고_열면_원래_값이다() {
    let k = key(7);
    let s = seal(&k, b"aad", b"I100000987").unwrap();
    assert_eq!(s.nonce.len(), NONCE_LEN);
    assert_ne!(s.ciphertext, b"I100000987".to_vec(), "평문이 그대로 들어갔다");
    assert_eq!(open(&k, b"aad", &s).unwrap().as_slice(), b"I100000987");
}

#[test]
fn 같은_평문도_nonce_가_달라_암호문이_다르다() {
    let k = key(7);
    let mut nonces = std::collections::HashSet::new();
    let mut ciphers = std::collections::HashSet::new();
    for _ in 0..200 {
        let s = seal(&k, b"aad", b"same").unwrap();
        assert!(nonces.insert(s.nonce.clone()), "nonce 가 되풀이됐다");
        ciphers.insert(s.ciphertext.clone());
    }
    assert_eq!(ciphers.len(), 200);
}

#[test]
fn 암호문_태그_nonce_aad_가_바뀌면_열리지_않는다() {
    let k = key(7);
    let s = seal(&k, b"student=1", b"170806").unwrap();

    let mut bad = s.clone();
    bad.ciphertext[0] ^= 1;
    assert_eq!(open(&k, b"student=1", &bad).unwrap_err().code, "CRYPTO_TAMPERED");

    let mut tag = s.clone();
    let last = tag.ciphertext.len() - 1;
    tag.ciphertext[last] ^= 0x80; // 인증 태그 끝
    assert_eq!(open(&k, b"student=1", &tag).unwrap_err().code, "CRYPTO_TAMPERED");

    let mut nonce = s.clone();
    nonce.nonce[0] ^= 1;
    assert!(open(&k, b"student=1", &nonce).is_err());

    assert!(open(&k, b"student=2", &s).is_err(), "다른 학생 줄로 옮긴 암호문이 열렸다");

    let short = Sealed {
        nonce: vec![0; 5],
        ciphertext: s.ciphertext.clone(),
    };
    assert!(open(&k, b"student=1", &short).is_err());
}

#[test]
fn 다른_키로는_열리지_않는다() {
    let s = seal(&key(7), b"aad", b"170806").unwrap();
    assert_eq!(open(&key(8), b"aad", &s).unwrap_err().code, "CRYPTO_TAMPERED");
}

#[test]
fn 찾기용_값은_같은_키_같은_번호면_같고_다르면_다르다() {
    let k = key(7);
    let a = lookup(&k, "I100000987");
    assert_eq!(a.len(), 64);
    assert_eq!(a, lookup(&k, "I100000987"), "결정적이어야 찾을 수 있다");
    assert_ne!(a, lookup(&k, "I100000988"));
    assert_ne!(a, lookup(&key(8), "I100000987"), "키가 다르면 값도 달라야 한다");
    // 그냥 SHA-256 이 아니다 — 키 없이 맞춰 볼 수 없다
    assert_ne!(a, sha256_hex(b"I100000987"));
}

#[test]
fn 오류와_디버그_출력에_키_평문이_나오지_않는다() {
    let k = key(0x41); // 'A' 32개
    let dbg = format!("{k:?}");
    assert!(!dbg.contains("AAAA"), "{dbg}");
    let s = seal(&k, b"", b"I100000987").unwrap();
    assert!(!format!("{s:?}").contains("I100000987"));
    let e = open(&key(9), b"", &s).unwrap_err();
    let all = format!("{} {:?}", e, e.detail);
    assert!(!all.contains("I100000987"));
}

#[test]
fn dpapi_로_감싸고_풀면_같은_키다() {
    let secret = [0x5au8; KEY_LEN];
    let blob = dpapi::protect(&secret).unwrap();
    assert!(!blob.windows(KEY_LEN).any(|w| w == secret), "감싼 사본에 키가 그대로 들어 있다");
    assert_eq!(dpapi::unprotect(&blob).unwrap().as_slice(), &secret);
    let mut broken = blob.clone();
    let mid = broken.len() / 2;
    broken[mid] ^= 0xff;
    assert_eq!(dpapi::unprotect(&broken).unwrap_err().code, "KEY_UNAVAILABLE");
}
