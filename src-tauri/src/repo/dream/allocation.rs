//! 방과후드림 지원금 재배분 — P3 재검증 · 계산 · 검산 · 저장 · 무효화 (P4).
//!
//! ```text
//! [배분 계산]  작업 기준값(파일 지문 · 정산 FRESH · 정산 id · 자료판) 다시 확인
//!              → P3 매칭 전체를 다시(정산 전체성 · 양수 Charge 누락 · 파일 ΣN 포함)
//!              → 모두 준비 완료일 때만 학생마다 V → F → O 배분(domain::dream_alloc)
//!              → 학생마다 · 파일 전체 검산
//!              → 모두 통과해야 한 트랜잭션에 줄 기록(before · after) · 기록 · 상태
//! ```
//!
//! READY 작업이어도 이 과정을 건너뛰지 않는다. 계산은 메모리에서 끝내고, 하나라도 실패하면
//! after 를 한 줄도 적지 않는다.
//!
//! ## 결과가 아직 유효한가 — 지문
//!
//! 배분 결과에는 지문(SHA-256)을 함께 적는다(`dream_job_event` 의 `ALLOCATE`). 재료는
//! 원본 지문 · 정산 id · 자료판 · 배분 차례 · 학생마다 기준액 · 줄마다 학생 · 부서 · 수강 ·
//! 항목 · N · 결과다. 학생 확정 · 강좌 연결 · 우선순위 · 정산 · 파일 가운데 하나라도 바뀌면
//! 지문이 달라져 이전 결과를 쓰지 않는다(`verify`). 우선순위를 바꾸면 끝나지 않은 작업의
//! after 를 곧바로 비우기도 한다. 다시 검증(P3)도 줄 기록을 새로 쓰므로 after 가 지워진다.
//!
//! 기존 정산 자료(정산 · 배분 · 한도 · Charge · 기존 우선순위)는 읽기만 한다.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::crypto;
use crate::domain::dream::ITEMS;
use crate::domain::dream_alloc::{self as alloc, Line, Priority, Target, Totals};
use crate::domain::dream_file::DreamFile;
use crate::error::{AppError, AppResult};
use crate::repo;
use crate::repo::dream::amount::{self, StudentAmount};
use crate::repo::dream::matching::{self, Matching};
use crate::repo::dream::session::{self, Session};
use crate::repo::dream::{job, priority};
use crate::repo::enrollment::dept_label;

