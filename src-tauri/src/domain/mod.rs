//! 업무 규칙. **DB에 의존하지 않는 순수 함수만** 둔다.
//!
//! 여기 있는 코드는 `rusqlite`를 import하지 않으며, 입력도 출력도 평범한 구조체다.
//! 그래야 정산 계산을 DB 없이 테스트할 수 있다 (설계안 1장).

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
