//! 정산 실행과 조회 (설계안 5·8·9장).
//!
//! ## 두 가지를 분리한다
//!
//! * **지원금 잔액 조회** — 언제나 지금 자료로 다시 계산한다.
//! * **저장된 정산결과** — 그때의 스냅샷이며 저절로 바뀌지 않는다.
//!
//! 둘이 어긋나면 [`status`]가 그 사실을 알리고, 다시 만드는 것은 사람이 정한다.
//! 자동 재계산은 하지 않는다 — 사람이 모르는 사이에 확정된 숫자가 바뀌는 편이
//! 훨씬 위험하다.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::settle::{self, Budget, ChargeUnit, Config, StudentInput};
use crate::domain::support::{self, PeriodRow};
use crate::domain::{class_no, eligibility_active, grade_matches, parse_grades, Fund, Program};
use crate::error::{AppError, AppResult};
use crate::model::{
    BudgetView, CostItem, Fee, GenerateResult, Issue, ProgramRow, SelfPayRow, SettlementStatus,
    Summary, SummaryRow, SupportState,
};
use crate::repo;
use crate::repo::enrollment::dept_label;

const PROGRAMS: [Program; 2] = [Program::Voucher, Program::FreeVoucher];

/// **정산 대상 수강은 상태가 아니라 금액으로 정한다** (v0.1.3, 최종 QA).
///
/// 중도에 수강을 취소해도 이미 발생한 비용은 징수한다. 교재비·재료비는 배부한
/// 뒤라면 환불하지 않고, 강사료·수용비는 실제 수강한 만큼 받는다. 그래서
/// **`status = 'ACTIVE'` 로 걸러서는 안 된다** — 그렇게 하면 취소한 학생의
/// 징수금액 전체가 정산에서 사라진다.
///
/// ```text
/// ACTIVE    + 금액 있음  → 정산
/// CANCELLED + 금액 있음  → 정산   ← 예전에는 여기가 빠졌다
/// CANCELLED + 전액 0원   → 정산할 것이 없음
/// ```
///
/// `status` 는 "지금 수강 중인가"만 뜻한다. 그래서 **부서 수강인원·학년도
/// 수강인원·중복 등록 방지에는 `ACTIVE` 조건을 그대로 둔다** — 그것들은
/// 금액이 아니라 실제 수강 중인 사람을 묻는 질문이다.
///
/// 이 조각은 `enrollment` 를 `e` 로 부르는 질의에서만 쓴다.
const SETTLE_TARGET: &str =
    "EXISTS (SELECT 1 FROM charge ct WHERE ct.enrollment_id = e.id AND ct.amount > 0)";

// ─────────────────────────────────────────────── 정책과 기간

struct PeriodDef {
    id: i64,
    seq: i64,
    name: String,
    start: String,
    end: String,
    limit: i64,
}

struct PolicyCtx {
    program: Program,
    annual_limit: i64,
    carryover: bool,
    targets: Vec<i64>,
    periods: Vec<PeriodDef>,
    /// 이번 작업공간이 속한 기간의 seq
    cur_seq: Option<i64>,
}

impl PolicyCtx {
    fn cur(&self) -> Option<&PeriodDef> {
        self.cur_seq
            .and_then(|s| self.periods.iter().find(|p| p.seq == s))
    }
}

