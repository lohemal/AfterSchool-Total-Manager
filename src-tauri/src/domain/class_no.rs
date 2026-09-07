//! 반 이름 규칙 (최종 QA에서 나온 요구사항).
//!
//! 학교마다 반 이름이 다르다. `1 2 3` 을 쓰는 곳도 있고 `가 나 다`,
//! `해 달 별` 을 쓰는 곳도 있다. 그래서 반은 **문자**다.
//!
//! ## 정렬
//!
//! 문자로 바꾸면 그냥 사전순으로 놓았을 때 `1, 10, 2, 3` 이 되어, 숫자 반을
//! 쓰는 학교의 쓰임새가 나빠진다. 그래서 정렬용 열쇠를 따로 만든다.
//!
//! ```text
//! 숫자 반  '1'  → "0000001"     '10' → "0000010"
//! 문자 반  '가' → "1가"
//! ```
//!
//! 앞자리 `0`/`1` 덕분에 **숫자 반이 늘 문자 반보다 앞**에 오고, 숫자끼리는
//! 자연 정렬이 된다. 문자끼리는 유니코드 차례라 `가 나 다`, `달 별 해` 처럼
//! 한글 순서가 된다.
//!
//! 같은 규칙이 SQL 쪽에도 `student.class_sort` 라는 GENERATED 열로 들어 있다
//! (`migrations/004_class_no_text.sql`). 둘이 어긋나면 화면과 엑셀의 차례가
//! 달라지므로, 시험이 두 값을 맞대어 본다.

/// 반 이름으로 쓸 수 있는 가장 긴 길이.
pub const MAX_LEN: usize = 10;

/// 정렬용 열쇠를 만든다. 이 값 자체는 어디에도 보여 주지 않는다.
pub fn sort_key(class_no: &str) -> String {
    if !class_no.is_empty() && class_no.bytes().all(|b| b.is_ascii_digit()) {
        // 숫자 반 — 여섯 자리로 채워 자연 정렬이 되게 한다.
        let padded = format!("000000{class_no}");
        format!("0{}", &padded[padded.len() - 6..])
    } else {
        format!("1{class_no}")
    }
}

/// 사람이 적어 넣은 반 이름을 다듬는다.
///
/// 엑셀에서 온 `'1 '` 과 손으로 친 `'1'` 이 서로 다른 반이 되면 안 되므로
/// 앞뒤 공백을 떼어 낸다. 그 밖의 글자는 손대지 않는다 — 학교가 쓰는 이름을
/// 프로그램이 고쳐서는 안 된다.
pub fn normalize(raw: &str) -> String {
    raw.trim().to_string()
}

/// 쓸 수 있는 반 이름인지 본다. 안 되면 사람이 읽을 수 있는 까닭을 돌려준다.
pub fn check(raw: &str) -> Result<String, String> {
    let v = normalize(raw);
    if v.is_empty() {
        return Err("반을 입력해 주세요.".into());
    }
    if v.chars().count() > MAX_LEN {
        return Err(format!("반은 {MAX_LEN}글자까지 쓸 수 있습니다."));
    }
    Ok(v)
}

/// 두 반 이름의 차례를 견준다.
pub fn cmp(a: &str, b: &str) -> std::cmp::Ordering {
    // 같은 열쇠가 나오는 경우('1' 과 '01')까지 생각해 원래 값으로 한 번 더 가른다.
    sort_key(a).cmp(&sort_key(b)).then_with(|| a.cmp(b))
}

#[cfg(test)]
#[path = "class_no_tests.rs"]
mod tests;
