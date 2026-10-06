//! 업무 규칙. **DB에 의존하지 않는 순수 함수만** 둔다.
//!
//! 여기 있는 코드는 `rusqlite`를 import하지 않으며, 입력도 출력도 평범한 구조체다.
//! 그래야 정산 계산을 DB 없이 테스트할 수 있다 (설계안 1장).

pub mod class_no;
pub mod settle;
pub mod support;

use serde::{Deserialize, Serialize};

/// 지원제도.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Program {
    /// 방과후 이용권
    Voucher,
    /// 자유수강권
    FreeVoucher,
}

impl Program {
    pub fn code(self) -> &'static str {
        match self {
            Program::Voucher => "VOUCHER",
            Program::FreeVoucher => "FREE_VOUCHER",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Program::Voucher => "방과후 이용권",
            Program::FreeVoucher => "자유수강권",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "VOUCHER" => Some(Program::Voucher),
            "FREE_VOUCHER" => Some(Program::FreeVoucher),
            _ => None,
        }
    }

    /// 이 제도로 지원한 금액이 들어가는 재원.
    pub fn fund(self) -> Fund {
        match self {
            Program::Voucher => Fund::Voucher,
            Program::FreeVoucher => Fund::FreeVoucher,
        }
    }

    pub const ALL: [Program; 2] = [Program::Voucher, Program::FreeVoucher];
}

/// 정산 재원. `3학년` 같은 학년 개념은 여기에 절대 들어오지 않는다 (설계안 3장).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Fund {
    SelfPay,
    Voucher,
    VoucherOver,
    FreeVoucher,
}

impl Fund {
    pub fn code(self) -> &'static str {
        match self {
            Fund::SelfPay => "SELF_PAY",
            Fund::Voucher => "VOUCHER",
            Fund::VoucherOver => "VOUCHER_OVER",
            Fund::FreeVoucher => "FREE_VOUCHER",
        }
    }
}

/// 학부모 부담이 발생한 사연.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Origin {
    Plain,
    VoucherExhausted,
    FreeExhausted,
}

impl Origin {
    pub fn code(self) -> &'static str {
        match self {
            Origin::Plain => "PLAIN",
            Origin::VoucherExhausted => "VOUCHER_EXHAUSTED",
            Origin::FreeExhausted => "FREE_EXHAUSTED",
        }
    }
}

/// `'3,4'` 형태의 대상학년 문자열을 파싱한다. 빈 값이면 전 학년(=제한 없음).
pub fn parse_grades(text: &str) -> Vec<i64> {
    let mut out: Vec<i64> = text
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .filter(|g| (1..=9).contains(g))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// 대상학년 조건을 만족하는가. 목록이 비어 있으면 전 학년이 대상이다.
pub fn grade_matches(target: &[i64], grade: i64) -> bool {
    target.is_empty() || target.contains(&grade)
}

/// 두 기간이 하루라도 겹치는가. 날짜는 `YYYY-MM-DD`라 문자열 비교로 충분하다.
pub fn ranges_overlap(a_from: &str, a_to: &str, b_from: &str, b_to: &str) -> bool {
    a_from <= b_to && b_from <= a_to
}

/// 지원자격이 그 작업공간에서 유효한가 (설계안 0-2).
/// `valid_from`/`valid_to`가 없으면 학년도 내내 유효하다.
pub fn eligibility_active(
    valid_from: Option<&str>,
    valid_to: Option<&str>,
    ws_start: &str,
    ws_end: &str,
) -> bool {
    let from_ok = valid_from.map(|f| f <= ws_end).unwrap_or(true);
    let to_ok = valid_to.map(|t| t >= ws_start).unwrap_or(true);
    from_ok && to_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 대상학년_파싱() {
        assert_eq!(parse_grades("3"), vec![3]);
        assert_eq!(parse_grades("3,4"), vec![3, 4]);
        assert_eq!(parse_grades(" 4 , 3 ,3"), vec![3, 4]);
        assert_eq!(parse_grades(""), Vec::<i64>::new());
        assert_eq!(parse_grades("abc"), Vec::<i64>::new());
    }

    #[test]
    fn 빈_대상학년은_전학년() {
        assert!(grade_matches(&[], 1));
        assert!(grade_matches(&[3, 4], 4));
        assert!(!grade_matches(&[3, 4], 5));
    }

    #[test]
    fn 자격기간_판정() {
        // 기간이 없으면 언제나 유효
        assert!(eligibility_active(None, None, "2026-04-01", "2026-04-30"));
        // 4월 중에 지원이 끊긴 학생 — 4월 작업공간에서는 아직 유효
        assert!(eligibility_active(None, Some("2026-04-15"), "2026-04-01", "2026-04-30"));
        // 5월 작업공간에서는 유효하지 않음
        assert!(!eligibility_active(None, Some("2026-04-15"), "2026-05-01", "2026-05-31"));
        // 5월 전입생은 4월 작업공간에 없음
        assert!(!eligibility_active(Some("2026-05-01"), None, "2026-04-01", "2026-04-30"));
    }
}

/// 업무상 요일 차례. 가나다순이 아니다.
pub const DAY_ORDER: [&str; 7] = ["월", "화", "수", "목", "금", "토", "일"];

/// 요일을 읽을 수 없는 반을 모아 두는 이름.
pub const DAY_UNKNOWN: &str = "미지정";

/// 요일 칸의 글을 요일 목록으로 읽는다.
///
/// `department.days` 는 자유 입력이다(`월,수`). 쉼표뿐 아니라 가운뎃점·빗금·
/// 빈칸으로 적는 사람도 있고 `월요일` 처럼 길게 적는 사람도 있어서, 한글이
/// 아닌 글자를 모두 구분자로 보고 토막마다 **첫 글자**만 본다. 요일이 아닌
/// 토막은 버린다. 차례는 월→일로 맞추고 중복은 지운다.
///
/// **같은 규칙이 두 곳에 있다** — 여기와 화면쪽 `src/lib/format.ts` 의
/// `parseDays`. 하나만 고치면 화면과 Excel 의 차례가 달라진다.
pub fn parse_days(text: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for token in text.split(|c: char| !('가'..='힣').contains(&c)) {
        let Some(first) = token.chars().next() else {
            continue;
        };
        let s = first.to_string();
        if let Some(day) = DAY_ORDER.iter().find(|d| **d == s) {
            if !out.contains(day) {
                out.push(day);
            }
        }
    }
    out.sort_by_key(|d| DAY_ORDER.iter().position(|x| x == d).unwrap_or(99));
    out
}

/// 수강 가능 학년을 사람이 읽는 글로. **빈 목록은 '미설정'** 이다.
///
/// `support_policy` 쪽 `grade_text` 와 헷갈리지 말 것 — 그쪽은 빈 목록이
/// '전 학년'이다. 뜻이 반대라서 함수를 따로 둔다.
pub fn allowed_grade_text(grades: &[i64]) -> String {
    if grades.is_empty() {
        return "미설정".to_string();
    }
    if grades.len() == 6 && (1..=6).all(|g| grades.contains(&g)) {
        return "전 학년".to_string();
    }
    let mut g = grades.to_vec();
    g.sort_unstable();
    g.dedup();
    format!(
        "{}학년",
        g.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("·")
    )
}

/// 수강 가능 학년 목록을 다듬는다. 1~6 밖은 버리고, 차례를 맞추고 중복을 지운다.
pub fn clean_grades(grades: &[i64]) -> Vec<i64> {
    let mut g: Vec<i64> = grades.iter().copied().filter(|x| (1..=6).contains(x)).collect();
    g.sort_unstable();
    g.dedup();
    g
}
