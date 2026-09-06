//! 지원금 사용 가능액 계산 (설계안 5장).
//!
//! ```text
//! carry_in = 이월허용 ? max(0, Σ_{k<현재} 기간한도(k) − Σ_{k<현재} 사용액(k)) : 0
//!
//! 가용액 = max(0, min(
//!     기간한도(현재) + carry_in − 현재기간_이전작업공간_사용액,   -- 기간 몫
//!     연간한도 − 이전_총사용액                                   -- 연간 몫
//! ))
//! ```
//!
//! 두 몫 중 **작은 쪽**이 실제 가용액이다. "이월을 허용해도 연간 총액은 넘을 수
//! 없다"는 요구가 `min`의 두 번째 항 하나로 지켜진다.
//!
//! 지원기간이 하나도 없으면 `current_seq = None`으로 부르며,
//! 그때는 '학년도 전체가 하나의 기간'과 똑같이 동작한다.

use serde::Serialize;

/// 한 지원기간의 한도와 그 기간에 이미 쓴 금액.
#[derive(Debug, Clone, Copy)]
pub struct PeriodRow {
    pub seq: i64,
    /// 그 기간의 한도 (학생별 예외가 있으면 예외 금액)
    pub limit: i64,
    /// 그 기간에 속한 작업공간들의 최신 정산 사용액 합
    pub used: i64,
}

/// 화면(설계안 5-3)에 그대로 쓰는 계산 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    /// 연간 총 한도
    pub annual_limit: i64,
    /// 현재 지원기간의 한도 (기간이 없으면 연간 한도와 같다)
    pub period_limit: i64,
    /// 이전 기간에서 넘어온 이월액 (소멸 정책이면 0)
    pub carry_in: i64,
    /// 이전 지원기간들의 사용액 합
    pub used_prior_periods: i64,
    /// 현재 기간 안에서 앞선 작업공간들이 쓴 금액
    pub used_in_period_before: i64,
    /// 어느 지원기간에도 속하지 않는 앞선 작업공간의 사용액 (연간 몫에만 반영)
    pub used_outside_before: i64,
    /// 연간 누적 사용액 (이번 작업공간 제외)
    pub used_all_before: i64,
    /// 이번 작업공간에서 쓸 수 있는 금액
    pub available: i64,
    /// 연간 한도에 걸려 기간 몫보다 작아졌는가 (화면에 안내를 띄운다)
    pub capped_by_annual: bool,
}

/// 이번 작업공간의 사용 가능액을 구한다.
///
/// * `periods` — seq 오름차순. 현재 기간도 포함되어 있어야 한다.
/// * `current_seq` — 이번 작업공간이 속한 기간의 seq. `None`이면 지원기간 미설정.
/// * `used_in_period_before` — 현재 기간 안에서 앞선 작업공간들의 사용액 합.
///   (기간 미설정이면 그 학년도의 앞선 작업공간 전체 사용액)
/// * `used_outside_before` — 어느 지원기간에도 속하지 않는 앞선 작업공간의 사용액.
///   기간 몫에는 넣지 않고 **연간 몫에서만** 뺀다. 기간 밖 운영은 설정 실수일
///   가능성이 높지만, 그 금액이 연간 한도에서 사라지면 안 되기 때문이다.
pub fn availability(
    annual_limit: i64,
    carryover: bool,
    periods: &[PeriodRow],
    current_seq: Option<i64>,
    used_in_period_before: i64,
    used_outside_before: i64,
) -> Availability {
    let (period_limit, prior_limits, prior_used) = match current_seq {
        Some(cur) => {
            let limit = periods
                .iter()
                .find(|p| p.seq == cur)
                .map(|p| p.limit)
                .unwrap_or(annual_limit);
            let prior: Vec<&PeriodRow> = periods.iter().filter(|p| p.seq < cur).collect();
            (
                limit,
                prior.iter().map(|p| p.limit).sum::<i64>(),
                prior.iter().map(|p| p.used).sum::<i64>(),
            )
        }
        // 지원기간을 만들지 않은 정책 — 연간 한도 하나로 운영한다.
        None => (annual_limit, 0, 0),
    };

    let carry_in = if carryover {
        (prior_limits - prior_used).max(0)
    } else {
        0
    };

    let used_all_before = prior_used + used_in_period_before + used_outside_before;
    let period_share = period_limit + carry_in - used_in_period_before;
    let annual_share = annual_limit - used_all_before;
    let available = period_share.min(annual_share).max(0);

    Availability {
        annual_limit,
        period_limit,
        carry_in,
        used_prior_periods: prior_used,
        used_in_period_before,
        used_outside_before,
        used_all_before,
        available,
        capped_by_annual: annual_share < period_share,
    }
}

