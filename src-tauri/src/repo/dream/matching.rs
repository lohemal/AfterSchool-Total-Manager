//! 방과후드림 결제파일 ↔ 통합 매니저 — 학생 · 강좌 · 항목 · 금액 정확 매칭 (P3).
//!
//! 목표는 "이 행의 금액이 정확히 이 학생 · 이 부서 · 이 비용항목에서 생긴 금액이고, 그
//! 학생의 파일 전체 금액이 정산 총액과 같다"를 **기존 자료로 증명하는 것**이다.
//! 지원금을 배분하지 않고, 아무것도 고치지 않는다(이 모듈은 읽기만 한다).
//!
//! ## 학생 — 찾기와 교차검증을 나눈다
//!
//! ```text
//! 찾기    개인참조번호(C) → HMAC 찾기 값으로 정확히 한 학생
//!         등록 전이면 같은 학년도 · 학년 · 반(원래 값 또는 끝 '반' 한 번 뗀 값) · 번호 ·
//!         이름이 모두 **정확히** 같은 학생만 후보
//! 검증    개인참조번호로 찾았어도 학년 · 반 · 번호 · 이름 · 생년월일을 다시 맞대어 본다
//! ```
//!
//! 이름 유사도 · 부분 일치 · 공백 제거 같은 짐작은 하지 않는다.
//!
//! ## 강좌 — 확정된 강좌코드 연결만 믿는다
//!
//! `K 강좌명코드 → dream_course_map → 부서`. 강좌명(J)은 그 부서의 화면 이름
//! (`부서명+반 이름`)과 정확히 같아야 하고, 연결이 없으면 후보를 **제안만** 한다.
//!
//! ## 금액 — 정산에 들어간 Charge 와 줄마다, 학생마다
//!
//! * 줄마다 `N == 그 학생 · 부서 · 항목의 Charge`. Charge 행이 없으면 0원이다 — 정산은
//!   `amount > 0` 인 Charge 만 쓰므로 행이 없는 것과 0원은 정산에서 같다.
//! * 정산에 들어간 양수 Charge 가 파일에 없으면 오류(지원금이 다른 줄로 밀리는 것을 막는다).
//! * 같은 학생 · 부서 · 항목이 두 줄이면 더하지 않고 오류.
//! * `ΣN == T`(최신 유효 정산의 그 학생 배분 합).
//!
//! ## 학생 단위로 한꺼번에
//!
//! 한 학생의 줄 가운데 하나라도 오류 · 확인 필요면 그 학생 전체가 준비 완료가 아니다.
//! 파일 안의 모든 학생이 준비 완료여야 다음 단계(배분)로 갈 수 있다.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::crypto::DataKey;
use crate::domain::dream::{self as rules, ITEMS};
use crate::domain::dream_file::{DreamFile, DreamRow};
use crate::error::AppResult;
use crate::repo;
use crate::repo::dream::amount::{self, FreshSettlement, StudentAmount};
use crate::repo::dream::{course, identity};
use crate::repo::enrollment::{dept_label, student_label};

/// 학생 · 강좌 · 줄의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    Ready,
    NeedsConfirmation,
    Error,
}

fn worst(states: impl Iterator<Item = State>) -> State {
    states.max().unwrap_or(State::Ready)
}

/// 까닭 하나. `detail` 에는 개인정보가 아닌 것(부서 · 항목 · 금액 · 코드)만 넣는다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reason {
    pub code: &'static str,
    pub state: State,
    pub row: Option<u32>,
    pub detail: Option<String>,
}

fn err(code: &'static str) -> Reason {
    Reason {
        code,
        state: State::Error,
        row: None,
        detail: None,
    }
}

fn confirm(code: &'static str) -> Reason {
    Reason {
        state: State::NeedsConfirmation,
        ..err(code)
    }
}

impl Reason {
    fn row(mut self, r: u32) -> Self {
        self.row = Some(r);
        self
    }
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
}

