//! 방과후드림 결제파일 규칙 — DB 를 모르는 순수 함수.
//!
//! 결제파일은 **학생 × 강좌 × 항목 = 한 줄**이고, 18열(A 회계년도 … R 이체예정일)이다.
//! O 개인부담금 · P 이용권 · Q 자유수강권.
//!
//! 여기 규칙은 모두 "추측보다 차단"이다. 값을 고쳐 맞추지 않고, 맞지 않으면 거절한다.
//! 앞뒤 공백만 떼어 낸다(Excel 에서 따라 들어온 공백 때문에 같은 번호가 다른 번호가
//! 되면 안 되므로). 그 밖의 글자는 바꾸지 않는다.

/// 결제파일 항목 — (방과후드림 항목구분코드, 항목구분, 비용항목 코드).
///
/// 코드는 숫자가 아니라 **글자**다. `01` 의 앞자리 0 을 지키고, `1` 은 `01` 이 아니다.
pub const ITEMS: [(&str, &str, &str); 4] = [
    ("01", "강사료", "INSTRUCTOR"),
    ("02", "수용비", "OPERATION"),
    ("03", "교재비", "TEXTBOOK"),
    ("04", "재료비", "MATERIAL"),
];

/// 결제파일 열 머리글. 차례가 곧 열(A~R)이다.
pub const HEADERS: [&str; 18] = [
    "회계년도",
    "기관코드",
    "개인참조번호",
    "학년",
    "반",
    "번호",
    "학생명",
    "생년월일",
    "수강월",
    "강좌명",
    "강좌명코드",
    "항목구분",
    "항목구분코드",
    "정산금액",
    "개인부담금",
    "이용권",
    "자유수강권",
    "이체예정일",
];

/// 처리 이력 보관 기간(일). 개인참조번호 · 강좌 매핑 · 우선순위 같은 설정은 지우지 않는다.
pub const JOB_RETENTION_DAYS: i64 = 30;

/// 항목구분과 항목구분코드가 **둘 다** 맞아야 비용항목 코드를 돌려준다.
/// 하나만 맞으면 None — 자동으로 고쳐 맞추지 않는다.
pub fn item_of(name: &str, code: &str) -> Option<&'static str> {
    let (name, code) = (name.trim(), code.trim());
    ITEMS
        .iter()
        .find(|(c, n, _)| *c == code && *n == name)
        .map(|(_, _, item)| *item)
}

/// 비용항목 코드 → 방과후드림 항목구분코드.
pub fn dream_code_of(item_code: &str) -> Option<&'static str> {
    ITEMS.iter().find(|(_, _, i)| *i == item_code).map(|(c, _, _)| *c)
}

fn is_ascii_alnum(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// 개인참조번호. 영문 · 숫자만, 1~32자. 대소문자를 바꾸지 않는다.
///
/// 오류 문장에 값을 넣지 않는다 — 화면 · 로그로 번호가 새지 않게.
pub fn check_ref(raw: &str) -> Result<String, &'static str> {
    let v = raw.trim();
    if v.is_empty() {
        return Err("개인참조번호를 입력해 주세요.");
    }
    if v.len() > 32 || !is_ascii_alnum(v) {
        return Err("개인참조번호는 영문과 숫자로 된 32자 이내여야 합니다.");
    }
    Ok(v.to_string())
}

/// 생년월일. 결제파일의 모양 그대로 `YYMMDD` 숫자 6자리만 받는다. 세기를 짐작하지 않는다.
pub fn check_birth(raw: &str) -> Result<String, &'static str> {
    let v = raw.trim();
    const MSG: &str = "생년월일은 YYMMDD 숫자 6자리여야 합니다.";
    if v.len() != 6 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MSG);
    }
    let mm: u32 = v[2..4].parse().map_err(|_| MSG)?;
    let dd: u32 = v[4..6].parse().map_err(|_| MSG)?;
    if !(1..=12).contains(&mm) || !(1..=31).contains(&dd) {
        return Err(MSG);
    }
    Ok(v.to_string())
}

/// 강좌명코드. 영문 · 숫자만, 1~64자(예: `20260818PGM000003317`).
pub fn check_course_code(raw: &str) -> Result<String, &'static str> {
    let v = raw.trim();
    if v.is_empty() {
        return Err("강좌명코드를 입력해 주세요.");
    }
    if v.len() > 64 || !is_ascii_alnum(v) {
        return Err("강좌명코드는 영문과 숫자로 된 64자 이내여야 합니다.");
    }
    Ok(v.to_string())
}

/// 기관코드. 숫자만, 1~20자(예: 10자리). 앞자리 0 을 지킨다.
pub fn check_org_code(raw: &str) -> Result<String, &'static str> {
    let v = raw.trim();
    if v.is_empty() {
        return Err("기관코드를 입력해 주세요.");
    }
    if v.len() > 20 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err("기관코드는 숫자로 된 20자 이내여야 합니다.");
    }
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 항목은_이름과_코드가_둘_다_맞아야_한다() {
        assert_eq!(item_of("강사료", "01"), Some("INSTRUCTOR"));
        assert_eq!(item_of(" 재료비 ", "04 "), Some("MATERIAL"));
        assert_eq!(item_of("강사료", "02"), None, "이름만 맞음");
        assert_eq!(item_of("수용비", "01"), None, "코드만 맞음");
        assert_eq!(item_of("강사료", "1"), None, "앞자리 0 이 빠지면 다른 코드다");
        assert_eq!(dream_code_of("TEXTBOOK"), Some("03"));
        assert_eq!(dream_code_of("UNKNOWN"), None);
    }

    #[test]
    fn 머리글은_18열이다() {
        assert_eq!(HEADERS.len(), 18);
        assert_eq!(HEADERS[14], "개인부담금"); // O
        assert_eq!(HEADERS[15], "이용권"); //     P
        assert_eq!(HEADERS[16], "자유수강권"); // Q
        assert_eq!(HEADERS[17], "이체예정일"); // R
    }

    #[test]
    fn 값은_공백만_떼고_고쳐_맞추지_않는다() {
        assert_eq!(check_ref(" A1B2 ").unwrap(), "A1B2");
        assert!(check_ref("").is_err());
        assert!(check_ref("A-1").is_err());
        assert!(check_ref(&"9".repeat(33)).is_err());
        assert_eq!(check_birth("170806").unwrap(), "170806");
        for bad in ["17086", "20170806", "171306", "170800", "17O806"] {
            assert!(check_birth(bad).is_err(), "{bad}");
        }
        assert_eq!(check_course_code("20260818PGM000003317").unwrap(), "20260818PGM000003317");
        assert!(check_course_code("2026 PGM").is_err());
        assert_eq!(check_org_code("0024000001").unwrap(), "0024000001");
        assert!(check_org_code("24-0001").is_err());
    }

    #[test]
    fn 오류_문장에는_값이_들어가지_않는다() {
        let secret = "Z9Y8X7W6V5U4T3S2R1Q0P9O8N7M6L5K4J3";
        let e = check_ref(secret).unwrap_err();
        assert!(!e.contains(secret));
    }
}