/// 작업공간이 어느 지원기간에 속하는지 고른다 (설계안 5-1).
///
/// 1. 작업공간 **시작일을 포함하는** 기간
/// 2. 없으면 겹치는 날이 가장 많은 기간
/// 3. 그래도 없으면 `None` (연간 한도만 적용 + 경고)
pub fn pick_period_seq(
    periods: &[(i64, String, String)], // (seq, start, end)
    ws_start: &str,
    ws_end: &str,
) -> Option<i64> {
    if let Some((seq, _, _)) = periods
        .iter()
        .find(|(_, s, e)| s.as_str() <= ws_start && ws_start <= e.as_str())
    {
        return Some(*seq);
    }
    periods
        .iter()
        .filter(|(_, s, e)| s.as_str() <= ws_end && ws_start <= e.as_str())
        .max_by_key(|(_, s, e)| {
            // 겹치는 날짜 문자열 구간의 크기를 대충 재는 대신, 겹침의 시작/끝을 비교한다.
            let from = if s.as_str() > ws_start { s.as_str() } else { ws_start };
            let to = if e.as_str() < ws_end { e.as_str() } else { ws_end };
            days_between(from, to)
        })
        .map(|(seq, _, _)| *seq)
}

/// `YYYY-MM-DD` 두 날짜 사이의 일수. 파싱에 실패하면 0.
fn days_between(from: &str, to: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(from), parse(to)) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(seq: i64, limit: i64, used: i64) -> PeriodRow {
        PeriodRow { seq, limit, used }
    }

    /// 설계안 5-2의 검증표 — 정책 A(소멸)
    #[test]
    fn 소멸정책_2학기_가용액은_기간한도_그대로() {
        let periods = [p(1, 250_000, 200_000), p(2, 250_000, 0)];
        let a = availability(500_000, false, &periods, Some(2), 0, 0);
        assert_eq!(a.carry_in, 0);
        assert_eq!(a.available, 250_000);
    }

    /// 설계안 5-2의 검증표 — 정책 B(이월)
    #[test]
    fn 이월정책_미사용액이_다음기간으로_넘어간다() {
        let periods = [p(1, 250_000, 200_000), p(2, 250_000, 0)];
        let a = availability(500_000, true, &periods, Some(2), 0, 0);
        assert_eq!(a.carry_in, 50_000);
        assert_eq!(a.available, 300_000);
    }

    #[test]
    fn 이월해도_연간한도를_넘지_못한다() {
        // 1학기를 한 푼도 안 썼다 → 이월 250,000, 기간 몫 500,000
        let periods = [p(1, 250_000, 0), p(2, 250_000, 0)];
        let a = availability(500_000, true, &periods, Some(2), 0, 0);
        assert_eq!(a.carry_in, 250_000);
        assert_eq!(a.available, 500_000); // 연간 한도와 같아서 잘리지 않음
        assert!(!a.capped_by_annual);

        // 연간 한도만 450,000인 학교라면 기간 몫 500,000이 연간에 잘린다
        let b = availability(450_000, true, &periods, Some(2), 0, 0);
        assert_eq!(b.available, 450_000);
        assert!(b.capped_by_annual);
    }

    /// 선행 기간을 다시 정산해 사용액이 늘면 이월액이 자동으로 줄어든다.
    #[test]
    fn 선행기간_재정산이_이월액에_반영된다() {
        let before = availability(500_000, true, &[p(1, 250_000, 200_000), p(2, 250_000, 0)], Some(2), 0, 0);
        assert_eq!(before.available, 300_000);

        let after = availability(500_000, true, &[p(1, 250_000, 230_000), p(2, 250_000, 0)], Some(2), 0, 0);
        assert_eq!(after.carry_in, 20_000);
        assert_eq!(after.available, 270_000);
    }

    /// 한 기간 안에 작업공간이 여럿이어도 한도는 기간 전체에 하나다.
    #[test]
    fn 기간안_작업공간이_여럿이면_같은_한도를_나눠쓴다() {
        let periods = [p(1, 250_000, 0)];
        // 3월에 100,000, 4월에 60,000을 이미 썼고 지금 5월을 정산한다
        let a = availability(500_000, false, &periods, Some(1), 160_000, 0);
        assert_eq!(a.available, 90_000);
        assert_eq!(a.used_all_before, 160_000);
    }

    #[test]
    fn 지원기간을_만들지_않으면_연간한도_하나로_동작한다() {
        let a = availability(500_000, false, &[], None, 120_000, 0);
        assert_eq!(a.period_limit, 500_000);
        assert_eq!(a.carry_in, 0);
        assert_eq!(a.available, 380_000);
    }

    #[test]
    fn 한도를_이미_다_썼으면_가용액은_0이고_음수가_되지_않는다() {
        let periods = [p(1, 250_000, 250_000)];
        let a = availability(500_000, false, &periods, Some(1), 250_000, 0);
        assert_eq!(a.available, 0);

        // 한도를 사후에 낮춰 이전 사용액이 한도를 넘긴 경우
        let periods = [p(1, 100_000, 0)];
        let b = availability(200_000, false, &periods, Some(1), 150_000, 0);
        assert_eq!(b.available, 0);
    }

    #[test]
    fn 기간_밖_작업공간의_사용액은_연간_몫에서만_뺀다() {
        // 어느 지원기간에도 속하지 않는 작업공간에서 100,000을 썼다.
        // 1학기 몫은 그대로 250,000이지만, 연간 몫은 400,000으로 줄어든다.
        let periods = [p(1, 250_000, 0), p(2, 250_000, 0)];
        let a = availability(500_000, false, &periods, Some(1), 0, 100_000);
        assert_eq!(a.period_limit, 250_000);
        assert_eq!(a.available, 250_000, "기간 몫이 더 작으므로 그대로");
        assert_eq!(a.used_all_before, 100_000, "연간 누적에는 잡힌다");

        // 연간 몫이 더 작아지는 지점까지 밀면 연간에 잘린다
        let b = availability(500_000, false, &periods, Some(1), 0, 300_000);
        assert_eq!(b.available, 200_000);
        assert!(b.capped_by_annual);
    }

    #[test]
    fn 기간이_없는_seq를_받으면_연간한도로_대체한다() {
        // 정책은 있는데 기간 목록에 그 seq가 빠진 비정상 상태 — 죽지 않고 연간으로 처리
        let a = availability(500_000, false, &[p(1, 250_000, 0)], Some(9), 0, 0);
        assert_eq!(a.period_limit, 500_000);
    }

    #[test]
    fn 작업공간이_속한_기간을_고른다() {
        let periods = vec![
            (1, "2026-03-01".to_string(), "2026-08-31".to_string()),
            (2, "2026-09-01".to_string(), "2027-02-28".to_string()),
        ];
        assert_eq!(pick_period_seq(&periods, "2026-04-01", "2026-04-30"), Some(1));
        assert_eq!(pick_period_seq(&periods, "2026-09-15", "2026-09-30"), Some(2));
        // 여름방학 특강 — 시작일이 1학기 안이므로 1학기
        assert_eq!(pick_period_seq(&periods, "2026-07-20", "2026-08-20"), Some(1));
        // 학년도 밖 — 속하는 기간 없음
        assert_eq!(pick_period_seq(&periods, "2025-01-01", "2025-01-31"), None);
        // 기간을 걸치면 겹치는 날이 더 많은 쪽
        assert_eq!(pick_period_seq(&periods, "2026-08-25", "2026-10-31"), Some(1));
    }
}
