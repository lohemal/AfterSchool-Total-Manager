//! 학년도 지원금 정책 — 연간한도 · 이월정책 · 대상학년 · 지원기간 (설계안 4-2).
//!
//! 지원기간 행이 0개면 '학년도 전체가 하나의 기간'으로 동작한다.
//! 화면(Phase 3)은 이 표를 그대로 보여주므로, `semester1_limit` 같은 컬럼은 없다.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::parse_grades;
use crate::error::{AppError, AppResult};
use crate::model::{Grant, GrantInput, Period, Policy};
use crate::repo::{check_date, required_text};

pub fn get(conn: &Connection, year_id: i64, program: &str) -> AppResult<Policy> {
    let mut p = conn
        .query_row(
            "SELECT id, program, annual_limit, carryover, target_grades, label_fund, label_over
               FROM support_policy WHERE year_id = ?1 AND program = ?2",
            params![year_id, program],
            |r| {
                Ok(Policy {
                    id: r.get(0)?,
                    program: r.get(1)?,
                    annual_limit: r.get(2)?,
                    carryover: r.get::<_, i64>(3)? == 1,
                    target_grades: r.get(4)?,
                    label_fund: r.get(5)?,
                    label_over: r.get(6)?,
                    periods: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("지원정책을 찾지 못했습니다."))?;

    p.periods = periods_of(conn, p.id)?;
    Ok(p)
}

pub fn list(conn: &Connection, year_id: i64) -> AppResult<Vec<Policy>> {
    let mut out = Vec::new();
    for program in ["VOUCHER", "FREE_VOUCHER"] {
        out.push(get(conn, year_id, program)?);
    }
    Ok(out)
}

pub fn periods_of(conn: &Connection, policy_id: i64) -> AppResult<Vec<Period>> {
    let mut st = conn.prepare(
        "SELECT id, name, start_date, end_date, limit_amount, seq
           FROM support_period WHERE policy_id = ?1 ORDER BY seq",
    )?;
    let rows = st
        .query_map(params![policy_id], |r| {
            Ok(Period {
                id: Some(r.get(0)?),
                name: r.get(1)?,
                start_date: r.get(2)?,
                end_date: r.get(3)?,
                limit_amount: r.get(4)?,
                seq: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 정책 본문과 지원기간 목록을 한 번에 저장한다.
///
/// 기간은 화면에서 통째로 받아 **지우고 다시 넣는다** — 몇 개 되지 않고,
/// 순서·겹침 검사를 한곳에서 하는 편이 안전하다.
pub fn save(
    conn: &Connection,
    year_id: i64,
    program: &str,
    annual_limit: i64,
    carryover: bool,
    target_grades: &str,
    periods: &[Period],
) -> AppResult<()> {
    if annual_limit < 0 {
        return Err(AppError::invalid("연간 지원한도는 0원 이상이어야 합니다."));
    }
    // 대상학년 문자열을 정규화한다 ('4, 3' → '3,4')
    let grades = parse_grades(target_grades)
        .iter()
        .map(|g| g.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let policy_id: i64 = conn
        .query_row(
            "SELECT id FROM support_policy WHERE year_id = ?1 AND program = ?2",
            params![year_id, program],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("지원정책을 찾지 못했습니다."))?;

    conn.execute(
        "UPDATE support_policy
            SET annual_limit = ?2, carryover = ?3, target_grades = ?4,
                updated_at = datetime('now', 'localtime')
          WHERE id = ?1",
        params![policy_id, annual_limit, carryover as i64, grades],
    )?;

    check_periods(periods)?;
    conn.execute(
        "DELETE FROM support_period WHERE policy_id = ?1",
        params![policy_id],
    )?;
    // 지원기간의 `seq`도 화면에 넣은 차례가 아니라 **시작일 순서**로 매긴다.
    // 이월액은 "앞선 기간"을 기준으로 계산하므로 둘이 갈리면 안 된다.
    let mut sorted: Vec<&Period> = periods.iter().collect();
    sorted.sort_by(|a, b| {
        a.start_date
            .cmp(&b.start_date)
            .then(a.end_date.cmp(&b.end_date))
    });
    for (i, p) in sorted.into_iter().enumerate() {
        conn.execute(
            "INSERT INTO support_period (policy_id, name, start_date, end_date, limit_amount, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                policy_id,
                p.name.trim(),
                p.start_date,
                p.end_date,
                p.limit_amount,
                i as i64 + 1
            ],
        )?;
    }
    Ok(())
}

fn check_periods(periods: &[Period]) -> AppResult<()> {
    let mut sorted: Vec<&Period> = periods.iter().collect();
    sorted.sort_by(|a, b| a.start_date.cmp(&b.start_date));

    for p in &sorted {
        required_text("지원기간 이름", &p.name)?;
        check_date("지원기간 시작일", &p.start_date)?;
        check_date("지원기간 종료일", &p.end_date)?;
        if p.start_date > p.end_date {
            return Err(AppError::invalid(format!(
                "'{}'의 종료일이 시작일보다 빠릅니다.",
                p.name
            )));
        }
        if p.limit_amount < 0 {
            return Err(AppError::invalid("지원기간 한도는 0원 이상이어야 합니다."));
        }
    }
    for w in sorted.windows(2) {
        if w[0].end_date >= w[1].start_date {
            return Err(AppError::invalid(format!(
                "'{}'와 '{}'의 기간이 겹칩니다.",
                w[0].name, w[1].name
            )));
        }
    }
    Ok(())
}

/// 기간 한도 합계와 연간 한도가 다른지 알려 준다.
/// **막지는 않는다** — 실제로 다르게 운영하는 학교가 있고, 계산식이 둘 다 지킨다.
pub fn limit_notice(policy: &Policy) -> Option<String> {
    if policy.periods.is_empty() {
        return None;
    }
    let sum: i64 = policy.periods.iter().map(|p| p.limit_amount).sum();
    if sum == policy.annual_limit {
        None
    } else {
        Some(format!(
            "지원기간 한도 합계 {}원 / 연간 한도 {}원 — 확인이 필요합니다.",
            sum, policy.annual_limit
        ))
    }
}

// ─────────────────────────────────────────────── 학생별 예외 한도

/// `period_id`가 NULL이면 **연간 한도**를, 값이 있으면 **그 기간 한도**를 대신한다.
/// 둘은 서로 독립이며 한 학생이 둘 다 가질 수 있다 (설계안 4-2).
pub fn grant_list(conn: &Connection, year_id: i64, program: &str) -> AppResult<Vec<Grant>> {
    let mut st = conn.prepare(
        "SELECT g.id, g.student_id, s.grade, s.class_no, s.student_no, s.name,
                g.program, g.period_id, COALESCE(p.name, ''), g.amount, g.reason
           FROM support_grant g
           JOIN student s ON s.id = g.student_id
           LEFT JOIN support_period p ON p.id = g.period_id
          WHERE g.year_id = ?1 AND g.program = ?2
          ORDER BY s.grade, s.class_no, s.student_no, COALESCE(p.seq, 0)",
    )?;
    let rows = st
        .query_map(params![year_id, program], |r| {
            Ok(Grant {
                id: r.get(0)?,
                student_id: r.get(1)?,
                grade: r.get(2)?,
                class_no: r.get(3)?,
                student_no: r.get(4)?,
                name: r.get(5)?,
                program: r.get(6)?,
                period_id: r.get(7)?,
                period_name: r.get(8)?,
                amount: r.get(9)?,
                reason: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn grant_save(conn: &Connection, year_id: i64, input: &GrantInput) -> AppResult<i64> {
    if input.amount < 0 {
        return Err(AppError::invalid("예외 한도는 0원 이상이어야 합니다."));
    }
    if !matches!(input.program.as_str(), "VOUCHER" | "FREE_VOUCHER") {
        return Err(AppError::invalid("알 수 없는 지원제도입니다."));
    }
    conn.execute(
        "INSERT INTO support_grant (year_id, student_id, program, period_id, amount, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(year_id, student_id, program, IFNULL(period_id, 0))
           DO UPDATE SET amount = excluded.amount,
                         reason = excluded.reason,
                         updated_at = datetime('now', 'localtime')",
        params![
            year_id,
            input.student_id,
            input.program,
            input.period_id,
            input.amount,
            input.reason.clone().unwrap_or_default()
        ],
    )?;
    Ok(conn
        .query_row(
            "SELECT id FROM support_grant
              WHERE year_id = ?1 AND student_id = ?2 AND program = ?3
                AND IFNULL(period_id, 0) = IFNULL(?4, 0)",
            params![year_id, input.student_id, input.program, input.period_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

pub fn grant_delete(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM support_grant WHERE id = ?1", params![id])?;
    }
    Ok(n)
}
