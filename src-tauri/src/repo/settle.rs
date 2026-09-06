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
use crate::domain::{eligibility_active, grade_matches, parse_grades, Fund, Program};
use crate::error::{AppError, AppResult};
use crate::model::{
    BudgetView, CostItem, Fee, GenerateResult, Issue, ProgramRow, SelfPayRow, SettlementStatus,
    Summary, SummaryRow, SupportState,
};
use crate::repo;
use crate::repo::enrollment::dept_label;

const PROGRAMS: [Program; 2] = [Program::Voucher, Program::FreeVoucher];

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

    // 수강 자료
    let active = repo::enrollment::count_active(conn, workspace_id)?;
    if active == 0 {
        out.push(Issue {
            level: "WARN".into(),
            code: "NO_ENROLLMENT".into(),
            message: "수강 중인 자료가 없습니다. 빈 정산이 만들어집니다.".into(),
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
    let negative: i64 = conn.query_row(
        "SELECT COUNT(*) FROM charge c
           JOIN enrollment e ON e.id = c.enrollment_id
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE' AND c.amount < 0",
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
            "SELECT COUNT(DISTINCT e.student_id)
               FROM enrollment e
               JOIN support_eligibility el
                      ON el.student_id = e.student_id AND el.year_id = ?1 AND el.program = ?3
              WHERE e.workspace_id = ?2 AND e.status = 'ACTIVE'",
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
                                    AND e.workspace_id = ?2 AND e.status = 'ACTIVE'
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

    // ACTIVE 수강의 charge를 학생별로 모은다
    let mut st = conn.prepare(
        "SELECT e.student_id, s.grade, e.id, e.department_id, c.item_code, c.amount
           FROM enrollment e
           JOIN student s ON s.id = e.student_id
           JOIN charge  c ON c.enrollment_id = e.id
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE'",
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

    // 원본 charge와 견준다 — 어긋나면 화면에 붉게 띄운다
    let charge_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(c.amount), 0) FROM charge c
           JOIN enrollment e ON e.id = c.enrollment_id
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE'",
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
          ORDER BY s.grade, s.class_no, s.student_no, d.name, d.class_name",
    )?;

    let mut acc: HashMap<(i64, i64), SelfPayRow> = HashMap::new();
    let mut fee_acc: HashMap<(i64, i64), HashMap<String, i64>> = HashMap::new();
    let mut order: Vec<(i64, i64)> = Vec::new();

    for row in st.query_map(params![sid], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
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
          ORDER BY s.grade, s.class_no, s.student_no",
    )?;

    let mut used_acc: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut over_acc: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut info: HashMap<i64, (i64, i64, i64, String)> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();

    for row in st.query_map(params![sid, used_fund, over_fund.unwrap_or("")], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
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
            r.get::<_, i64>(2)?,
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
    out.sort_by_key(|r| (r.grade, r.class_no, r.student_no, r.student_id));
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
        "SELECT DISTINCT e.student_id, s.grade FROM enrollment e
           JOIN student s ON s.id = e.student_id
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE'",
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