// ─────────────────────────────────────────────── 우선순위

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityItem {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityDept {
    pub department_id: i64,
    pub label: String,
    /// 정산할 금액이 있거나 강좌코드에 연결된 부서 — 화면에 늘어놓는 대상
    pub in_use: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityView {
    /// `DREAM` | `SETTLEMENT` | `DEFAULT`
    pub dept_source: &'static str,
    /// 배분할 차례대로
    pub departments: Vec<PriorityDept>,
    /// 공통 항목 차례 — 이 작업공간의 모든 부서에 같다(P6)
    pub items: Vec<PriorityItem>,
    /// `DREAM` | `LEGACY`(v0.1.11 부서별 차례가 모두 같았음) | `CONFLICT`(부서마다 달랐음 —
    /// 저장해야 배분) | `SETTLEMENT` | `DEFAULT`
    pub item_source: &'static str,
    /// 지금 차례의 지문 — 배분 결과가 어느 차례로 계산됐는지 견줄 때
    pub digest: String,
}

/// 지금 배분에 쓸 차례 — 작업공간의 모든 부서와 공통 항목 차례(결정적 대체 규칙 포함).
pub fn current_priority(conn: &Connection, workspace_id: i64) -> AppResult<Priority> {
    Ok(Priority {
        depts: priority::dept_order(conn, workspace_id)?,
        items: priority::common_item_order(conn, workspace_id)?.order,
    })
}

fn digest(p: &Priority) -> String {
    crypto::sha256_hex(p.canonical().as_bytes())
}

/// 화면에 늘어놓을 부서 — 정산할 금액이 있는 수강(취소 포함, `SETTLE_TARGET` 과 같은 뜻)이
/// 있거나 강좌코드에 연결된 부서. 저장은 작업공간 전체 차례로 한다.
fn in_use(conn: &Connection, ws: i64) -> AppResult<BTreeSet<i64>> {
    let mut st = conn.prepare(
        "SELECT e.department_id FROM enrollment e
          WHERE e.workspace_id = ?1
            AND EXISTS (SELECT 1 FROM charge ct WHERE ct.enrollment_id = e.id AND ct.amount > 0)
         UNION
         SELECT department_id FROM dream_course_map WHERE workspace_id = ?1",
    )?;
    let ids = st
        .query_map(params![ws], |r| r.get(0))?
        .collect::<rusqlite::Result<BTreeSet<i64>>>()?;
    Ok(ids)
}

pub fn priority_view(conn: &Connection, workspace_id: i64) -> AppResult<PriorityView> {
    let p = current_priority(conn, workspace_id)?;
    let dept_source = if priority::has_dept_order(conn, workspace_id)? {
        "DREAM"
    } else if repo::priority::has_dept_priority(conn, workspace_id)? {
        "SETTLEMENT"
    } else {
        "DEFAULT"
    };
    let names: HashMap<String, String> = repo::cost_items(conn)?.into_iter().map(|i| (i.code, i.name)).collect();
    let used = in_use(conn, workspace_id)?;
    let mut departments = Vec::new();
    for d in &p.depts {
        let (n, c) = repo::department::name_of(conn, *d)?;
        departments.push(PriorityDept {
            department_id: *d,
            label: dept_label(&n, &c),
            in_use: used.contains(d),
        });
    }
    let items = priority::common_item_order(conn, workspace_id)?;
    Ok(PriorityView {
        dept_source,
        departments,
        items: items
            .order
            .iter()
            .map(|code| PriorityItem {
                code: code.clone(),
                name: names.get(code).cloned().unwrap_or_else(|| code.clone()),
            })
            .collect(),
        item_source: items.source,
        digest: digest(&p),
    })
}

/// 차례가 실제로 바뀌었으면 끝나지 않은 작업의 배분 결과를 비운다. 돌려주는 값은 비운 작업 수.
fn invalidate_if_changed(conn: &Connection, ws: i64, before: &Priority) -> AppResult<usize> {
    let after = current_priority(conn, ws)?;
    if after == *before {
        return Ok(0);
    }
    Ok(job::clear_after(conn, ws, "PRIORITY_CHANGED")?.len())
}

/// 부서 차례 저장. `order` 는 화면에 늘어놓은 부서만이어도 된다 — 나머지 부서는 지금 차례
/// 그대로 뒤에 붙여 작업공간 전체 차례로 저장한다. 기존 정산 순위 · 자료판은 그대로다.
pub fn save_dept_priority(conn: &Connection, workspace_id: i64, order: &[i64]) -> AppResult<usize> {
    let before = current_priority(conn, workspace_id)?;
    let mut full = order.to_vec();
    for d in &before.depts {
        if !full.contains(d) {
            full.push(*d);
        }
    }
    priority::save_dept_order(conn, workspace_id, &full)?;
    invalidate_if_changed(conn, workspace_id, &before)
}

/// 이 작업공간의 공통 항목 차례 저장 — 그 작업공간의 모든 부서에 같다. 저장된 차례가 실제로
/// 바뀌었으면 **이 작업공간의** 끝나지 않은 작업의 배분 결과만 비운다(다른 작업공간은 그대로).
/// 돌려주는 값은 비운 작업 수. 기존 정산 항목 순위 · v0.1.11 부서별 차례 표는 건드리지 않는다.
pub fn save_common_item_order(conn: &Connection, workspace_id: i64, order: &[String]) -> AppResult<usize> {
    // 쓰던 차례(기본값 포함)와 같은 차례를 처음 저장하는 것은 바뀐 것이 아니다
    let before = priority::common_item_order(conn, workspace_id)?;
    priority::save_common_item_order(conn, workspace_id, order)?;
    let after = priority::common_item_order(conn, workspace_id)?;
    if before.order == after.order && before.source != "CONFLICT" {
        return Ok(0);
    }
    Ok(job::clear_after(conn, workspace_id, "ITEM_ORDER_CHANGED")?.len())
}

// ─────────────────────────────────────────────── 배분 결과

/// 배분 단계의 문제. `detail` 에는 개인정보가 아닌 것(코드 · 금액 · 건수)만.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: &'static str,
    /// 그 학생의 첫 줄 번호(학생 문제일 때)
    pub row: Option<u32>,
    pub detail: Option<String>,
}

pub mod code {
    /// P3 매칭 · 검증이 모두 준비 완료가 아니다
    pub const P3_NOT_READY: &str = "P3_NOT_READY";
    /// 검증을 마친 학생인데 배분에 필요한 값이 없다(내부 일관성)
    pub const INTERNAL_INCONSISTENT: &str = "ALLOC_INTERNAL_INCONSISTENT";
    /// 정산에 금액이 남은 학생이 배분 대상에 없다
    pub const SETTLEMENT_STUDENT_MISSING: &str = "ALLOC_SETTLEMENT_STUDENT_MISSING";
    /// 저장된 배분 결과가 지금 자료 · 차례와 맞지 않는다(다시 계산 필요)
    pub const STALE: &str = "ALLOCATION_STALE";
    /// v0.1.11 부서별 항목 차례가 부서마다 달라 공통 차례를 정할 수 없다 — 사람이 저장해야 한다
    pub const ITEM_ORDER_NOT_CONFIRMED: &str = "ITEM_ORDER_NOT_CONFIRMED";
}

fn issue(code: &'static str, row: Option<u32>, detail: Option<String>) -> Issue {
    Issue { code, row, detail }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocLine {
    pub source_row: u32,
    /// 배분 차례(1부터)
    pub seq: u32,
    pub department_id: i64,
    pub dept_label: String,
    pub item_code: String,
    pub item_name: String,
    pub amount_n: i64,
    /// 원본 O · P · Q
    pub before: (i64, i64, i64),
    /// 새 O · P · Q
    pub after: (i64, i64, i64),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeptOrder {
    pub label: String,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocStudent {
    pub key: u32,
    pub student_id: i64,
    pub student_label: String,
    pub amount: StudentAmount,
    pub before: Totals,
    pub after: Totals,
    pub lines: Vec<AllocLine>,
    /// 이 학생에게 적용된 부서 차례와 (공통) 항목 차례
    pub order: Vec<DeptOrder>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub job_id: i64,
    pub job_status: String,
    /// `DONE`(배분 검증 완료) | `NOT_READY`(P3 가 준비 완료가 아님) | `FAILED`(배분 · 검산 실패)
    /// | `STALE`(저장된 결과가 지금과 맞지 않음 — 다시 계산)
    pub status: &'static str,
    pub issues: Vec<Issue>,
    /// 배분 계산 때 다시 돌린 P3 결과(유효성 확인에는 없다)
    pub session: Option<Session>,
    pub students: Vec<AllocStudent>,
    /// 원본 O · P · Q 합계(N 은 파일 ΣN)
    pub before: Totals,
    pub after: Totals,
    /// 적용 학년 정산 합 — n = T, o = 개인부담, p = 이용권 V, q = 자유수강권 F
    pub settlement: Totals,
    /// 이 결과가 다룬 방과후드림 적용 학년
    pub applied_grades: Vec<i64>,
    pub priority_digest: String,
    pub fingerprint: Option<String>,
}

/// 메모리에서 끝낸 배분 — 아직 아무것도 적지 않았다.
struct Plan {
    students: Vec<AllocStudent>,
    before: Totals,
    after: Totals,
    settlement: Totals,
    after_by_row: HashMap<u32, (i64, i64, i64)>,
    priority_digest: String,
    fingerprint: String,
}

fn item_name(code: &str) -> String {
    ITEMS
        .iter()
        .find(|(_, _, c)| *c == code)
        .map(|(_, n, _)| n.to_string())
        .unwrap_or_else(|| code.to_string())
}

/// 학생마다 배분하고 학생 · 파일 전체를 검산한다. 읽기만 한다.
fn plan(conn: &Connection, head: &job::JobHead, file: &DreamFile, m: &Matching) -> AppResult<Result<Plan, Vec<Issue>>> {
    if !m.p4_ready {
        return Ok(Err(vec![issue(code::P3_NOT_READY, None, None)]));
    }
    let ws = head.workspace_id;
    let pr = current_priority(conn, ws)?;
    if priority::common_item_order(conn, ws)?.source == "CONFLICT" {
        return Ok(Err(vec![issue(code::ITEM_ORDER_NOT_CONFIRMED, None, None)]));
    }
    let prio_digest = digest(&pr);
    let grades = &m.applied_grades;
    let labels: HashMap<i64, String> = m.departments.iter().map(|d| (d.id, d.label.clone())).collect();
    let by_row: HashMap<u32, &crate::domain::dream_file::DreamRow> = file.rows.iter().map(|r| (r.source_row, r)).collect();

    // 적용 학년 정산(학생마다 기준액의 합) — 한도 기록 교차 확인을 다시 거친다. 학교 전체가 아니다
    let (_, every) = amount::workspace_amounts(conn, ws)?;
    let every: HashMap<i64, StudentAmount> = every.into_iter().map(|a| (a.student_id, a)).collect();
    let mut all: Vec<StudentAmount> = matching::universe(conn, &every, grades)?.into_values().collect();
    all.sort_by_key(|a| a.student_id);
    let mut settlement = Totals::default();
    for a in &all {
        settlement.add(a.total, a.personal, a.voucher, a.free_voucher);
    }

    let mut issues = Vec::new();
    let mut students = Vec::new();
    let mut before = Totals::default();
    let mut after = Totals::default();
    let mut after_by_row = HashMap::new();
    // 지문 재료 — 차례가 정해진 글
    let mut fp = format!(
        "job={};settlement={:?};ws={:?};year={:?};grades={};prio={};",
        head.source_sha256,
        head.settlement_id,
        head.ws_data_version,
        head.year_data_version,
        grades.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(","),
        pr.canonical()
    );

    let mut sorted: Vec<&matching::StudentMatch> = m.students.iter().collect();
    sorted.sort_by_key(|s| s.student_id);
    for s in sorted {
        let (Some(sid), Some(a), "MATCHED") = (s.student_id, s.amount.as_ref(), s.ref_state) else {
            issues.push(issue(code::INTERNAL_INCONSISTENT, Some(s.key), Some("STUDENT".into())));
            continue;
        };
        let mut lines = Vec::new();
        for l in &s.lines {
            match (l.department_id, l.item_code, l.enrollment_id) {
                (Some(d), Some(i), Some(_)) => lines.push(Line {
                    source_row: l.source_row,
                    department_id: d,
                    item_code: i.to_string(),
                    amount_n: l.amount_n,
                }),
                _ => issues.push(issue(code::INTERNAL_INCONSISTENT, Some(l.source_row), Some("LINE".into()))),
            }
        }
        let target = Target {
            total: a.total,
            voucher: a.voucher,
            free_voucher: a.free_voucher,
            personal: a.personal,
        };
        let splits = match alloc::allocate(&lines, &target, &pr) {
            Ok(x) => x,
            Err(c) => {
                issues.push(issue(c, Some(s.key), None));
                continue;
            }
        };

        fp.push_str(&format!("s{sid}:{},{},{},{};", a.total, a.voucher, a.free_voucher, a.personal));
        let mut st_before = Totals::default();
        let mut st_after = Totals::default();
        let mut out_lines = Vec::new();
        for sp in &splits {
            let l = s.lines.iter().find(|x| x.source_row == sp.source_row).expect("배분은 입력 줄만 돌려준다");
            let r = by_row.get(&sp.source_row);
            let b = (
                r.and_then(|x| x.personal_before).unwrap_or(0),
                r.and_then(|x| x.voucher_before).unwrap_or(0),
                r.and_then(|x| x.free_voucher_before).unwrap_or(0),
            );
            let dept = l.department_id.expect("위에서 확인");
            let item = l.item_code.expect("위에서 확인");
            fp.push_str(&format!(
                "r{}:{sid}:{dept}:{}:{item}:{}:{},{},{};",
                sp.source_row,
                l.enrollment_id.expect("위에서 확인"),
                l.amount_n,
                sp.o,
                sp.p,
                sp.q
            ));
            st_before.add(l.amount_n, b.0, b.1, b.2);
            st_after.add(l.amount_n, sp.o, sp.p, sp.q);
            after_by_row.insert(sp.source_row, (sp.o, sp.p, sp.q));
            out_lines.push(AllocLine {
                source_row: sp.source_row,
                seq: sp.seq,
                department_id: dept,
                dept_label: labels.get(&dept).cloned().unwrap_or_default(),
                item_code: item.to_string(),
                item_name: item_name(item),
                amount_n: l.amount_n,
                before: b,
                after: (sp.o, sp.p, sp.q),
            });
        }
        // 이 학생에게 적용된 차례(근거 표시)
        let mine: BTreeSet<i64> = lines.iter().map(|l| l.department_id).collect();
        let order = pr
            .depts
            .iter()
            .filter(|d| mine.contains(d))
            .map(|d| DeptOrder {
                label: labels.get(d).cloned().unwrap_or_default(),
                items: pr.items.iter().map(|c| item_name(c)).collect(),
            })
            .collect();
        before.add(st_before.n, st_before.o, st_before.p, st_before.q);
        after.add(st_after.n, st_after.o, st_after.p, st_after.q);
        students.push(AllocStudent {
            key: s.key,
            student_id: sid,
            student_label: s.student_label.clone().unwrap_or_default(),
            amount: a.clone(),
            before: st_before,
            after: st_after,
            lines: out_lines,
            order,
        });
    }

    // 정산에 금액이 남은 학생은 모두 배분 대상이어야 한다(P3 가 이미 보지만 한 번 더)
    let planned: BTreeSet<i64> = students.iter().map(|s| s.student_id).collect();
    let missing = all.iter().filter(|a| a.total > 0 && !planned.contains(&a.student_id)).count();
    if missing > 0 && issues.is_empty() {
        issues.push(issue(code::SETTLEMENT_STUDENT_MISSING, None, Some(missing.to_string())));
    }
    if !issues.is_empty() {
        return Ok(Err(issues));
    }
    // 파일 전체 검산 — ΣN = T, ΣP = V, ΣQ = F, ΣO = 개인부담, ΣN = ΣO + ΣP + ΣQ
    let whole = Target {
        total: settlement.n,
        voucher: settlement.p,
        free_voucher: settlement.q,
        personal: settlement.o,
    };
    if let Err(c) = alloc::verify_file(&after, &whole) {
        return Ok(Err(vec![issue(
            c,
            None,
            Some(format!("파일 {}/{}/{}/{} · 정산 {}/{}/{}/{}", after.n, after.o, after.p, after.q, settlement.n, settlement.o, settlement.p, settlement.q)),
        )]));
    }
    if after_by_row.len() != file.rows.len() {
        return Ok(Err(vec![issue(code::INTERNAL_INCONSISTENT, None, Some("ROWS".into()))]));
    }
    students.sort_by_key(|s| s.key);
    Ok(Ok(Plan {
        students,
        before,
        after,
        settlement,
        after_by_row,
        priority_digest: prio_digest,
        fingerprint: crypto::sha256_hex(fp.as_bytes()),
    }))
}

fn fp_of(detail: &str) -> Option<&str> {
    detail.split_whitespace().find_map(|w| w.strip_prefix("fp="))
}

/// [배분 계산] · [배분 다시 계산]. 쓰기 트랜잭션 안에서 부른다.
///
/// 파일 · 정산이 작업을 만들 때와 다르면 오류(호출하는 쪽이 작업을 닫는다). P3 가 준비 완료가
/// 아니거나 배분 · 검산이 실패하면 after 없이 P3 결과만 다시 적고 그 까닭을 돌려준다.
pub fn allocate(conn: &Connection, job_id: i64, bytes: &[u8]) -> AppResult<Allocation> {
    let head = session::check_job(conn, job_id, bytes)?;
    let file = session::parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let key = session::usable_key(conn)?;
    let m = matching::run(conn, head.workspace_id, &file, key.as_ref())?;
    let p4 = m.p4_ready;
    let grades = m.applied_grades.clone();
    match plan(conn, &head, &file, &m)? {
        Ok(p) => {
            // 여기까지 메모리에서 끝났다 — 이제 한 번에 적는다
            session::record(conn, job_id, &file, &m, Some(&p.after_by_row))?;
            job::set_status(conn, job_id, "READY", None)?;
            job::add_event(
                conn,
                job_id,
                "ALLOCATE",
                &format!(
                    "fp={} prio={} students={} rows={}",
                    p.fingerprint,
                    p.priority_digest,
                    p.students.len(),
                    p.after_by_row.len()
                ),
            )?;
            Ok(Allocation {
                job_id,
                job_status: "READY".into(),
                status: "DONE",
                issues: Vec::new(),
                session: Some(Session {
                    job_id,
                    job_status: "READY".into(),
                    matching: m,
                }),
                students: p.students,
                before: p.before,
                after: p.after,
                settlement: p.settlement,
                applied_grades: grades.clone(),
                priority_digest: p.priority_digest,
                fingerprint: Some(p.fingerprint),
            })
        }
        Err(issues) => {
            session::record(conn, job_id, &file, &m, None)?;
            let st = if p4 { "BLOCKED" } else { session::job_status_of(&m) };
            job::set_status(conn, job_id, st, None)?;
            let codes: Vec<&str> = issues.iter().map(|i| i.code).collect();
            job::add_event(conn, job_id, "ALLOCATE_FAILED", &codes.join(","))?;
            let before = file.rows.iter().fold(Totals::default(), |mut t, r| {
                t.add(
                    r.amount_n.unwrap_or(0),
                    r.personal_before.unwrap_or(0),
                    r.voucher_before.unwrap_or(0),
                    r.free_voucher_before.unwrap_or(0),
                );
                t
            });
            Ok(Allocation {
                job_id,
                job_status: st.into(),
                status: if p4 { "FAILED" } else { "NOT_READY" },
                issues,
                session: Some(Session {
                    job_id,
                    job_status: st.into(),
                    matching: m,
                }),
                students: Vec::new(),
                before,
                after: Totals::default(),
                settlement: Totals::default(),
                applied_grades: grades.clone(),
                priority_digest: digest(&current_priority(conn, head.workspace_id)?),
                fingerprint: None,
            })
        }
    }
}

/// 저장된 배분 결과가 **지금도** 유효한가 — 아무것도 적지 않는다(P5 진입 전에 다시 쓴다).
///
/// 지금 자료로 처음부터 다시 계산해, 지문과 줄마다의 after 가 저장된 것과 모두 같을 때만
/// `DONE`. 우선순위 · 학생 확정 · 강좌 연결 · 정산 · 파일 가운데 하나라도 바뀌었으면 `STALE`.
pub fn verify(conn: &Connection, job_id: i64, bytes: &[u8]) -> AppResult<Allocation> {
    let head = session::check_job(conn, job_id, bytes)?;
    let file = session::parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let key = session::usable_key(conn)?;
    let m = matching::run(conn, head.workspace_id, &file, key.as_ref())?;
    let p4 = m.p4_ready;
    let grades = m.applied_grades.clone();
    let empty = |status: &'static str, issues: Vec<Issue>, digest_: String| Allocation {
        job_id,
        job_status: head.status.clone(),
        status,
        issues,
        session: None,
        students: Vec::new(),
        before: Totals::default(),
        after: Totals::default(),
        settlement: Totals::default(),
        applied_grades: grades.clone(),
        priority_digest: digest_,
        fingerprint: None,
    };
    let p = match plan(conn, &head, &file, &m)? {
        Ok(p) => p,
        Err(issues) => {
            let d = digest(&current_priority(conn, head.workspace_id)?);
            return Ok(empty(if p4 { "FAILED" } else { "NOT_READY" }, issues, d));
        }
    };
    let stored = job::stored_after(conn, job_id)?;
    let rows_ok = stored.len() == p.after_by_row.len()
        && stored
            .iter()
            .all(|(row, v)| v.is_some() && p.after_by_row.get(row).copied() == *v);
    let event = job::last_event(conn, job_id, "ALLOCATE")?;
    let fp_ok = event.as_deref().and_then(fp_of) == Some(p.fingerprint.as_str());
    if !(rows_ok && fp_ok) {
        return Ok(empty("STALE", vec![issue(code::STALE, None, None)], p.priority_digest));
    }
    Ok(Allocation {
        job_id,
        job_status: head.status,
        status: "DONE",
        issues: Vec::new(),
        session: None,
        students: p.students,
        before: p.before,
        after: p.after,
        settlement: p.settlement,
        applied_grades: grades.clone(),
        priority_digest: p.priority_digest,
        fingerprint: Some(p.fingerprint),
    })
}

/// 처리 이력의 배분 결과가 지금 유효하지 않으면 쓰지 못하게 하는 오류.
pub fn require_valid(conn: &Connection, job_id: i64, bytes: &[u8]) -> AppResult<Allocation> {
    let a = verify(conn, job_id, bytes)?;
    if a.status != "DONE" {
        return Err(AppError::new(
            "ALLOCATION_STALE",
            "배분 결과가 지금 자료 · 우선순위와 맞지 않습니다. [배분 계산]을 다시 해 주세요.",
        ));
    }
    Ok(a)
}
