//! 학생별 징수 내역 집계 (v0.1.4).
//!
//! ## 이 파일이 하는 일과 하지 않는 일
//!
//! * **한다** — 원본 `charge`를 학생 단위로 모으고, 화면·Excel이 함께 쓸
//!   항목별 총합까지 만든다.
//! * **하지 않는다** — 정산 배분(`settlement_alloc`)을 보지 않는다. 금액을
//!   다시 계산하지도 않는다. Excel writer는 이 결과를 받아 쓰기만 한다.
//!
//! ## 필터는 학생을 찾는 조건이다
//!
//! 이 메뉴의 목적은 **한 학생에게 모두 얼마를 징수하는가**다. 그래서 부서나
//! 수강상태로 걸러도 그것은 *학생을 고르는* 조건이고, 금액은 찾은 학생의
//! **전체** 합계다. 로봇과학으로 걸러 나온 학생의 줄에는 그 학생이 듣는 미술·
//! 축구 금액까지 들어 있다.
//!
//! 그러므로 상세(`details`)도 걸러진 부서만 담아서는 안 된다. 찾은 학생의
//! 모든 수강 건을 담는다 — 그러지 않으면 목록 합계와 상세 합계가 어긋난다.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::error::AppResult;
use crate::model::{CostItem, EnrollmentFilter, Fee, FeeReport, StudentSumRow};
use crate::repo;

/// 걸러 내는 조건이 하나라도 있는가.
///
/// 없으면 학생을 찾는 질의와 상세 질의가 같은 것이므로 한 번만 읽는다.
fn narrows(f: &EnrollmentFilter) -> bool {
    f.department_id.is_some()
        || f.grade.is_some()
        || f.class_no.as_deref().is_some_and(|s| !s.trim().is_empty())
        || f.status.as_deref().is_some_and(|s| !s.is_empty())
        || f.query.as_deref().is_some_and(|s| !s.trim().is_empty())
        || f.program.as_deref().is_some_and(|s| !s.is_empty())
}

/// 항목 차례대로의 합계와 총액.
fn sum_fees(items: &[CostItem], map: &HashMap<String, i64>) -> (Vec<Fee>, i64) {
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

/// 학생별 징수 내역을 만든다.
pub fn build(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
    filter: &EnrollmentFilter,
) -> AppResult<FeeReport> {
    // 1) 조건에 맞는 **학생**을 찾는다
    let matched = repo::enrollment::list_by_student(conn, workspace_id, items, filter)?;

    // 2) 그 학생들의 **모든** 수강 건을 상세로 삼는다
    let details = if narrows(filter) {
        let ids: HashSet<i64> = matched.iter().map(|e| e.student_id).collect();
        if ids.is_empty() {
            Vec::new()
        } else {
            repo::enrollment::list_by_student(conn, workspace_id, items, &EnrollmentFilter::default())?
                .into_iter()
                .filter(|e| ids.contains(&e.student_id))
                .collect()
        }
    } else {
        matched
    };

    // 3) 학생 단위로 모은다. 차례는 상세가 이미 학생 우선이라 그대로 따른다.
    let mut order: Vec<i64> = Vec::new();
    let mut rows: HashMap<i64, StudentSumRow> = HashMap::new();
    let mut per_student: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut grand: HashMap<String, i64> = HashMap::new();

    for e in &details {
        if !rows.contains_key(&e.student_id) {
            order.push(e.student_id);
            rows.insert(
                e.student_id,
                StudentSumRow {
                    student_id: e.student_id,
                    grade: e.grade,
                    class_no: e.class_no.clone(),
                    student_no: e.student_no,
                    name: e.name.clone(),
                    programs: e.programs.clone(),
                    fees: Vec::new(),
                    total: 0,
                    details: 0,
                },
            );
        }
        let row = rows.get_mut(&e.student_id).expect("방금 넣었다");
        row.details += 1;
        let mine = per_student.entry(e.student_id).or_default();
        for f in &e.fees {
            *mine.entry(f.item_code.clone()).or_insert(0) += f.amount;
            *grand.entry(f.item_code.clone()).or_insert(0) += f.amount;
        }
    }

    let out: Vec<StudentSumRow> = order
        .into_iter()
        .filter_map(|id| {
            let mut row = rows.remove(&id)?;
            let empty = HashMap::new();
            let (fees, total) = sum_fees(items, per_student.get(&id).unwrap_or(&empty));
            row.fees = fees;
            row.total = total;
            Some(row)
        })
        .collect();

    let (fees, total) = sum_fees(items, &grand);
    Ok(FeeReport {
        students: out.len() as i64,
        enrollments: details.len() as i64,
        rows: out,
        details,
        fees,
        total,
    })
}