/// 까닭 코드.
pub mod code {
    pub const SETTLEMENT_NOT_FRESH: &str = "SETTLEMENT_NOT_FRESH";
    pub const BUDGET_CROSSCHECK_FAILED: &str = "BUDGET_CROSSCHECK_FAILED";
    pub const SETTLED_STUDENT_NOT_IN_FILE: &str = "SETTLED_STUDENT_NOT_IN_FILE";
    // 학생
    pub const STUDENT_INFO_INCONSISTENT: &str = "STUDENT_INFO_INCONSISTENT";
    pub const STUDENT_REF_NOT_REGISTERED: &str = "STUDENT_REF_NOT_REGISTERED";
    pub const STUDENT_REF_CONFLICT: &str = "STUDENT_REF_CONFLICT";
    pub const STUDENT_NOT_FOUND: &str = "STUDENT_NOT_FOUND";
    pub const STUDENT_AMBIGUOUS: &str = "STUDENT_AMBIGUOUS";
    pub const STUDENT_DUPLICATE_IN_FILE: &str = "STUDENT_DUPLICATE_IN_FILE";
    pub const STUDENT_GRADE_MISMATCH: &str = "STUDENT_GRADE_MISMATCH";
    pub const STUDENT_CLASS_MISMATCH: &str = "STUDENT_CLASS_MISMATCH";
    pub const STUDENT_NO_MISMATCH: &str = "STUDENT_NO_MISMATCH";
    pub const STUDENT_NAME_MISMATCH: &str = "STUDENT_NAME_MISMATCH";
    pub const STUDENT_BIRTH_MISMATCH: &str = "STUDENT_BIRTH_MISMATCH";
    pub const STUDENT_BIRTH_NOT_REGISTERED: &str = "STUDENT_BIRTH_NOT_REGISTERED";
    // 강좌
    pub const COURSE_MAP_MISSING: &str = "COURSE_MAP_MISSING";
    pub const COURSE_NAME_MISMATCH: &str = "COURSE_NAME_MISMATCH";
    pub const COURSE_NAME_INCONSISTENT: &str = "COURSE_NAME_INCONSISTENT";
    pub const ENROLLMENT_NOT_FOUND: &str = "ENROLLMENT_NOT_FOUND";
    pub const ENROLLMENT_AMBIGUOUS: &str = "ENROLLMENT_AMBIGUOUS";
    // 항목 · 금액
    pub const ITEM_UNKNOWN: &str = "ITEM_UNKNOWN";
    pub const DUPLICATE_PAYMENT_ROW: &str = "DUPLICATE_PAYMENT_ROW";
    pub const CHARGE_MISSING: &str = "CHARGE_MISSING";
    pub const CHARGE_AMOUNT_MISMATCH: &str = "CHARGE_AMOUNT_MISMATCH";
    pub const DB_CHARGE_NOT_IN_FILE: &str = "DB_CHARGE_NOT_IN_FILE";
    pub const STUDENT_TOTAL_MISMATCH: &str = "STUDENT_TOTAL_MISMATCH";
    pub const CHARGE_SETTLEMENT_MISMATCH: &str = "CHARGE_SETTLEMENT_MISMATCH";
    // 학생이 아직 확정되지 않아 줄을 끝까지 볼 수 없다
    pub const STUDENT_NOT_CONFIRMED: &str = "STUDENT_NOT_CONFIRMED";
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub student_id: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub department_id: i64,
    pub label: String,
    /// `SAME_NAME` | `PREVIOUS_WORKSPACE` | `STUDENT_ENROLLED`
    pub why: Vec<&'static str>,
    /// 다른 강좌코드에 이미 연결된 부서면 고를 수 없다
    pub taken: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseMatch {
    pub course_code: String,
    pub course_names: Vec<String>,
    pub department_id: Option<i64>,
    pub dept_label: Option<String>,
    pub state: State,
    pub reasons: Vec<Reason>,
    pub suggestions: Vec<Suggestion>,
    /// 직전 작업공간에서의 연결(작업공간 이름, 부서 이름)
    pub previous: Option<(String, String)>,
    pub rows: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    pub source_row: u32,
    pub course_code: String,
    pub department_id: Option<i64>,
    pub dept_label: Option<String>,
    pub item_code: Option<&'static str>,
    pub item_name: String,
    pub amount_n: i64,
    /// 정산에 쓰인 Charge(행이 없으면 Some(0)). 견줄 수 없었으면 None
    pub charge: Option<i64>,
    pub enrollment_id: Option<i64>,
    pub state: State,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingCharge {
    pub department_id: i64,
    pub dept_label: String,
    pub item_name: String,
    pub amount: i64,
}

/// 파일 속 학생 한 명(개인참조번호가 같은 줄들).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentMatch {
    /// 이 학생의 첫 줄 번호 — 확인 · 등록 요청에 쓰는 열쇠
    pub key: u32,
    pub rows: Vec<u32>,
    pub file_name: String,
    pub file_grade: String,
    pub file_class: String,
    pub file_no: String,
    /// 가린 값
    pub ref_masked: String,
    pub birth_masked: String,
    pub student_id: Option<i64>,
    pub student_label: Option<String>,
    /// `MATCHED` | `NOT_REGISTERED` | `CONFLICT` | `UNAVAILABLE`
    pub ref_state: &'static str,
    /// `MATCHED` | `NOT_REGISTERED` | `MISMATCH` | `UNCHECKED`
    pub birth_state: &'static str,
    pub candidates: Vec<Candidate>,
    pub lines: Vec<LineMatch>,
    pub missing: Vec<MissingCharge>,
    pub sum_n: i64,
    pub before: (i64, i64, i64),
    pub amount: Option<StudentAmount>,
    pub state: State,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Department {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Matching {
    pub settlement_id: Option<i64>,
    pub file_reasons: Vec<Reason>,
    pub courses: Vec<CourseMatch>,
    pub students: Vec<StudentMatch>,
    /// 강좌 연결을 고를 때 쓰는 이 작업공간 부서 목록
    pub departments: Vec<Department>,
    pub ready: usize,
    pub needs_confirmation: usize,
    pub errors: usize,
    /// 모든 학생이 준비 완료이고 파일 · 정산 문제가 없는가 — 배분 단계로 갈 수 있는가
    pub p4_ready: bool,
}

// ─────────────────────────────────────────────── 읽기

struct DbStudent {
    id: i64,
    grade: i64,
    class_no: String,
    student_no: i64,
    name: String,
}

fn db_student(conn: &Connection, id: i64) -> AppResult<Option<(i64, DbStudent)>> {
    Ok(conn
        .query_row(
            "SELECT year_id, grade, class_no, student_no, name FROM student WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    DbStudent {
                        id,
                        grade: r.get(1)?,
                        class_no: r.get(2)?,
                        student_no: r.get(3)?,
                        name: r.get(4)?,
                    },
                ))
            },
        )
        .optional()?)
}

