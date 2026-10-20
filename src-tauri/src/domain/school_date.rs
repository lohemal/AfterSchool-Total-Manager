//! 학교 운영 날짜 셈 (v0.1.8).
//!
//! 수업일은 **시각이 아니라 날짜**다. `2026-10-06` 은 그날 하루를 가리키는
//! 것이지 어느 시점이 아니다. 그래서 어디에서도 UTC 로 바꾸지 않는다 —
//! 한 번만 시간대를 거쳐도 날짜가 하루 밀린다.
//!
//! 날짜 셈은 **여기 한 곳에만** 둔다. 화면과 Excel 이 각자 계산하면 같은
//! 부서의 수업일이 두 곳에서 달라진다.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::error::{AppError, AppResult};

/// `YYYY-MM-DD` 를 날짜로 읽는다. 시간대를 거치지 않는다.
pub fn parse(date: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::invalid(format!("날짜 형식이 올바르지 않습니다: {date}")))
}

/// 오늘(로컬). 전출 판정의 기준이다.
pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn fmt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// 요일 한 글자. `domain::DAY_ORDER` 와 같은 표기를 쓴다.
pub fn weekday_label(d: NaiveDate) -> &'static str {
    match d.weekday() {
        Weekday::Mon => "월",
        Weekday::Tue => "화",
        Weekday::Wed => "수",
        Weekday::Thu => "목",
        Weekday::Fri => "금",
        Weekday::Sat => "토",
        Weekday::Sun => "일",
    }
}

/// 기간 안에서 주어진 요일에 해당하는 날짜를 모두 센다.
///
/// 요일 목록은 `domain::parse_days` 가 읽어 준 것을 그대로 받는다 — 요일을
/// 해석하는 규칙이 화면마다 달라지지 않게 하기 위해서다.
///
/// 시작일이 끝일보다 뒤이거나 요일 목록이 비면 빈 결과다. '몇 주 × 몇 회'로
/// 어림하지 않고 **달력을 실제로 훑는다** — 그래야 달 경계와 5주짜리 달이
/// 맞는다.
pub fn dates_on_weekdays(start: NaiveDate, end: NaiveDate, days: &[&str]) -> Vec<NaiveDate> {
    if days.is_empty() || start > end {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut d = start;
    while d <= end {
        if days.contains(&weekday_label(d)) {
            out.push(d);
        }
        d += Duration::days(1);
    }
    out
}

/// 학생의 재학 상태. **저장하지 않고 전출일과 오늘로 판정한다.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enrolment {
    /// 전출일이 없다
    Attending,
    /// 전출일이 아직 오지 않았다 — 그날까지는 재학생이다
    Leaving,
    /// 전출일이 오늘이거나 지났다
    Left,
}

/// 전출일과 기준일로 재학 상태를 가른다.
///
/// 기준일을 받는 까닭은 시험 때문이다. 화면은 늘 `today()` 를 넘긴다.
///
/// ```text
/// 전출일 없음            → 재학
/// 전출일 > 기준일        → 전출 예정
/// 전출일 <= 기준일       → 전출
/// ```
pub fn enrolment_on(transfer_out_date: Option<&str>, on: NaiveDate) -> Enrolment {
    match transfer_out_date.map(str::trim).filter(|s| !s.is_empty()) {
        None => Enrolment::Attending,
        Some(raw) => match parse(raw) {
            // 읽을 수 없는 값은 재학으로 둔다 — 알 수 없는 글자 때문에 학생이
            // 조용히 명단에서 사라지는 쪽이 더 나쁘다.
            Err(_) => Enrolment::Attending,
            Ok(d) if d > on => Enrolment::Leaving,
            Ok(_) => Enrolment::Left,
        },
    }
}

pub fn enrolment_label(e: Enrolment) -> &'static str {
    match e {
        Enrolment::Attending => "재학",
        Enrolment::Leaving => "전출 예정",
        Enrolment::Left => "전출",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse(s).unwrap()
    }

    #[test]
    fn 전출일이_없으면_재학이다() {
        assert_eq!(enrolment_on(None, d("2026-10-07")), Enrolment::Attending);
        assert_eq!(enrolment_on(Some(""), d("2026-10-07")), Enrolment::Attending);
    }

    #[test]
    fn 전출일이_미래면_전출_예정이다() {
        assert_eq!(
            enrolment_on(Some("2026-10-15"), d("2026-10-07")),
            Enrolment::Leaving
        );
    }

    #[test]
    fn 전출일이_오늘이면_전출이다() {
        // 경계: 전출일 당일은 이미 떠난 것으로 본다
        assert_eq!(
            enrolment_on(Some("2026-10-15"), d("2026-10-15")),
            Enrolment::Left
        );
        assert_eq!(
            enrolment_on(Some("2026-10-15"), d("2026-10-20")),
            Enrolment::Left
        );
    }

    #[test]
    fn 읽을_수_없는_날짜는_재학으로_둔다() {
        assert_eq!(enrolment_on(Some("몰라"), d("2026-10-07")), Enrolment::Attending);
    }

    #[test]
    fn 기간_안의_요일_날짜를_달력대로_센다() {
        // 2026-10-01 은 목요일. 10월의 화요일은 6·13·20·27 넷이다.
        let got = dates_on_weekdays(d("2026-10-01"), d("2026-10-31"), &["화"]);
        assert_eq!(
            got.iter().map(|x| fmt(*x)).collect::<Vec<_>>(),
            vec!["2026-10-06", "2026-10-13", "2026-10-20", "2026-10-27"]
        );
    }

    #[test]
    fn 여러_요일은_모두_센다() {
        let got = dates_on_weekdays(d("2026-10-01"), d("2026-10-14"), &["월", "수"]);
        assert_eq!(
            got.iter().map(|x| fmt(*x)).collect::<Vec<_>>(),
            vec!["2026-10-05", "2026-10-07", "2026-10-12", "2026-10-14"]
        );
    }

    #[test]
    fn 끝일과_시작일이_뒤집히거나_요일이_없으면_빈_결과다() {
        assert!(dates_on_weekdays(d("2026-10-31"), d("2026-10-01"), &["화"]).is_empty());
        assert!(dates_on_weekdays(d("2026-10-01"), d("2026-10-31"), &[]).is_empty());
    }

    #[test]
    fn 양_끝날도_센다() {
        // 10-06 은 화요일이고 기간의 첫날이자 끝날이다
        let got = dates_on_weekdays(d("2026-10-06"), d("2026-10-06"), &["화"]);
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn 요일_이름이_달력과_맞는다() {
        assert_eq!(weekday_label(d("2026-10-05")), "월");
        assert_eq!(weekday_label(d("2026-10-11")), "일");
    }
}