fn load_policy(
    conn: &Connection,
    year_id: i64,
    program: Program,
    ws_start: &str,
    ws_end: &str,
) -> AppResult<PolicyCtx> {
    let (policy_id, annual_limit, carryover, targets): (i64, i64, bool, String) = conn
        .query_row(
            "SELECT id, annual_limit, carryover, target_grades
               FROM support_policy WHERE year_id = ?1 AND program = ?2",
            params![year_id, program.code()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? == 1,
                    r.get(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            AppError::not_found(format!("{} 지원정책이 없습니다.", program.label()))
        })?;

    let mut st = conn.prepare(
        "SELECT id, seq, name, start_date, end_date, limit_amount
           FROM support_period WHERE policy_id = ?1 ORDER BY seq",
    )?;
    let periods: Vec<PeriodDef> = st
        .query_map(params![policy_id], |r| {
            Ok(PeriodDef {
                id: r.get(0)?,
                seq: r.get(1)?,
                name: r.get(2)?,
                start: r.get(3)?,
                end: r.get(4)?,
                limit: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let ranges: Vec<(i64, String, String)> = periods
        .iter()
        .map(|p| (p.seq, p.start.clone(), p.end.clone()))
        .collect();
    let cur_seq = support::pick_period_seq(&ranges, ws_start, ws_end);

    Ok(PolicyCtx {
        program,
        annual_limit,
        carryover,
        targets: parse_grades(&targets),
        periods,
        cur_seq,
    })
}

/// 학년도의 작업공간을 `(id, 시작, 끝)`로. 정렬은 언제나 (시작, 끝, id).
fn year_workspaces(conn: &Connection, year_id: i64) -> AppResult<Vec<(i64, String, String)>> {
    let mut st = conn.prepare(
        "SELECT id, start_date, end_date FROM workspace
          WHERE year_id = ?1 ORDER BY start_date, end_date, id",
    )?;
    let rows = st
        .query_map(params![year_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 다른 작업공간들의 최신 정산에서 학생별·제도별 사용액을 모은다.
/// **이번 작업공간은 뺀다** — 지금 다시 계산하는 중이기 때문이다.
type UsageMap = HashMap<(i64, i64, &'static str), i64>; // (workspace, student, program) → 금액

fn prior_usage(conn: &Connection, year_id: i64, exclude_ws: i64) -> AppResult<UsageMap> {
    let mut st = conn.prepare(
        "SELECT s.workspace_id, a.student_id, a.fund, SUM(a.amount)
           FROM settlement_alloc a
           JOIN settlement s ON s.id = a.settlement_id AND s.is_latest = 1
           JOIN workspace  w ON w.id = s.workspace_id
          WHERE w.year_id = ?1 AND w.id <> ?2 AND a.fund IN ('VOUCHER', 'FREE_VOUCHER')
          GROUP BY s.workspace_id, a.student_id, a.fund",
    )?;
    let mut out: UsageMap = HashMap::new();
    let rows = st
        .query_map(params![year_id, exclude_ws], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (ws, student, fund, amount) in rows {
        let program: &'static str = match fund.as_str() {
            "VOUCHER" => "VOUCHER",
            _ => "FREE_VOUCHER",
        };
        *out.entry((ws, student, program)).or_insert(0) += amount;
    }
    Ok(out)
}

/// 학생별 예외 한도. 키는 `(학생, 제도, 기간id 또는 0=연간)`.
fn grants(conn: &Connection, year_id: i64) -> AppResult<HashMap<(i64, String, i64), i64>> {
    let mut st = conn.prepare(
        "SELECT student_id, program, IFNULL(period_id, 0), amount
           FROM support_grant WHERE year_id = ?1",
    )?;
    let rows = st
        .query_map(params![year_id], |r| {
            Ok((
                (r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?),
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

struct Ctx {
    workspace_id: i64,
    year_id: i64,
    ws_start: String,
    ws_end: String,
    policies: Vec<PolicyCtx>,
    usage: UsageMap,
    grants: HashMap<(i64, String, i64), i64>,
    /// 다른 작업공간이 어느 기간에 속하는지 (제도별로 기간이 다를 수 있다)
    ws_list: Vec<(i64, String, String)>,
}

impl Ctx {
    fn load(conn: &Connection, workspace_id: i64) -> AppResult<Self> {
        let ws = repo::year::get_workspace(conn, workspace_id)?;
        let mut policies = Vec::new();
        for p in PROGRAMS {
            policies.push(load_policy(conn, ws.year_id, p, &ws.start_date, &ws.end_date)?);
        }
        Ok(Self {
            workspace_id,
            year_id: ws.year_id,
            ws_start: ws.start_date.clone(),
            ws_end: ws.end_date.clone(),
            policies,
            usage: prior_usage(conn, ws.year_id, workspace_id)?,
            grants: grants(conn, ws.year_id)?,
            ws_list: year_workspaces(conn, ws.year_id)?,
        })
    }

    fn policy(&self, program: Program) -> &PolicyCtx {
        self.policies
            .iter()
            .find(|p| p.program == program)
            .expect("정책은 언제나 두 개다")
    }

    fn precedes(&self, other: &(i64, String, String)) -> bool {
        (other.1.as_str(), other.2.as_str(), other.0)
            < (self.ws_start.as_str(), self.ws_end.as_str(), self.workspace_id)
    }

    /// 학생 하나·제도 하나의 사용 가능액 (설계안 5-2).
    fn availability(&self, student_id: i64, program: Program) -> support::Availability {
        let pol = self.policy(program);
        let code = program.code();

        let limit_of = |period_id: i64, fallback: i64| -> i64 {
            self.grants
                .get(&(student_id, code.to_string(), period_id))
                .copied()
                .unwrap_or(fallback)
        };
        let annual = limit_of(0, pol.annual_limit);

        // 다른 작업공간의 사용액을 기간별로 나눈다
        let mut period_used: HashMap<i64, i64> = HashMap::new();
        let mut used_in_period_before = 0i64;
        let mut used_outside_before = 0i64;

        let ranges: Vec<(i64, String, String)> = pol
            .periods
            .iter()
            .map(|p| (p.seq, p.start.clone(), p.end.clone()))
            .collect();

        for w in &self.ws_list {
            if w.0 == self.workspace_id {
                continue;
            }
            let amount = self
                .usage
                .get(&(w.0, student_id, code))
                .copied()
                .unwrap_or(0);
            if amount == 0 {
                continue;
            }
            let seq = support::pick_period_seq(&ranges, &w.1, &w.2);
            match (seq, pol.cur_seq) {
                (Some(k), Some(cur)) if k < cur => *period_used.entry(k).or_insert(0) += amount,
                (Some(k), Some(cur)) if k == cur && self.precedes(w) => {
                    used_in_period_before += amount
                }
                (Some(_), Some(_)) => {} // 뒤에 오는 작업공간 — 이번 계산에 넣지 않는다
                (_, None) if self.precedes(w) => used_in_period_before += amount,
                (None, Some(_)) if self.precedes(w) => used_outside_before += amount,
                _ => {}
            }
        }

        let rows: Vec<PeriodRow> = pol
            .periods
            .iter()
            .map(|p| PeriodRow {
                seq: p.seq,
                limit: limit_of(p.id, p.limit),
                used: period_used.get(&p.seq).copied().unwrap_or(0),
            })
            .collect();

        support::availability(
            annual,
            pol.carryover,
            &rows,
            pol.cur_seq,
            used_in_period_before,
            used_outside_before,
        )
    }

    /// 이 학생이 이 작업공간에서 그 제도의 자격을 가지는가.
    fn eligible(&self, conn: &Connection, student_id: i64, grade: i64, program: Program) -> AppResult<bool> {
        let pol = self.policy(program);
        if !grade_matches(&pol.targets, grade) {
            return Ok(false);
        }
        let mut st = conn.prepare(
            "SELECT valid_from, valid_to FROM support_eligibility
              WHERE year_id = ?1 AND student_id = ?2 AND program = ?3",
        )?;
        let rows = st
            .query_map(params![self.year_id, student_id, program.code()], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.iter().any(|(f, t)| {
            eligibility_active(f.as_deref(), t.as_deref(), &self.ws_start, &self.ws_end)
        }))
    }
}

// ─────────────────────────────────────────────── 정산 생성 전 검사

pub fn validate(conn: &Connection, workspace_id: i64) -> AppResult<Vec<Issue>> {
    let mut out = Vec::new();
    let ctx = Ctx::load(conn, workspace_id)?;
    let items = repo::cost_items(conn)?;

    // 수강 자료 — 정산 대상은 상태가 아니라 금액으로 센다 (SETTLE_TARGET 참고)
    let targets: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM enrollment e
              WHERE e.workspace_id = ?1 AND {SETTLE_TARGET}"
        ),
        params![workspace_id],
        |r| r.get(0),
    )?;
    if targets == 0 {
        out.push(Issue {
            level: "WARN".into(),
            code: "NO_ENROLLMENT".into(),
            message: "정산할 금액이 있는 수강이 없습니다. 빈 정산이 만들어집니다.".into(),
        });
    }

    // 예전 버전에서 취소한 수강은 금액이 취소 전 그대로 남아 있을 수 있다.
    // 그것이 이제 정산에 들어가므로 **프로그램이 추측하지 않고 사람에게 확인시킨다** —
    // 0원으로 만들면 징수할 돈을 놓치고, 전액으로 두면 안 받을 돈을 받는다.
    let stale_cancels: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM enrollment e
              WHERE e.workspace_id = ?1 AND e.status = 'CANCELLED' AND {SETTLE_TARGET}
                AND NOT EXISTS (SELECT 1 FROM charge c2
                                 WHERE c2.enrollment_id = e.id AND c2.is_overridden = 1)"
        ),
        params![workspace_id],
        |r| r.get(0),
    )?;
    if stale_cancels > 0 {
        out.push(Issue {
            level: "WARN".into(),
            code: "CANCEL_FULL_AMOUNT".into(),
            message: format!(
                "취소된 수강 {stale_cancels}건의 금액이 부서 기준금액 그대로입니다. \
                 실제 징수할 금액인지 확인해 주세요 (수강생 명단 → 상태: 취소)."
            ),
        });
    }

    // 금액이 없는 수강 / 음수 금액
    let broken: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollment e
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE'
            AND NOT EXISTS (SELECT 1 FROM charge c WHERE c.enrollment_id = e.id)",
        params![workspace_id],
        |r| r.get(0),
    )?;
    if broken > 0 {
        out.push(Issue {
            level: "ERROR".into(),
            code: "NO_CHARGE".into(),
            message: format!("금액이 없는 수강이 {broken}건 있습니다. 수강생 명단에서 확인해 주세요."),
        });
    }
    // 음수는 상태와 무관하게 다 본다 — 취소 건에 음수가 있으면 불변식이 깨진다.
    let negative: i64 = conn.query_row(
        "SELECT COUNT(*) FROM charge c
           JOIN enrollment e ON e.id = c.enrollment_id
          WHERE e.workspace_id = ?1 AND c.amount < 0",
        params![workspace_id],
        |r| r.get(0),
    )?;
    if negative > 0 {
        out.push(Issue {
            level: "ERROR".into(),
            code: "NEGATIVE_AMOUNT".into(),
            message: format!("음수 금액이 {negative}건 있습니다."),
        });
    }

    // 제도별 정책 상태
    for program in PROGRAMS {
        let pol = ctx.policy(program);
        let label = program.label();

        let holders: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT e.student_id)
                   FROM enrollment e
                   JOIN support_eligibility el
                          ON el.student_id = e.student_id AND el.year_id = ?1
                         AND el.program = ?3
                  WHERE e.workspace_id = ?2 AND {SETTLE_TARGET}"
            ),
            params![ctx.year_id, workspace_id, program.code()],
            |r| r.get(0),
        )?;
        if holders == 0 {
            continue;
        }

        if pol.annual_limit == 0 {
            out.push(Issue {
                level: "WARN".into(),
                code: "NO_LIMIT".into(),
                message: format!(
                    "{label} 대상자가 {holders}명 있으나 연간 지원한도가 0원입니다. 전액 학부모 부담으로 계산됩니다."
                ),
            });
        }
        if pol.periods.is_empty() {
            out.push(Issue {
                level: "WARN".into(),
                code: "NO_PERIOD".into(),
                message: format!("{label}에 지원기간이 없습니다. 연간 한도 하나로 계산합니다."),
            });
        } else {
            if pol.cur_seq.is_none() {
                out.push(Issue {
                    level: "WARN".into(),
                    code: "OUTSIDE_PERIOD".into(),
                    message: format!(
                        "이 작업공간({} ~ {})이 {label}의 어느 지원기간에도 속하지 않습니다. 연간 한도만 적용됩니다.",
                        ctx.ws_start, ctx.ws_end
                    ),
                });
            }
            let sum: i64 = pol.periods.iter().map(|p| p.limit).sum();
            if sum != pol.annual_limit {
                out.push(Issue {
                    level: "WARN".into(),
                    code: "LIMIT_MISMATCH".into(),
                    message: format!(
                        "{label}의 지원기간 한도 합계({sum}원)가 연간 한도({}원)와 다릅니다.",
                        pol.annual_limit
                    ),
                });
            }
        }

        // 대상학년 불일치
        if !pol.targets.is_empty() {
            let list = pol
                .targets
                .iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT COUNT(DISTINCT s.id)
                   FROM support_eligibility el
                   JOIN student s ON s.id = el.student_id
                   JOIN enrollment e ON e.student_id = s.id
                                    AND e.workspace_id = ?2 AND {SETTLE_TARGET}
                  WHERE el.year_id = ?1 AND el.program = ?3 AND s.grade NOT IN ({list})"
            );
            let n: i64 = conn.query_row(&sql, params![ctx.year_id, workspace_id, program.code()], |r| {
                r.get(0)
            })?;
            if n > 0 {
                out.push(Issue {
                    level: "WARN".into(),
                    code: "GRADE_MISMATCH".into(),
                    message: format!(
                        "{label} 대상학년이 아닌 수강생 {n}명이 대상자 명단에 있습니다. 이 학생들은 지원되지 않습니다."
                    ),
                });
            }
        }
    }

    // 우선순위
    if !repo::priority::has_dept_priority(conn, workspace_id)? {
        out.push(Issue {
            level: "WARN".into(),
            code: "NO_DEPT_PRIORITY".into(),
            message: "부서 차감 우선순위를 정하지 않았습니다. 부서 등록 순서대로 차감합니다.".into(),
        });
    }
    let has_item: i64 = conn.query_row(
        "SELECT COUNT(*) FROM item_priority WHERE workspace_id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )?;
    if has_item == 0 {
        let names = items
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(" > ");
        out.push(Issue {
            level: "WARN".into(),
            code: "NO_ITEM_PRIORITY".into(),
            message: format!("비용항목 우선순위를 정하지 않았습니다. 기본 순서({names})로 차감합니다."),
        });
    }

    // 선행 작업공간 가운데 정산이 없는 것
    let mut missing: Vec<String> = Vec::new();
    for w in &ctx.ws_list {
        if w.0 == workspace_id || !ctx.precedes(w) {
            continue;
        }
        let has: i64 = conn.query_row(
            "SELECT COUNT(*) FROM settlement WHERE workspace_id = ?1 AND is_latest = 1",
            params![w.0],
            |r| r.get(0),
        )?;
        if has == 0 {
            let name: String = conn.query_row(
                "SELECT name FROM workspace WHERE id = ?1",
                params![w.0],
                |r| r.get(0),
            )?;
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        out.push(Issue {
            level: "WARN".into(),
            code: "PRIOR_NOT_SETTLED".into(),
            message: format!(
                "앞선 작업공간({})의 정산이 아직 없습니다. 이전 사용액이 0원으로 계산됩니다.",
                missing.join(", ")
            ),
        });
    }

    Ok(out)
}

// ─────────────────────────────────────────────── 낡음 판정

fn prior_key(conn: &Connection, ctx: &Ctx) -> AppResult<String> {
    let mut parts = Vec::new();
    for w in &ctx.ws_list {
        if w.0 == ctx.workspace_id || !ctx.precedes(w) {
            continue;
        }
        let row: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, created_at FROM settlement WHERE workspace_id = ?1 AND is_latest = 1",
                params![w.0],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((sid, at)) => parts.push(format!("{}:{}:{}", w.0, sid, at)),
            None => parts.push(format!("{}:-:-", w.0)),
        }
    }
    Ok(parts.join("|"))
}

pub fn status(conn: &Connection, workspace_id: i64) -> AppResult<SettlementStatus> {
    let none = |msg: &str| SettlementStatus {
        state: "NONE".into(),
        message: msg.into(),
        settlement_id: None,
        created_at: None,
        program_order: String::new(),
        prior_name: None,
        prior_is_earlier_period: false,
    };

    let saved: Option<(i64, String, i64, i64, String, String)> = conn
        .query_row(
            "SELECT id, created_at, ws_data_version, year_data_version, prior_key, program_order
               FROM settlement WHERE workspace_id = ?1 AND is_latest = 1",
            params![workspace_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;

    let Some((id, created_at, ws_v, yr_v, key, order)) = saved else {
        return Ok(none("정산 데이터가 없습니다. [정산 데이터 생성]을 눌러 주세요."));
    };

    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;

    let mut out = SettlementStatus {
        state: "FRESH".into(),
        message: format!("{created_at} 생성 · 최신"),
        settlement_id: Some(id),
        created_at: Some(created_at.clone()),
        program_order: order,
        prior_name: None,
        prior_is_earlier_period: false,
    };

    if ws.data_version != ws_v {
        out.state = "STALE_DATA".into();
        out.message = "정산 이후 수강·부서 자료가 변경되었습니다. 다시 정산해 주세요.".into();
        return Ok(out);
    }
    if year.data_version != yr_v {
        out.state = "STALE_YEAR".into();
        out.message = "정산 이후 지원대상·지원금 설정이 변경되었습니다. 다시 정산해 주세요.".into();
        return Ok(out);
    }

    let ctx = Ctx::load(conn, workspace_id)?;
    let now = prior_key(conn, &ctx)?;
    if now != key {
        // 어느 작업공간이 달라졌는지 찾아 사람 말로 알려 준다
        let before: Vec<&str> = key.split('|').filter(|s| !s.is_empty()).collect();
        let after: Vec<&str> = now.split('|').filter(|s| !s.is_empty()).collect();
        let changed_ws: Option<i64> = after
            .iter()
            .zip(before.iter().chain(std::iter::repeat(&"")))
            .find(|(a, b)| a != b)
            .and_then(|(a, _)| a.split(':').next().and_then(|s| s.parse().ok()))
            .or_else(|| {
                after
                    .first()
                    .and_then(|a| a.split(':').next().and_then(|s| s.parse().ok()))
            });

        out.state = "STALE_PRIOR".into();
        if let Some(wid) = changed_ws {
            let name: Option<String> = conn
                .query_row("SELECT name FROM workspace WHERE id = ?1", params![wid], |r| {
                    r.get(0)
                })
                .optional()?;
            let pol = ctx.policy(Program::Voucher);
            let ranges: Vec<(i64, String, String)> = pol
                .periods
                .iter()
                .map(|p| (p.seq, p.start.clone(), p.end.clone()))
                .collect();
            let other = ctx.ws_list.iter().find(|w| w.0 == wid);
            let other_seq = other.and_then(|w| support::pick_period_seq(&ranges, &w.1, &w.2));
            let earlier = matches!((other_seq, pol.cur_seq), (Some(a), Some(b)) if a < b);
            let period_name = other_seq
                .and_then(|s| pol.periods.iter().find(|p| p.seq == s))
                .map(|p| p.name.clone());

            out.prior_name = name.clone();
            out.prior_is_earlier_period = earlier;
            out.message = if earlier {
                format!(
                    "선행 지원기간({})의 정산이 변경되어 이월액이 달라졌습니다. 다시 정산해 주세요.",
                    period_name.unwrap_or_else(|| "이전 기간".into())
                )
            } else {
                format!(
                    "선행 작업공간({})의 정산이 변경되었습니다. 현재 작업공간의 정산 데이터를 다시 생성해 주세요.",
                    name.unwrap_or_else(|| "이전".into())
                )
            };
        } else {
            out.message =
                "선행 작업공간의 정산이 변경되었습니다. 다시 정산해 주세요.".into();
        }
        return Ok(out);
    }

    Ok(out)
}

// ─────────────────────────────────────────────── 정산 생성

pub fn generate(conn: &Connection, workspace_id: i64) -> AppResult<GenerateResult> {
    let issues = validate(conn, workspace_id)?;
    let errors: Vec<&Issue> = issues.iter().filter(|i| i.level == "ERROR").collect();
    if !errors.is_empty() {
        return Err(AppError::invalid(format!(
            "정산을 진행할 수 없습니다.\n{}",
            errors
                .iter()
                .map(|e| format!("· {}", e.message))
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }

    let ctx = Ctx::load(conn, workspace_id)?;
    let items = repo::cost_items(conn)?;
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ctx.year_id)?;

    // 차감 순서
    let dept_order = repo::priority::dept_order(conn, workspace_id)?;
    let item_order = repo::priority::item_order(conn, workspace_id)?;
    let program_order = load_program_order(conn)?;
    let cfg = Config::new(program_order.clone(), &dept_order, &item_order);

    // 정산 대상 charge를 학생별로 모은다.
    //
    // **상태로 걸러지 않는다** (SETTLE_TARGET 참고). 취소한 수강도 징수할 금액이
    // 남아 있으면 여기 들어온다. `c.amount > 0` 만 보므로 전액 면제한 취소는
    // 자연히 빠지고, 0원짜리 배분 행이 만들어지는 일도 없다.
    let mut st = conn.prepare(
        "SELECT e.student_id, s.grade, e.id, e.department_id, c.item_code, c.amount
           FROM enrollment e
           JOIN student s ON s.id = e.student_id
           JOIN charge  c ON c.enrollment_id = e.id
          WHERE e.workspace_id = ?1 AND c.amount > 0",
    )?;
    let raw = st
        .query_map(params![workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                ChargeUnit {
                    enrollment_id: r.get(2)?,
                    department_id: r.get(3)?,
                    item_code: r.get(4)?,
                    amount: r.get(5)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let mut by_student: HashMap<i64, (i64, Vec<ChargeUnit>)> = HashMap::new();
    for (student_id, grade, unit) in raw {
        by_student
            .entry(student_id)
            .or_insert_with(|| (grade, Vec::new()))
            .1
            .push(unit);
    }

    // 학생 순서를 고정한다 (결과가 실행할 때마다 달라지지 않도록)
    let mut student_ids: Vec<i64> = by_student.keys().copied().collect();
    student_ids.sort_unstable();

    let mut all_charges: Vec<ChargeUnit> = Vec::new();
    let mut all_allocs: Vec<settle::Alloc> = Vec::new();
    let mut budgets: Vec<(i64, Program, support::Availability, i64)> = Vec::new();

    for sid in &student_ids {
        let (grade, charges) = by_student.get(sid).expect("방금 넣었다");
        let mut budget_of = HashMap::new();
        for program in PROGRAMS {
            let eligible = ctx.eligible(conn, *sid, *grade, program)?;
            if eligible {
                let a = ctx.availability(*sid, program);
                budget_of.insert(program.code(), (Budget::of(a.available), Some(a)));
            } else {
                budget_of.insert(program.code(), (Budget::none(), None));
            }
        }

        let input = StudentInput {
            student_id: *sid,
            charges: charges.clone(),
            voucher: budget_of[Program::Voucher.code()].0,
            free_voucher: budget_of[Program::FreeVoucher.code()].0,
        };
        let result = settle::settle_student(&input, &cfg);

        // 학생 단위로도 불변식을 확인한다 — 어디서 깨졌는지 바로 알 수 있다
        settle::verify(&input.charges, &result.allocs).map_err(|e| {
            AppError::new("SETTLE_INVARIANT", "정산 금액이 맞지 않아 저장하지 않았습니다.")
                .detail(format!("학생 {sid}: {e}"))
        })?;

        for program in PROGRAMS {
            if let Some(a) = budget_of[program.code()].1 {
                budgets.push((*sid, program, a, result.used.of(program)));
            }
        }
        all_charges.extend(charges.clone());
        all_allocs.extend(result.allocs);
    }

    // 전체 불변식
    settle::verify(&all_charges, &all_allocs).map_err(|e| {
        AppError::new("SETTLE_INVARIANT", "정산 금액이 맞지 않아 저장하지 않았습니다.").detail(e)
    })?;

    // 저장 — 기존 최신 정산은 내리고 새로 만든다. 과거 정산은 지우지 않는다.
    conn.execute(
        "UPDATE settlement SET is_latest = 0 WHERE workspace_id = ?1 AND is_latest = 1",
        params![workspace_id],
    )?;
    let warnings: Vec<&Issue> = issues.iter().filter(|i| i.level == "WARN").collect();
    conn.execute(
        "INSERT INTO settlement
           (workspace_id, ws_data_version, year_data_version, prior_key, program_order,
            is_latest, warning_json)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
        params![
            workspace_id,
            ws.data_version,
            year.data_version,
            prior_key(conn, &ctx)?,
            program_order
                .iter()
                .map(|p| p.code())
                .collect::<Vec<_>>()
                .join(","),
            serde_json::to_string(&warnings)?
        ],
    )?;
    let settlement_id = conn.last_insert_rowid();

    for a in &all_allocs {
        conn.execute(
            "INSERT INTO settlement_alloc
               (settlement_id, student_id, department_id, enrollment_id, item_code,
                fund, origin, amount)
             VALUES (?1,
                     (SELECT student_id FROM enrollment WHERE id = ?2),
                     ?3, ?2, ?4, ?5, ?6, ?7)",
            params![
                settlement_id,
                a.enrollment_id,
                a.department_id,
                a.item_code,
                a.fund.code(),
                a.origin.code(),
                a.amount
            ],
        )?;
    }

    for (sid, program, a, used_now) in &budgets {
        let pol = ctx.policy(*program);
        let cur = pol.cur();
        conn.execute(
            "INSERT INTO settlement_budget
               (settlement_id, student_id, program, annual_limit, period_id, period_name,
                period_limit, carryover, carry_in, used_prior_periods, used_in_period_before,
                used_outside_before, used_all_before, capped_by_annual, available, used_now)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                settlement_id,
                sid,
                program.code(),
                a.annual_limit,
                cur.map(|p| p.id),
                cur.map(|p| p.name.clone()).unwrap_or_default(),
                a.period_limit,
                pol.carryover as i64,
                a.carry_in,
                a.used_prior_periods,
                a.used_in_period_before,
                a.used_outside_before,
                a.used_all_before,
                a.capped_by_annual as i64,
                a.available,
                used_now
            ],
        )?;
    }

    let created_at: String = conn.query_row(
        "SELECT created_at FROM settlement WHERE id = ?1",
        params![settlement_id],
        |r| r.get(0),
    )?;

    let _ = items; // 검사에서만 쓴다
    Ok(GenerateResult {
        settlement_id,
        created_at,
        students: student_ids.len() as i64,
        allocs: all_allocs.len() as i64,
        total: all_allocs.iter().map(|a| a.amount).sum(),
        warnings: warnings.into_iter().cloned().collect(),
    })
}

fn load_program_order(conn: &Connection) -> AppResult<Vec<Program>> {
    let raw = repo::setting::get(conn, "program_order")?
        .unwrap_or_else(|| "VOUCHER,FREE_VOUCHER".to_string());
    let mut out: Vec<Program> = raw.split(',').filter_map(|c| Program::from_code(c.trim())).collect();
    // 빠진 제도가 있으면 뒤에 붙인다 — 설정 실수로 제도가 통째로 무시되면 안 된다
    for p in PROGRAMS {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────── 조회

fn latest_id(conn: &Connection, workspace_id: i64) -> AppResult<Option<(i64, String)>> {
    Ok(conn
        .query_row(
            "SELECT id, created_at FROM settlement WHERE workspace_id = ?1 AND is_latest = 1",
            params![workspace_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub fn summary(conn: &Connection, workspace_id: i64, items: &[CostItem]) -> AppResult<Option<Summary>> {
    let Some((sid, created_at)) = latest_id(conn, workspace_id)? else {
        return Ok(None);
    };

    let mut st = conn.prepare(
        "SELECT item_code, fund, SUM(amount) FROM settlement_alloc
          WHERE settlement_id = ?1 GROUP BY item_code, fund",
    )?;
    let mut map: HashMap<(String, String), i64> = HashMap::new();
    for row in st.query_map(params![sid], |r| {
        Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (k, v) = row?;
        map.insert(k, v);
    }
    drop(st);

    let pick = |item: &str, fund: Fund| map.get(&(item.to_string(), fund.code().to_string())).copied().unwrap_or(0);

    let mut rows = Vec::new();
    let mut total = SummaryRow {
        item_code: "TOTAL".into(),
        item_name: "합계".into(),
        self_pay: 0,
        voucher: 0,
        voucher_over: 0,
        free_voucher: 0,
        total: 0,
    };
    for it in items {
        let row = SummaryRow {
            item_code: it.code.clone(),
            item_name: it.name.clone(),
            self_pay: pick(&it.code, Fund::SelfPay),
            voucher: pick(&it.code, Fund::Voucher),
            voucher_over: pick(&it.code, Fund::VoucherOver),
            free_voucher: pick(&it.code, Fund::FreeVoucher),
            total: 0,
        };
        let row = SummaryRow {
            total: row.self_pay + row.voucher + row.voucher_over + row.free_voucher,
            ..row
        };
        total.self_pay += row.self_pay;
        total.voucher += row.voucher;
        total.voucher_over += row.voucher_over;
        total.free_voucher += row.free_voucher;
        total.total += row.total;
        rows.push(row);
    }

    // 원본 charge와 견준다 — 어긋나면 화면에 붉게 띄운다.
    // **정산에 넣은 것과 똑같은 집합**이어야 한다. 한쪽만 상태로 걸러면
    // 취소자가 있는 작업공간에서 늘 어긋난다고 나온다.
    let charge_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(c.amount), 0) FROM charge c
           JOIN enrollment e ON e.id = c.enrollment_id
          WHERE e.workspace_id = ?1 AND c.amount > 0",
        params![workspace_id],
        |r| r.get(0),
    )?;

    Ok(Some(Summary {
        balanced: total.total == charge_total,
        rows,
        total,
        charge_total,
        created_at,
    }))
}

fn fees_of(items: &[CostItem], map: &HashMap<String, i64>) -> (Vec<Fee>, i64) {
    let fees: Vec<Fee> = items
        .iter()
        .map(|it| Fee {
            item_code: it.code.clone(),
            amount: map.get(&it.code).copied().unwrap_or(0),
        })
        .collect();
    let total = fees.iter().map(|f| f.amount).sum();
    (fees, total)
}

/// 수익자 탭 — 실제 학부모 부담이 생긴 줄만 (요구사항 §16).
pub fn self_pay_rows(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
) -> AppResult<Vec<SelfPayRow>> {
    let Some((sid, _)) = latest_id(conn, workspace_id)? else {
        return Ok(Vec::new());
    };

    let mut st = conn.prepare(
        "SELECT a.student_id, s.grade, s.class_no, s.student_no, s.name,
                a.department_id, d.name, d.class_name,
                a.item_code, a.fund, a.origin, SUM(a.amount)
           FROM settlement_alloc a
           JOIN student s    ON s.id = a.student_id
           JOIN department d ON d.id = a.department_id
          WHERE a.settlement_id = ?1 AND a.fund IN ('SELF_PAY', 'VOUCHER_OVER')
          GROUP BY a.student_id, a.department_id, a.item_code, a.fund, a.origin
          ORDER BY s.grade, s.class_sort, s.class_no, s.student_no, d.name, d.class_name",
    )?;

    let mut acc: HashMap<(i64, i64), SelfPayRow> = HashMap::new();
    let mut fee_acc: HashMap<(i64, i64), HashMap<String, i64>> = HashMap::new();
    let mut order: Vec<(i64, i64)> = Vec::new();

    for row in st.query_map(params![sid], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, String>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, i64>(11)?,
        ))
    })? {
        let (student_id, grade, class_no, student_no, name, dept_id, dname, dclass, item, fund, origin, amount) = row?;
        let key = (student_id, dept_id);
        if !acc.contains_key(&key) {
            order.push(key);
            acc.insert(
                key,
                SelfPayRow {
                    student_id,
                    grade,
                    class_no,
                    student_no,
                    name,
                    department_id: dept_id,
                    dept_label: dept_label(&dname, &dclass),
                    fees: Vec::new(),
                    total: 0,
                    self_pay: 0,
                    voucher_over: 0,
                    origin_plain: 0,
                    origin_voucher: 0,
                    origin_free: 0,
                },
            );
        }
        let e = acc.get_mut(&key).expect("방금 넣었다");
        *fee_acc.entry(key).or_default().entry(item).or_insert(0) += amount;
        match fund.as_str() {
            "SELF_PAY" => e.self_pay += amount,
            _ => e.voucher_over += amount,
        }
        match origin.as_str() {
            "VOUCHER_EXHAUSTED" => e.origin_voucher += amount,
            "FREE_EXHAUSTED" => e.origin_free += amount,
            _ => e.origin_plain += amount,
        }
    }
    drop(st);

    Ok(order
        .into_iter()
        .filter_map(|k| {
            let mut row = acc.remove(&k)?;
            let (fees, total) = fees_of(items, fee_acc.get(&k).unwrap_or(&HashMap::new()));
            row.fees = fees;
            row.total = total;
            Some(row)
        })
        .collect())
}

/// 수익자 탭 — 학생별 합계 + 부서별 상세 (v0.1.4).
///
/// 이용권·자유수강권 탭과 같은 모양으로 맞춘다 — **목록에서 학생이 얼마인지
/// 보고, 눌러서 왜 그 금액인지 본다.**
///
/// ## 원본 charge 를 더하지 않는다
///
/// 여기서 더하는 것은 정산 스냅샷의 배분액 가운데 **학부모 부담**
/// (`SELF_PAY` + `VOUCHER_OVER`)뿐이다. 이용권으로 정상 지원된 금액은 들어오지
/// 않는다. 학생별 징수 내역(원본 `charge` 전체)과 섞으면 학부모가 실제로 내는
/// 돈이 부풀려진다.
///
/// 집계만 한다 — `self_pay_rows`가 만든 줄을 학생별로 모으는 것이 전부이고,
/// 정산 결과의 총액은 달라지지 않는다.
pub fn self_pay_report(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
) -> AppResult<crate::model::SelfPayReport> {
    let details = self_pay_rows(conn, workspace_id, items)?;

    let mut order: Vec<i64> = Vec::new();
    let mut rows: HashMap<i64, crate::model::StudentSumRow> = HashMap::new();
    let mut per_student: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut grand: HashMap<String, i64> = HashMap::new();

    // 지원유형은 학생 자격에서 온다 — 배분액에는 들어 있지 않다.
    let programs = student_programs(conn, workspace_id)?;

    for d in &details {
        if !rows.contains_key(&d.student_id) {
            order.push(d.student_id);
            rows.insert(
                d.student_id,
                crate::model::StudentSumRow {
                    student_id: d.student_id,
                    grade: d.grade,
                    class_no: d.class_no.clone(),
                    student_no: d.student_no,
                    name: d.name.clone(),
                    programs: programs.get(&d.student_id).cloned().unwrap_or_default(),
                    fees: Vec::new(),
                    total: 0,
                    details: 0,
                },
            );
        }
        let row = rows.get_mut(&d.student_id).expect("방금 넣었다");
        row.details += 1;
        let mine = per_student.entry(d.student_id).or_default();
        for f in &d.fees {
            *mine.entry(f.item_code.clone()).or_insert(0) += f.amount;
            *grand.entry(f.item_code.clone()).or_insert(0) += f.amount;
        }
    }

    let out: Vec<crate::model::StudentSumRow> = order
        .into_iter()
        .filter_map(|id| {
            let mut row = rows.remove(&id)?;
            let empty = HashMap::new();
            let map = per_student.get(&id).unwrap_or(&empty);
            let (fees, total) = fees_of(items, map);
            row.fees = fees;
            row.total = total;
            Some(row)
        })
        .collect();

    let (fees, total) = fees_of(items, &grand);
    Ok(crate::model::SelfPayReport {
        rows: out,
        details,
        fees,
        total,
    })
}

/// 이 작업공간 기간에 유효한 학생별 지원제도.
///
/// 유효기간 판단은 수강생 명단과 같은 규칙이다 — 작업공간 기간과 겹치면 유효.
fn student_programs(conn: &Connection, workspace_id: i64) -> AppResult<HashMap<i64, Vec<String>>> {
    let mut st = conn.prepare(
        "SELECT DISTINCT e.student_id, el.program
           FROM enrollment e
           JOIN workspace w ON w.id = e.workspace_id
           JOIN support_eligibility el ON el.student_id = e.student_id
          WHERE e.workspace_id = ?1
            AND (el.valid_from IS NULL OR el.valid_from <= w.end_date)
            AND (el.valid_to   IS NULL OR el.valid_to   >= w.start_date)
          ORDER BY e.student_id, el.program",
    )?;
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for row in st.query_map(params![workspace_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, program) = row?;
        out.entry(id).or_default().push(program);
    }
    Ok(out)
}

/// 이용권 · 자유수강권 탭 (요구사항 §17·§18).
pub fn program_rows(
    conn: &Connection,
    workspace_id: i64,
    program: Program,
    items: &[CostItem],
) -> AppResult<Vec<ProgramRow>> {
    let Some((sid, _)) = latest_id(conn, workspace_id)? else {
        return Ok(Vec::new());
    };

    let (used_fund, over_fund) = match program {
        Program::Voucher => ("VOUCHER", Some("VOUCHER_OVER")),
        Program::FreeVoucher => ("FREE_VOUCHER", None),
    };

    let mut st = conn.prepare(
        "SELECT a.student_id, s.grade, s.class_no, s.student_no, s.name,
                a.item_code, a.fund, SUM(a.amount)
           FROM settlement_alloc a
           JOIN student s ON s.id = a.student_id
          WHERE a.settlement_id = ?1 AND a.fund IN (?2, ?3)
          GROUP BY a.student_id, a.item_code, a.fund
          ORDER BY s.grade, s.class_sort, s.class_no, s.student_no",
    )?;

    let mut used_acc: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut over_acc: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut info: HashMap<i64, (i64, String, i64, String)> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();

    for row in st.query_map(params![sid, used_fund, over_fund.unwrap_or("")], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })? {
        let (student_id, grade, class_no, student_no, name, item, fund, amount) = row?;
        if !info.contains_key(&student_id) {
            order.push(student_id);
            info.insert(student_id, (grade, class_no, student_no, name));
        }
        if fund == used_fund {
            *used_acc.entry(student_id).or_default().entry(item).or_insert(0) += amount;
        } else {
            *over_acc.entry(student_id).or_default().entry(item).or_insert(0) += amount;
        }
    }
    drop(st);

    // 이 제도의 자격이 있으나 한 푼도 안 쓴 학생도 보여 준다 (잔액을 봐야 하므로)
    let mut st = conn.prepare(
        "SELECT b.student_id, s.grade, s.class_no, s.student_no, s.name
           FROM settlement_budget b JOIN student s ON s.id = b.student_id
          WHERE b.settlement_id = ?1 AND b.program = ?2",
    )?;
    for row in st.query_map(params![sid, program.code()], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (student_id, grade, class_no, student_no, name) = row?;
        if !info.contains_key(&student_id) {
            order.push(student_id);
            info.insert(student_id, (grade, class_no, student_no, name));
        }
    }
    drop(st);

    let budgets = budget_map(conn, sid, program)?;
    let empty = HashMap::new();

    let mut out: Vec<ProgramRow> = order
        .into_iter()
        .map(|student_id| {
            let (grade, class_no, student_no, name) = info[&student_id].clone();
            let (used, used_total) = fees_of(items, used_acc.get(&student_id).unwrap_or(&empty));
            let (over, over_total) = if over_fund.is_some() {
                fees_of(items, over_acc.get(&student_id).unwrap_or(&empty))
            } else {
                (Vec::new(), 0)
            };
            ProgramRow {
                student_id,
                grade,
                class_no,
                student_no,
                name,
                used,
                used_total,
                over,
                over_total,
                budget: budgets.get(&student_id).cloned(),
            }
        })
        .collect();
    // 숫자 반이 1, 10, 2 로 놓이지 않도록 반 정렬 규칙을 쓴다.
    out.sort_by(|a, b| {
        a.grade
            .cmp(&b.grade)
            .then(class_no::cmp(&a.class_no, &b.class_no))
            .then(a.student_no.cmp(&b.student_no))
            .then(a.student_id.cmp(&b.student_id))
    });
    Ok(out)
}

fn budget_map(
    conn: &Connection,
    settlement_id: i64,
    program: Program,
) -> AppResult<HashMap<i64, BudgetView>> {
    let mut st = conn.prepare(
        "SELECT student_id, annual_limit, period_name, period_limit, carryover, carry_in,
                used_prior_periods, used_in_period_before, used_all_before, capped_by_annual,
                available, used_now
           FROM settlement_budget WHERE settlement_id = ?1 AND program = ?2",
    )?;
    let rows = st
        .query_map(params![settlement_id, program.code()], |r| {
            let student_id: i64 = r.get(0)?;
            let annual_limit: i64 = r.get(1)?;
            let available: i64 = r.get(10)?;
            let used_now: i64 = r.get(11)?;
            let used_all_before: i64 = r.get(8)?;
            Ok((
                student_id,
                BudgetView {
                    program: program.code().to_string(),
                    annual_limit,
                    period_name: r.get(2)?,
                    period_limit: r.get(3)?,
                    carryover: r.get::<_, i64>(4)? == 1,
                    carry_in: r.get(5)?,
                    used_prior_periods: r.get(6)?,
                    used_in_period_before: r.get(7)?,
                    used_all_before,
                    capped_by_annual: r.get::<_, i64>(9)? == 1,
                    available,
                    used_now,
                    period_left: available - used_now,
                    annual_used: used_all_before + used_now,
                    annual_left: annual_limit - (used_all_before + used_now),
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

/// 학생 상세정보의 지원제도 칸 (요구사항 §19).
///
/// 자격이 없으면 `NONE`(해당없음), 정산이 없으면 `BEFORE`(정산 전),
/// 낡았으면 `STALE`(재정산 필요). **`OK`일 때만 숫자를 준다.**
pub fn student_supports(
    conn: &Connection,
    workspace_id: i64,
    student_id: i64,
) -> AppResult<Vec<SupportState>> {
    let st = status(conn, workspace_id)?;
    let ctx = Ctx::load(conn, workspace_id)?;
    let grade: i64 = conn.query_row(
        "SELECT grade FROM student WHERE id = ?1",
        params![student_id],
        |r| r.get(0),
    )?;

    let mut out = Vec::new();
    for program in PROGRAMS {
        let eligible = ctx.eligible(conn, student_id, grade, program)?;
        let (state, budget) = if !eligible {
            ("NONE", None)
        } else if st.state == "NONE" {
            ("BEFORE", None)
        } else if !st.is_fresh() {
            ("STALE", None)
        } else {
            let sid = st.settlement_id.expect("최신이면 id가 있다");
            let b = budget_map(conn, sid, program)?.get(&student_id).cloned();
            match b {
                Some(b) => ("OK", Some(b)),
                None => ("BEFORE", None),
            }
        };
        out.push(SupportState {
            program: program.code().to_string(),
            program_label: program.label().to_string(),
            state: state.to_string(),
            budget,
        });
    }
    Ok(out)
}

/// 정산 기록을 지운다 (다시 만들기 전에 화면에서 부르지는 않는다 — 감사용 보존).
pub fn history(conn: &Connection, workspace_id: i64) -> AppResult<Vec<(i64, String, bool)>> {
    let mut st = conn.prepare(
        "SELECT id, created_at, is_latest FROM settlement
          WHERE workspace_id = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? == 1))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 이 작업공간에서 이용권 자격이 있는 학생 수 — 화면 안내에 쓴다.
pub fn voucher_holder_count(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    let ctx = Ctx::load(conn, workspace_id)?;
    let mut st = conn.prepare(
        &format!(
            "SELECT DISTINCT e.student_id, s.grade FROM enrollment e
               JOIN student s ON s.id = e.student_id
              WHERE e.workspace_id = ?1 AND {SETTLE_TARGET}"
        ),
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let mut seen = HashSet::new();
    for (sid, grade) in rows {
        if ctx.eligible(conn, sid, grade, Program::Voucher)? {
            seen.insert(sid);
        }
    }
    Ok(seen.len() as i64)
}

// ─────────────────────────────────────────────── 행정자료 공통 (Phase 4)

/// 행정자료를 만들기 전에 **최신 유효 정산인지** 확인하고 그 id를 돌려준다.
///
/// 낡은 정산으로 Excel을 내보내는 것은 이 프로그램에서 가장 위험한 일이다.
/// 잘못된 숫자가 파일이 되어 학교 밖으로 나가면 되돌릴 수 없기 때문에,
/// 경고가 아니라 **막는다** (요구사항 §1).
pub fn require_fresh(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    let st = status(conn, workspace_id)?;
    match st.state.as_str() {
        "FRESH" => st
            .settlement_id
            .ok_or_else(|| AppError::not_found("정산 결과를 찾지 못했습니다.")),
        "NONE" => Err(AppError::invalid(
            "정산 전입니다. [정산 데이터 생성]을 먼저 실행해 주세요.",
        )),
        _ => Err(AppError::invalid(format!(
            "재정산이 필요합니다. {} 지금 자료로 파일을 만들면 틀린 금액이 나갑니다.",
            st.message
        ))),
    }
}

/// 학생 한 명의 정산 내역 — 부서 × 항목 × 재원 (요구사항 §3·§4의 상세 팝업).
pub fn student_allocs(
    conn: &Connection,
    workspace_id: i64,
    student_id: i64,
) -> AppResult<Vec<crate::model::StudentAllocRow>> {
    let Some((sid, _)) = latest_id(conn, workspace_id)? else {
        return Ok(Vec::new());
    };
    let items = repo::cost_items(conn)?;
    let name_of: HashMap<String, String> = items
        .iter()
        .map(|i| (i.code.clone(), i.name.clone()))
        .collect();
    let order: HashMap<String, usize> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (it.code.clone(), i))
        .collect();

    let mut st = conn.prepare(
        "SELECT a.department_id, d.name, d.class_name, a.item_code, a.fund, a.origin,
                SUM(a.amount)
           FROM settlement_alloc a
           JOIN department d ON d.id = a.department_id
          WHERE a.settlement_id = ?1 AND a.student_id = ?2
          GROUP BY a.department_id, a.item_code, a.fund, a.origin
          ORDER BY d.name, d.class_name",
    )?;
    let mut rows = st
        .query_map(params![sid, student_id], |r| {
            let code: String = r.get(3)?;
            let fund: String = r.get(4)?;
            let origin: String = r.get(5)?;
            Ok(crate::model::StudentAllocRow {
                department_id: r.get(0)?,
                dept_label: dept_label(&r.get::<_, String>(1)?, &r.get::<_, String>(2)?),
                item_name: name_of.get(&code).cloned().unwrap_or_default(),
                item_code: code,
                fund_label: fund_label(&fund).to_string(),
                fund,
                origin_label: origin_label(&origin).to_string(),
                origin,
                amount: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    rows.sort_by_key(|r| {
        (
            r.dept_label.clone(),
            order.get(&r.item_code).copied().unwrap_or(usize::MAX),
            r.fund.clone(),
        )
    });
    Ok(rows)
}

/// 내부 코드를 사람이 읽는 말로. 화면에 `PLAIN` 같은 코드를 그대로 쓰지 않는다.
pub fn fund_label(code: &str) -> &'static str {
    match code {
        "SELF_PAY" => "수익자 부담금",
        "VOUCHER" => "이용권 지원",
        "VOUCHER_OVER" => "이용권 초과금",
        "FREE_VOUCHER" => "자유수강권 지원",
        _ => "기타",
    }
}

pub fn origin_label(code: &str) -> &'static str {
    match code {
        "VOUCHER_EXHAUSTED" => "이용권 소진 후 발생",
        "FREE_EXHAUSTED" => "자유수강권 소진 후 발생",
        _ => "일반 수익자",
    }
}