fn label(s: &DbStudent) -> String {
    student_label(s.grade, &s.class_no, s.student_no, &s.name)
}

/// 같은 학년도에서 학년 · 반(원래 값 또는 후보) · 번호 · 이름이 모두 정확히 같은 학생.
fn exact_candidates(conn: &Connection, year_id: i64, r: &DreamRow) -> AppResult<Vec<DbStudent>> {
    let (Some(grade), Some(no)) = (r.grade_candidate, r.student_no) else {
        return Ok(Vec::new());
    };
    let alt = r.class_candidate.clone().unwrap_or_else(|| r.class_raw.clone());
    let mut st = conn.prepare(
        "SELECT id, grade, class_no, student_no, name FROM student
          WHERE year_id = ?1 AND grade = ?2 AND student_no = ?3 AND name = ?4
            AND (class_no = ?5 OR class_no = ?6)
          ORDER BY id",
    )?;
    let rows = st
        .query_map(params![year_id, grade, no, r.student_name, r.class_raw, alt], |x| {
            Ok(DbStudent {
                id: x.get(0)?,
                grade: x.get(1)?,
                class_no: x.get(2)?,
                student_no: x.get(3)?,
                name: x.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 학생의 이 작업공간 수강과 Charge — 부서마다.
struct Enr {
    id: i64,
    status: String,
    charges: HashMap<String, i64>,
}

fn enrollments(conn: &Connection, ws: i64, student_id: i64) -> AppResult<BTreeMap<i64, Vec<Enr>>> {
    let mut st = conn.prepare(
        "SELECT e.id, e.department_id, e.status, c.item_code, c.amount
           FROM enrollment e LEFT JOIN charge c ON c.enrollment_id = e.id
          WHERE e.workspace_id = ?1 AND e.student_id = ?2
          ORDER BY e.department_id, e.id",
    )?;
    let mut out: BTreeMap<i64, Vec<Enr>> = BTreeMap::new();
    let rows = st
        .query_map(params![ws, student_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (eid, dept, status, item, amt) in rows {
        let list = out.entry(dept).or_default();
        if list.last().map(|e| e.id) != Some(eid) {
            list.push(Enr {
                id: eid,
                status,
                charges: HashMap::new(),
            });
        }
        if let (Some(i), Some(a)) = (item, amt) {
            list.last_mut().expect("방금 넣었다").charges.insert(i, a);
        }
    }
    Ok(out)
}

/// 한 부서의 수강이 여럿일 때 정산에 쓰인 하나를 고른다. 고를 수 없으면 Err(까닭).
///
/// 정산 대상은 상태가 아니라 금액으로 정한다(`settle::SETTLE_TARGET` — 취소 수강이어도
/// 양수 Charge 가 있으면 정산된다). 양수 Charge 가 있는 수강이 하나면 그것, 하나도 없으면
/// 수강 중인 것 하나. 둘 이상이면 고르지 않는다(임의로 고르거나 더하지 않는다).
fn pick_enrollment(list: &[Enr]) -> Result<&Enr, &'static str> {
    let positive: Vec<&Enr> = list.iter().filter(|e| e.charges.values().any(|a| *a > 0)).collect();
    match positive.len() {
        1 => Ok(positive[0]),
        0 => {
            let active: Vec<&Enr> = list.iter().filter(|e| e.status == "ACTIVE").collect();
            match (active.len(), list.len()) {
                (1, _) => Ok(active[0]),
                (0, 1) => Ok(&list[0]),
                _ => Err(code::ENROLLMENT_AMBIGUOUS),
            }
        }
        _ => Err(code::ENROLLMENT_AMBIGUOUS),
    }
}

fn item_name(code: &str) -> String {
    ITEMS
        .iter()
        .find(|(_, _, c)| *c == code)
        .map(|(_, n, _)| n.to_string())
        .unwrap_or_else(|| code.to_string())
}

// ─────────────────────────────────────────────── 강좌

fn match_courses(conn: &Connection, ws: i64, file: &DreamFile) -> AppResult<Vec<CourseMatch>> {
    let mut by_code: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    for r in &file.rows {
        let Some(code) = &r.course_code else { continue };
        let e = by_code.entry(code.clone()).or_default();
        e.0.insert(r.course_name.clone());
        e.1 += 1;
    }
    let ws_row = repo::year::get_workspace(conn, ws)?;
    let before = repo::year::workspaces_before(conn, &ws_row)?;
    let mapped: HashMap<i64, String> = course::list(conn, ws)?
        .into_iter()
        .map(|m| (m.department_id, m.course_code))
        .collect();
    let depts = departments(conn, ws)?;

    let mut out = Vec::new();
    for (code_v, (names, rows)) in by_code {
        let names: Vec<String> = names.into_iter().collect();
        let mut m = CourseMatch {
            course_code: code_v.clone(),
            course_names: names.clone(),
            department_id: None,
            dept_label: None,
            state: State::Ready,
            reasons: Vec::new(),
            suggestions: Vec::new(),
            previous: None,
            rows,
        };
        if names.len() > 1 {
            m.reasons.push(err(code::COURSE_NAME_INCONSISTENT).detail(names.join(", ")));
        }
        match course::find(conn, ws, &code_v)? {
            Some(dept) => {
                let lbl = depts.iter().find(|d| d.id == dept).map(|d| d.label.clone()).unwrap_or_default();
                m.department_id = Some(dept);
                m.dept_label = Some(lbl.clone());
                // 강좌명은 부서 화면 이름(부서명+반 이름)과 정확히 같아야 한다
                if names.len() == 1 && names[0] != lbl {
                    m.reasons.push(err(code::COURSE_NAME_MISMATCH).detail(format!("{} ≠ {lbl}", names[0])));
                }
            }
            None => {
                m.reasons.push(confirm(code::COURSE_MAP_MISSING));
                let mut sug: BTreeMap<i64, Vec<&'static str>> = BTreeMap::new();
                for d in &depts {
                    if names.contains(&d.label) {
                        sug.entry(d.id).or_default().push("SAME_NAME");
                    }
                }
                // 직전 작업공간(가장 가까운 것부터)에서 같은 코드가 이었던 부서와 이름 · 반이 같은 부서
                for prev in before.iter().rev() {
                    let Some(pd) = course::find(conn, prev.id, &code_v)? else { continue };
                    let (pn, pc) = repo::department::name_of(conn, pd)?;
                    m.previous = Some((prev.name.clone(), dept_label(&pn, &pc)));
                    let same: Vec<i64> = conn
                        .prepare("SELECT id FROM department WHERE workspace_id = ?1 AND name = ?2 AND class_name = ?3")?
                        .query_map(params![ws, pn, pc], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    for id in same {
                        sug.entry(id).or_default().push("PREVIOUS_WORKSPACE");
                    }
                    break;
                }
                m.suggestions = sug
                    .into_iter()
                    .map(|(id, why)| Suggestion {
                        department_id: id,
                        label: depts.iter().find(|d| d.id == id).map(|d| d.label.clone()).unwrap_or_default(),
                        why,
                        taken: mapped.get(&id).is_some_and(|c| *c != code_v),
                    })
                    .collect();
            }
        }
        m.state = worst(m.reasons.iter().map(|r| r.state));
        out.push(m);
    }
    Ok(out)
}

fn departments(conn: &Connection, ws: i64) -> AppResult<Vec<Department>> {
    let mut st = conn.prepare(
        "SELECT id, name, class_name FROM department WHERE workspace_id = ?1 ORDER BY name, class_name, id",
    )?;
    let rows = st
        .query_map(params![ws], |r| {
            Ok(Department {
                id: r.get(0)?,
                label: dept_label(&r.get::<_, String>(1)?, &r.get::<_, String>(2)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ─────────────────────────────────────────────── 학생

/// 학생 한 명의 파일 줄들을 개인참조번호로 묶는다(파일 차례). 첫 줄 번호가 열쇠다.
fn groups(file: &DreamFile) -> Vec<Vec<&DreamRow>> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<&DreamRow>> = HashMap::new();
    for r in &file.rows {
        let k = r.personal_ref.as_ref().map(|s| s.expose().to_string()).unwrap_or_default();
        if !map.contains_key(&k) {
            order.push(k.clone());
        }
        map.entry(k).or_default().push(r);
    }
    order.into_iter().filter_map(|k| map.remove(&k)).collect()
}

/// 학생 하나 — 찾고, 교차검증하고, 줄 · 금액을 맞대어 본다.
#[allow(clippy::too_many_arguments)]
fn match_student(
    conn: &Connection,
    ws: i64,
    year_id: i64,
    key: Option<&DataKey>,
    rows: &[&DreamRow],
    courses: &[CourseMatch],
    amounts: &HashMap<i64, StudentAmount>,
) -> AppResult<StudentMatch> {
    let first = rows[0];
    let mut m = StudentMatch {
        key: first.source_row,
        rows: rows.iter().map(|r| r.source_row).collect(),
        file_name: first.student_name.clone(),
        file_grade: first.grade_raw.clone(),
        file_class: first.class_raw.clone(),
        file_no: first.student_no_raw.clone(),
        ref_masked: first.display.get(2).cloned().unwrap_or_default(),
        birth_masked: first.display.get(7).cloned().unwrap_or_default(),
        student_id: None,
        student_label: None,
        ref_state: "NOT_REGISTERED",
        birth_state: "UNCHECKED",
        candidates: Vec::new(),
        lines: Vec::new(),
        missing: Vec::new(),
        sum_n: rows.iter().filter_map(|r| r.amount_n).sum(),
        before: rows.iter().fold((0, 0, 0), |a, r| {
            (
                a.0 + r.personal_before.unwrap_or(0),
                a.1 + r.voucher_before.unwrap_or(0),
                a.2 + r.free_voucher_before.unwrap_or(0),
            )
        }),
        amount: None,
        state: State::Ready,
        reasons: Vec::new(),
    };

    // 같은 개인참조번호의 줄은 학생 정보가 모두 같아야 한다
    let same = |r: &&DreamRow| {
        r.grade_raw == first.grade_raw
            && r.class_raw == first.class_raw
            && r.student_no_raw == first.student_no_raw
            && r.student_name == first.student_name
            && r.birth.as_ref().map(|b| b.expose()) == first.birth.as_ref().map(|b| b.expose())
    };
    if !rows.iter().all(same) {
        m.reasons.push(err(code::STUDENT_INFO_INCONSISTENT));
    }

    // 1) 찾기
    let ref_value = first.personal_ref.as_ref().map(|s| s.expose().to_string()).unwrap_or_default();
    let by_ref = match key {
        Some(k) => identity::find_by_ref(conn, k, year_id, &ref_value)?,
        None => None,
    };
    // 학생 : 확정(개인참조번호로) / 후보만 / 없음
    let mut tentative: Option<DbStudent> = None;
    match by_ref {
        Some(sid) => {
            m.ref_state = "MATCHED";
            let Some((sy, s)) = db_student(conn, sid)? else {
                return Err(crate::error::AppError::new("INTERNAL", "개인참조번호가 없는 학생에게 등록되어 있습니다."));
            };
            // 2) 교차검증 — 개인참조번호가 맞아도 나머지가 다르면 정상으로 두지 않는다
            if sy != year_id {
                m.reasons.push(err(code::STUDENT_REF_CONFLICT));
            }
            if first.grade_candidate != Some(s.grade) {
                m.reasons.push(err(code::STUDENT_GRADE_MISMATCH));
            }
            let class_ok = s.class_no == first.class_raw || first.class_candidate.as_deref() == Some(s.class_no.as_str());
            if !class_ok {
                m.reasons.push(err(code::STUDENT_CLASS_MISMATCH));
            }
            if first.student_no != Some(s.student_no) {
                m.reasons.push(err(code::STUDENT_NO_MISMATCH));
            }
            if first.student_name != s.name {
                m.reasons.push(err(code::STUDENT_NAME_MISMATCH));
            }
            let file_birth = first.birth.as_ref().map(|b| b.expose().to_string()).unwrap_or_default();
            match identity::reveal_birth(conn, key.expect("찾았으면 키가 있다"), sid)? {
                Some(stored) => {
                    if rules::birth_matches(&stored, &file_birth) {
                        m.birth_state = "MATCHED";
                    } else {
                        m.birth_state = "MISMATCH";
                        m.reasons.push(err(code::STUDENT_BIRTH_MISMATCH));
                    }
                }
                None => {
                    m.birth_state = "NOT_REGISTERED";
                    m.reasons.push(confirm(code::STUDENT_BIRTH_NOT_REGISTERED));
                }
            }
            m.student_id = Some(s.id);
            m.student_label = Some(label(&s));
            tentative = Some(s);
        }
        None => {
            let cands = exact_candidates(conn, year_id, first)?;
            m.candidates = cands
                .iter()
                .map(|c| Candidate {
                    student_id: c.id,
                    label: label(c),
                })
                .collect();
            match cands.len() {
                0 => m.reasons.push(err(code::STUDENT_NOT_FOUND)),
                1 => {
                    let c = cands.into_iter().next().expect("하나");
                    if identity::state(conn, c.id)?.has_ref {
                        // 그 학생에게는 이미 다른 개인참조번호가 있다 — 옮기지 않는다
                        m.ref_state = "CONFLICT";
                        m.reasons.push(err(code::STUDENT_REF_CONFLICT));
                    } else {
                        m.reasons.push(confirm(code::STUDENT_REF_NOT_REGISTERED));
                    }
                    m.student_label = Some(label(&c));
                    tentative = Some(c); // 확인 전이지만 금액은 미리 맞대어 본다
                }
                _ => m.reasons.push(confirm(code::STUDENT_AMBIGUOUS)),
            }
        }
    }

    // 3) 줄 · 금액 — 학생(확정 또는 후보 하나)이 있을 때만
    if let Some(s) = &tentative {
        check_lines(conn, ws, s.id, rows, courses, amounts, &mut m)?;
    } else {
        for r in rows {
            m.lines.push(LineMatch {
                source_row: r.source_row,
                course_code: r.course_code.clone().unwrap_or_default(),
                department_id: None,
                dept_label: None,
                item_code: r.item_code,
                item_name: r.item_name.clone(),
                amount_n: r.amount_n.unwrap_or(0),
                charge: None,
                enrollment_id: None,
                state: State::NeedsConfirmation,
                reasons: vec![confirm(code::STUDENT_NOT_CONFIRMED).row(r.source_row)],
            });
        }
    }
    m.state = worst(m.reasons.iter().map(|r| r.state).chain(m.lines.iter().map(|l| l.state)));
    Ok(m)
}

fn check_lines(
    conn: &Connection,
    ws: i64,
    student_id: i64,
    rows: &[&DreamRow],
    courses: &[CourseMatch],
    amounts: &HashMap<i64, StudentAmount>,
    m: &mut StudentMatch,
) -> AppResult<()> {
    let enr = enrollments(conn, ws, student_id)?;
    let mut seen: HashMap<(i64, &'static str), u32> = HashMap::new();
    let mut all_resolved = true;

    for r in rows {
        let code_v = r.course_code.clone().unwrap_or_default();
        let course = courses.iter().find(|c| c.course_code == code_v);
        let mut line = LineMatch {
            source_row: r.source_row,
            course_code: code_v.clone(),
            department_id: course.and_then(|c| c.department_id),
            dept_label: course.and_then(|c| c.dept_label.clone()),
            item_code: r.item_code,
            item_name: r.item_name.clone(),
            amount_n: r.amount_n.unwrap_or(0),
            charge: None,
            enrollment_id: None,
            state: State::Ready,
            reasons: Vec::new(),
        };
        let item = r.item_code;
        match (course, item) {
            (Some(c), Some(item)) if c.state == State::Ready => {
                let dept = c.department_id.expect("준비된 강좌는 부서가 있다");
                if let Some(prev) = seen.insert((dept, item), r.source_row) {
                    line.reasons.push(err(code::DUPLICATE_PAYMENT_ROW).row(r.source_row).detail(format!("{prev}행과 같음")));
                }
                match enr.get(&dept).map(|l| pick_enrollment(l)) {
                    None => line.reasons.push(err(code::ENROLLMENT_NOT_FOUND).row(r.source_row)),
                    Some(Err(why)) => line.reasons.push(err(why).row(r.source_row)),
                    Some(Ok(e)) => {
                        line.enrollment_id = Some(e.id);
                        // Charge 행이 없으면 0원 — 정산도 그렇게 본다
                        let db = e.charges.get(item).copied().unwrap_or(0);
                        line.charge = Some(db);
                        if db != line.amount_n {
                            let code_x = if db == 0 { code::CHARGE_MISSING } else { code::CHARGE_AMOUNT_MISMATCH };
                            line.reasons.push(err(code_x).row(r.source_row).detail(format!("파일 {} · 정산 {}", line.amount_n, db)));
                        }
                    }
                }
            }
            (_, None) => line.reasons.push(err(code::ITEM_UNKNOWN).row(r.source_row)),
            (Some(c), _) => {
                all_resolved = false;
                let st = if c.state == State::Error { State::Error } else { State::NeedsConfirmation };
                line.reasons.push(Reason {
                    code: c.reasons.first().map(|x| x.code).unwrap_or(code::COURSE_MAP_MISSING),
                    state: st,
                    row: Some(r.source_row),
                    detail: Some(code_v.clone()),
                });
            }
            (None, _) => {
                all_resolved = false;
                line.reasons.push(err(code::COURSE_MAP_MISSING).row(r.source_row));
            }
        }
        line.state = worst(line.reasons.iter().map(|x| x.state));
        m.lines.push(line);
    }

    // 정산에 들어간 양수 Charge 가 파일에 빠지지 않았는가 — 모든 줄의 강좌가 정해졌을 때만 본다
    let db_positive: i64 = enr
        .values()
        .flatten()
        .flat_map(|e| e.charges.values())
        .filter(|a| **a > 0)
        .sum();
    if all_resolved {
        for (dept, list) in &enr {
            for e in list {
                for (item, amt) in &e.charges {
                    if *amt <= 0 {
                        continue;
                    }
                    let Some(item_static) = ITEMS.iter().map(|(_, _, c)| *c).find(|c| c == item) else {
                        continue;
                    };
                    if !seen.contains_key(&(*dept, item_static)) {
                        let (n, c) = repo::department::name_of(conn, *dept)?;
                        let lbl = dept_label(&n, &c);
                        m.missing.push(MissingCharge {
                            department_id: *dept,
                            dept_label: lbl.clone(),
                            item_name: item_name(item),
                            amount: *amt,
                        });
                        m.reasons.push(err(code::DB_CHARGE_NOT_IN_FILE).detail(format!("{lbl} {} {amt}", item_name(item))));
                    }
                }
            }
        }
    }

    // ΣN == T, 그리고 정산에 쓰인 Charge 합 == T
    let a = amounts.get(&student_id).cloned();
    let t = a.as_ref().map(|x| x.total).unwrap_or(0);
    if all_resolved && m.sum_n != t {
        m.reasons.push(err(code::STUDENT_TOTAL_MISMATCH).detail(format!("파일 {} · 정산 {t}", m.sum_n)));
    }
    if db_positive != t {
        m.reasons.push(err(code::CHARGE_SETTLEMENT_MISMATCH).detail(format!("Charge {db_positive} · 정산 {t}")));
    }
    m.amount = Some(a.unwrap_or(StudentAmount {
        student_id,
        total: 0,
        voucher: 0,
        free_voucher: 0,
        personal: 0,
        self_pay: 0,
        voucher_over: 0,
    }));
    Ok(())
}

// ─────────────────────────────────────────────── 파일 전체

/// 파일 전체를 맞대어 본다. 읽기만 한다. `key` 는 키가 아직 없으면 None(그러면 아무도
/// 개인참조번호로 찾을 수 없다 — 모두 후보 검색으로 간다).
pub fn run(conn: &Connection, workspace_id: i64, file: &DreamFile, key: Option<&DataKey>) -> AppResult<Matching> {
    let mut out = Matching {
        settlement_id: None,
        file_reasons: Vec::new(),
        courses: Vec::new(),
        students: Vec::new(),
        departments: departments(conn, workspace_id)?,
        ready: 0,
        needs_confirmation: 0,
        errors: 0,
        p4_ready: false,
    };

    // 정산이 최신이어야 하고, 한도 기록과도 맞아야 한다
    let fresh: FreshSettlement = match amount::fresh_settlement(conn, workspace_id) {
        Ok(f) => f,
        Err(e) => {
            out.file_reasons.push(err(code::SETTLEMENT_NOT_FRESH).detail(e.message));
            return Ok(out);
        }
    };
    out.settlement_id = Some(fresh.settlement_id);
    let amounts: HashMap<i64, StudentAmount> = match amount::workspace_amounts(conn, workspace_id) {
        Ok((_, list)) => list.into_iter().map(|a| (a.student_id, a)).collect(),
        Err(e) => {
            out.file_reasons.push(err(code::BUDGET_CROSSCHECK_FAILED).detail(e.code));
            return Ok(out);
        }
    };
    let year_id = repo::year::get_workspace(conn, workspace_id)?.year_id;

    out.courses = match_courses(conn, workspace_id, file)?;
    for g in groups(file) {
        out.students.push(match_student(conn, workspace_id, year_id, key, &g, &out.courses, &amounts)?);
    }

    // 강좌 제안 — 후보 학생들이 실제로 수강하는 부서
    let mut enrolled: HashMap<String, BTreeSet<i64>> = HashMap::new();
    for s in &out.students {
        let Some(sid) = s.student_id.or_else(|| (s.candidates.len() == 1).then(|| s.candidates[0].student_id)) else {
            continue;
        };
        let depts: Vec<i64> = conn
            .prepare("SELECT DISTINCT department_id FROM enrollment WHERE workspace_id = ?1 AND student_id = ?2")?
            .query_map(params![workspace_id, sid], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for l in &s.lines {
            enrolled.entry(l.course_code.clone()).or_default().extend(depts.iter().copied());
        }
    }
    let taken: HashMap<i64, String> = course::list(conn, workspace_id)?
        .into_iter()
        .map(|m| (m.department_id, m.course_code))
        .collect();
    for c in out.courses.iter_mut().filter(|c| c.department_id.is_none()) {
        if let Some(depts) = enrolled.get(&c.course_code) {
            for d in depts {
                match c.suggestions.iter_mut().find(|s| s.department_id == *d) {
                    Some(sg) => sg.why.push("STUDENT_ENROLLED"),
                    None => {
                        if let Some(dd) = out.departments.iter().find(|x| x.id == *d) {
                            c.suggestions.push(Suggestion {
                                department_id: *d,
                                label: dd.label.clone(),
                                why: vec!["STUDENT_ENROLLED"],
                                taken: taken.get(d).is_some_and(|x| *x != c.course_code),
                            });
                        }
                    }
                }
            }
        }
        // 근거가 많은 것부터
        c.suggestions.sort_by(|a, b| b.why.len().cmp(&a.why.len()).then(a.label.cmp(&b.label)));
    }

    // 같은 학생이 파일에 두 번(다른 개인참조번호로) 나오면 둘 다 오류
    let mut owners: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, s) in out.students.iter().enumerate() {
        if let Some(id) = s.student_id {
            owners.entry(id).or_default().push(i);
        }
    }
    for idx in owners.values().filter(|v| v.len() > 1) {
        for i in idx {
            let s = &mut out.students[*i];
            s.reasons.push(err(code::STUDENT_DUPLICATE_IN_FILE));
            s.state = State::Error;
        }
    }

    // 정산에는 있는데 파일에 없는 학생 — 참고(이 파일로 그 학생 결제가 나가지 않는다)
    let in_file: BTreeSet<i64> = out
        .students
        .iter()
        .filter_map(|s| s.student_id.or_else(|| (s.candidates.len() == 1).then(|| s.candidates[0].student_id)))
        .collect();
    let absent = amounts.values().filter(|a| a.total > 0 && !in_file.contains(&a.student_id)).count();
    if absent > 0 {
        out.file_reasons.push(Reason {
            code: code::SETTLED_STUDENT_NOT_IN_FILE,
            state: State::Ready,
            row: None,
            detail: Some(absent.to_string()),
        });
    }

    out.ready = out.students.iter().filter(|s| s.state == State::Ready).count();
    out.needs_confirmation = out.students.iter().filter(|s| s.state == State::NeedsConfirmation).count();
    out.errors = out.students.iter().filter(|s| s.state == State::Error).count();
    let file_ok = out.file_reasons.iter().all(|r| r.state == State::Ready);
    out.p4_ready = file_ok && !out.students.is_empty() && out.ready == out.students.len();
    Ok(out)
}
