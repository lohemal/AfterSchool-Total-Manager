//! 추가징수 · 환불 기록 (v0.1.5, 설계안 26장).
//!
//! ## 수강생 명단과 다른 자료다
//!
//! 수강생 명단은 **지금 이 학생이 얼마를 내는가**를 관리한다. 여기는 **이미 한
//! 번 걷은 뒤에 얼마를 더 걷거나 돌려주어야 하는가**를 관리한다. 그래서 금액을
//! `charge` 에서 그때그때 읽어 오지 않고 만든 시점의 값으로 굳혀 둔다.
//!
//! ## 확인 필요는 깃발이 아니라 값 비교로 정한다
//!
//! 금액을 고치는 경로는 여럿이다 — 학생별 수정, 취소, 부서 기준금액 재반영,
//! Excel 일괄수정. 그 모두가 깃발을 세우도록 만들면 한 곳만 빠져도 조용히
//! 어긋나고, 같은 값을 다시 쓰는 것(no-op)까지 확인 필요로 잡힌다.
//!
//! 그래서 **마지막으로 사람이 확인한 그때의 charge**(`checked_charge`)를 적어
//! 두고, 읽을 때 지금 charge 와 견준다.
//!
//! ```text
//! 확인 필요  ⇔  charge.amount ≠ checked_charge   (항목별)
//! ```
//!
//! 값을 견주므로 no-op 은 애초에 잡히지 않고, 고치는 경로가 늘어도 손댈 것이
//! 없다. [기존 금액 유지]는 `checked_charge` 만 지금 값으로 옮기고, [현재 금액
//! 반영]은 `amount` 까지 옮긴다. 그 뒤 또 바뀌면 다시 달라지므로 새로운 확인
//! 필요가 된다.
//!
//! ## 정산을 낡게 만들지 않는다
//!
//! 이 표에는 자료판 트리거가 없다. 추가징수·환불 기록을 만드는 것은 정산의
//! 원본을 바꾸는 일이 아니기 때문이다.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::error::{AppError, AppResult};
use crate::model::{
    Adjustment, AdjustmentDiff, AdjustmentFee, AdjustmentFilter, AdjustmentInput,
    AdjustmentReport, AdjustmentView, CostItem, Enrollment, Fee, StudentSumRow,
};
use crate::repo;
use crate::repo::enrollment::{dept_label, student_label, won};

pub const ADDITIONAL: &str = "ADDITIONAL_CHARGE";
pub const REFUND: &str = "REFUND";

/// `기존 금액 유지` / `현재 금액 반영`.
pub const KEEP: &str = "KEEP";
pub const APPLY: &str = "APPLY";

/// 오늘 날짜를 SQLite 에게 묻는다.
///
/// 프로그램 안의 다른 시각(`created_at` 등)이 모두 SQLite 의 지역시각이므로
/// 여기서도 같은 시계를 쓴다. 두 시계를 섞으면 자정 무렵에 날짜가 어긋난다.
fn today_of(conn: &Connection) -> AppResult<String> {
    Ok(conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?)
}

/// 발생일을 다듬는다. 비어 있으면 오늘.
fn occurred_of(conn: &Connection, input: &AdjustmentInput) -> AppResult<String> {
    let raw = input
        .occurred_on
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match raw {
        None => today_of(conn),
        Some(v) => {
            if v.len() != 10 || v.as_bytes()[4] != b'-' || v.as_bytes()[7] != b'-' {
                return Err(AppError::invalid("발생일은 2026-09-22 꼴이어야 합니다."));
            }
            Ok(v.to_string())
        }
    }
}

// ─────────────────────────────────────────────── 만들기

/// 추가징수 — 지금 확정된 charge 를 그대로 굳혀 둔다.
///
/// 수강 건을 만드는 것과 **같은 트랜잭션**에서 불러야 한다. 부르는 쪽
/// (`commands::enrollment`)이 `Db::write` 하나로 감싼다.
pub fn create_additional(
    conn: &Connection,
    enrollment_id: i64,
    input: &AdjustmentInput,
    items: &[CostItem],
) -> AppResult<i64> {
    let e = repo::enrollment::get(conn, enrollment_id, items)?;
    let occurred = occurred_of(conn, input)?;
    let id = insert(conn, &e, ADDITIONAL, &occurred, input)?;

    for it in items {
        let amount = fee_of(&e.fees, &it.code);
        conn.execute(
            "INSERT INTO billing_adjustment_amount
                 (adjustment_id, item_code, amount, base_amount, checked_charge)
             VALUES (?1, ?2, ?3, 0, ?3)",
            params![id, it.code, amount],
        )?;
    }
    Ok(id)
}

/// 환불 — `취소 직전 금액 - 취소 후 최종 금액`.
///
/// `before` 는 **취소하기 직전**의 수강 건이다. 취소가 끝난 뒤에는 그 값을
/// 다시 구할 수 없으므로 부르는 쪽이 미리 떠서 넘긴다.
///
/// 어느 항목이든 환불액이 음수가 되면 **만들지 않고 오류**다. 업무적으로
/// 확인이 필요한 상태를 0원으로 뭉개면 그 사실이 사라진다.
pub fn create_refund(
    conn: &Connection,
    enrollment_id: i64,
    before: &Enrollment,
    input: &AdjustmentInput,
    items: &[CostItem],
) -> AppResult<i64> {
    let after = repo::enrollment::get(conn, enrollment_id, items)?;
    let occurred = occurred_of(conn, input)?;

    // 먼저 전부 따져 본다 — 한 항목이라도 음수면 아무것도 만들지 않는다.
    let mut cells: Vec<(String, i64, i64)> = Vec::new();
    for it in items {
        let base = fee_of(&before.fees, &it.code);
        let now = fee_of(&after.fees, &it.code);
        let refund = base - now;
        if refund < 0 {
            return Err(AppError::invalid(format!(
                "{}의 취소 후 금액({}원)이 취소 전({}원)보다 많아 환불액이 음수가 됩니다. \
                 금액을 확인하시거나 [환불 대상] 체크를 풀고 취소해 주세요.",
                it.name,
                won(now),
                won(base)
            )));
        }
        cells.push((it.code.clone(), refund, base));
    }

    let id = insert(conn, &after, REFUND, &occurred, input)?;
    for (code, refund, base) in cells {
        let now = fee_of(&after.fees, &code);
        conn.execute(
            "INSERT INTO billing_adjustment_amount
                 (adjustment_id, item_code, amount, base_amount, checked_charge)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, code, refund, base, now],
        )?;
    }
    Ok(id)
}

fn insert(
    conn: &Connection,
    e: &Enrollment,
    kind: &str,
    occurred_on: &str,
    input: &AdjustmentInput,
) -> AppResult<i64> {
    let ws: i64 = conn.query_row(
        "SELECT workspace_id FROM enrollment WHERE id = ?1",
        params![e.id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO billing_adjustment
             (workspace_id, student_id, enrollment_id, department_id, kind,
              occurred_on, note, student_label, dept_label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            ws,
            e.student_id,
            e.id,
            e.department_id,
            kind,
            occurred_on,
            input.note.as_deref().unwrap_or("").trim(),
            student_label(e.grade, &e.class_no, e.student_no, &e.name),
            e.dept_label,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn fee_of(fees: &[Fee], code: &str) -> i64 {
    fees.iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap_or(0)
}

// ─────────────────────────────────────────────── 읽기

struct Raw {
    id: i64,
    kind: String,
    student_id: i64,
    enrollment_id: i64,
    department_id: i64,
    grade: i64,
    class_no: String,
    student_no: i64,
    name: String,
    dept_label: String,
    status: String,
    student_label_at: String,
    dept_label_at: String,
    occurred_on: String,
    note: String,
    created_at: String,
}

const SELECT: &str = "
SELECT a.id, a.kind, a.student_id, a.enrollment_id, a.department_id,
       s.grade, s.class_no, s.student_no, s.name,
       d.name, d.class_name, e.status,
       a.student_label, a.dept_label, a.occurred_on, a.note, a.created_at
  FROM billing_adjustment a
  JOIN student s    ON s.id = a.student_id
  JOIN department d ON d.id = a.department_id
  JOIN enrollment e ON e.id = a.enrollment_id";

const ORDER: &str = "
 ORDER BY s.grade, s.class_sort, s.class_no, s.student_no, s.name,
          d.name, d.class_name, a.occurred_on, a.id";

/// 항목별 금액을 조정 id 로 묶어 읽는다. 지금 charge 도 함께 가져온다.
fn amounts_of(
    conn: &Connection,
    workspace_id: i64,
) -> AppResult<HashMap<i64, Vec<(String, i64, i64, i64, i64)>>> {
    let mut st = conn.prepare(
        "SELECT m.adjustment_id, m.item_code, m.amount, m.base_amount, m.checked_charge,
                COALESCE(c.amount, 0)
           FROM billing_adjustment_amount m
           JOIN billing_adjustment a ON a.id = m.adjustment_id
           LEFT JOIN charge c
                  ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
          WHERE a.workspace_id = ?1",
    )?;
    let mut out: HashMap<i64, Vec<(String, i64, i64, i64, i64)>> = HashMap::new();
    for row in st.query_map(params![workspace_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })? {
        let (aid, code, amount, base, checked, current) = row?;
        out.entry(aid)
            .or_default()
            .push((code, amount, base, checked, current));
    }
    Ok(out)
}

fn map_raw(r: &rusqlite::Row) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        kind: r.get(1)?,
        student_id: r.get(2)?,
        enrollment_id: r.get(3)?,
        department_id: r.get(4)?,
        grade: r.get(5)?,
        class_no: r.get(6)?,
        student_no: r.get(7)?,
        name: r.get(8)?,
        dept_label: dept_label(&r.get::<_, String>(9)?, &r.get::<_, String>(10)?),
        status: r.get(11)?,
        student_label_at: r.get(12)?,
        dept_label_at: r.get(13)?,
        occurred_on: r.get(14)?,
        note: r.get(15)?,
        created_at: r.get(16)?,
    })
}

/// 조건에 맞는 조정을 종류별로 읽는다.
///
/// **부서는 학생을 찾는 조건이다** — 학생별 징수 내역과 같은 원칙이다. 기간·
/// 학년·반·이름은 그대로 거른다(학생의 성질이거나, 기간처럼 본래 범위를
/// 정하는 조건이므로 합계의 뜻이 흔들리지 않는다).
fn load(
    conn: &Connection,
    workspace_id: i64,
    kind: &str,
    f: &AdjustmentFilter,
    items: &[CostItem],
) -> AppResult<Vec<Adjustment>> {
    let mut sql = format!("{SELECT} WHERE a.workspace_id = ?1 AND a.kind = ?2");
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(workspace_id), Box::new(kind.to_string())];

    let push = |sql: &mut String, args: &mut Vec<Box<dyn ToSql>>, frag: &str, v: Box<dyn ToSql>| {
        args.push(v);
        sql.push_str(&frag.replace("?N", &format!("?{}", args.len())));
    };

    if let Some(v) = f.from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        push(&mut sql, &mut args, " AND a.occurred_on >= ?N", Box::new(v.to_string()));
    }
    if let Some(v) = f.to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        push(&mut sql, &mut args, " AND a.occurred_on <= ?N", Box::new(v.to_string()));
    }
    if let Some(g) = f.grade {
        push(&mut sql, &mut args, " AND s.grade = ?N", Box::new(g));
    }
    if let Some(c) = f
        .class_no
        .as_deref()
        .map(crate::domain::class_no::normalize)
        .filter(|s| !s.is_empty())
    {
        push(&mut sql, &mut args, " AND s.class_no = ?N", Box::new(c));
    }
    if let Some(q) = f.query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        args.push(Box::new(format!("%{q}%")));
        let i = args.len();
        sql.push_str(&format!(
            " AND (s.name LIKE ?{i} OR d.name LIKE ?{i} OR d.class_name LIKE ?{i})"
        ));
    }
    sql.push_str(ORDER);

    let mut st = conn.prepare(&sql)?;
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let raws = st
        .query_map(refs.as_slice(), map_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    // 부서 필터 — 그 부서의 조정이 있는 **학생**을 고르고, 그 학생의 줄은
    // 부서를 가리지 않고 모두 남긴다.
    let raws: Vec<Raw> = match f.department_id {
        Some(d) => {
            let ids: std::collections::HashSet<i64> = raws
                .iter()
                .filter(|r| r.department_id == d)
                .map(|r| r.student_id)
                .collect();
            raws.into_iter()
                .filter(|r| ids.contains(&r.student_id))
                .collect()
        }
        None => raws,
    };

    let amounts = amounts_of(conn, workspace_id)?;
    let name_of: HashMap<&str, &str> = items
        .iter()
        .map(|i| (i.code.as_str(), i.name.as_str()))
        .collect();

    Ok(raws
        .into_iter()
        .map(|r| {
            let mine = amounts.get(&r.id).cloned().unwrap_or_default();
            let by_code: HashMap<&str, &(String, i64, i64, i64, i64)> =
                mine.iter().map(|x| (x.0.as_str(), x)).collect();
            let fees: Vec<AdjustmentFee> = items
                .iter()
                .map(|it| {
                    let v = by_code.get(it.code.as_str());
                    AdjustmentFee {
                        item_code: it.code.clone(),
                        item_name: name_of.get(it.code.as_str()).copied().unwrap_or("").to_string(),
                        amount: v.map(|x| x.1).unwrap_or(0),
                        base_amount: v.map(|x| x.2).unwrap_or(0),
                        checked_charge: v.map(|x| x.3).unwrap_or(0),
                        current_charge: v.map(|x| x.4).unwrap_or(0),
                    }
                })
                .collect();
            let total = fees.iter().map(|f| f.amount).sum();
            let needs_check = fees.iter().any(|f| f.checked_charge != f.current_charge);
            Adjustment {
                id: r.id,
                kind: r.kind,
                student_id: r.student_id,
                enrollment_id: r.enrollment_id,
                department_id: r.department_id,
                grade: r.grade,
                class_no: r.class_no,
                student_no: r.student_no,
                name: r.name,
                dept_label: r.dept_label,
                enrollment_status: r.status,
                student_label_at: r.student_label_at,
                dept_label_at: r.dept_label_at,
                occurred_on: r.occurred_on,
                note: r.note,
                fees,
                total,
                needs_check,
                created_at: r.created_at,
            }
        })
        .collect())
}

/// 상세를 학생 단위로 모은다 — 학생별 징수 내역과 같은 모양.
fn summarize(details: Vec<Adjustment>, items: &[CostItem]) -> AdjustmentReport {
    let mut order: Vec<i64> = Vec::new();
    let mut rows: HashMap<i64, StudentSumRow> = HashMap::new();
    let mut per_student: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut grand: HashMap<String, i64> = HashMap::new();

    for a in &details {
        if !rows.contains_key(&a.student_id) {
            order.push(a.student_id);
            rows.insert(
                a.student_id,
                StudentSumRow {
                    student_id: a.student_id,
                    grade: a.grade,
                    class_no: a.class_no.clone(),
                    student_no: a.student_no,
                    name: a.name.clone(),
                    programs: Vec::new(),
                    fees: Vec::new(),
                    total: 0,
                    details: 0,
                },
            );
        }
        let row = rows.get_mut(&a.student_id).expect("방금 넣었다");
        row.details += 1;
        let mine = per_student.entry(a.student_id).or_default();
        for f in &a.fees {
            *mine.entry(f.item_code.clone()).or_insert(0) += f.amount;
            *grand.entry(f.item_code.clone()).or_insert(0) += f.amount;
        }
    }

    let sum = |map: &HashMap<String, i64>| -> (Vec<Fee>, i64) {
        let fees: Vec<Fee> = items
            .iter()
            .map(|it| Fee {
                item_code: it.code.clone(),
                amount: map.get(&it.code).copied().unwrap_or(0),
            })
            .collect();
        let total = fees.iter().map(|f| f.amount).sum();
        (fees, total)
    };

    let out: Vec<StudentSumRow> = order
        .into_iter()
        .filter_map(|id| {
            let mut row = rows.remove(&id)?;
            let empty = HashMap::new();
            let (fees, total) = sum(per_student.get(&id).unwrap_or(&empty));
            row.fees = fees;
            row.total = total;
            Some(row)
        })
        .collect();

    let (fees, total) = sum(&grand);
    AdjustmentReport {
        students: out.len() as i64,
        count: details.len() as i64,
        needs_check: details.iter().filter(|a| a.needs_check).count() as i64,
        rows: out,
        details,
        fees,
        total,
    }
}

pub fn view(
    conn: &Connection,
    workspace_id: i64,
    filter: &AdjustmentFilter,
    items: &[CostItem],
) -> AppResult<AdjustmentView> {
    Ok(AdjustmentView {
        additional: summarize(load(conn, workspace_id, ADDITIONAL, filter, items)?, items),
        refund: summarize(load(conn, workspace_id, REFUND, filter, items)?, items),
        needs_check_all: needs_check_count(conn, workspace_id)?,
    })
}

/// 작업공간 전체에서 확인이 필요한 조정 건수 — 사이드바 배지에 쓴다.
///
/// 화면 필터와 무관하다. 걸러 놓은 바깥에 확인할 것이 있는데 배지가 0이면
/// 그것을 영영 못 본다.
pub fn needs_check_count(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT a.id)
           FROM billing_adjustment a
           JOIN billing_adjustment_amount m ON m.adjustment_id = a.id
           LEFT JOIN charge c
                  ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
          WHERE a.workspace_id = ?1
            AND m.checked_charge <> COALESCE(c.amount, 0)",
        params![workspace_id],
        |r| r.get(0),
    )?)
}

/// 원본이 달라진 칸만 모아 준다 — [변경내역 확인] 화면이 쓴다.
pub fn diffs(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
) -> AppResult<Vec<AdjustmentDiff>> {
    let mut out = Vec::new();
    for kind in [ADDITIONAL, REFUND] {
        for a in load(conn, workspace_id, kind, &AdjustmentFilter::default(), items)? {
            if !a.needs_check {
                continue;
            }
            for f in &a.fees {
                if f.checked_charge == f.current_charge {
                    continue;
                }
                let suggested = if a.kind == REFUND {
                    f.base_amount - f.current_charge
                } else {
                    f.current_charge
                };
                out.push(AdjustmentDiff {
                    adjustment_id: a.id,
                    kind: a.kind.clone(),
                    student_label: student_label(a.grade, &a.class_no, a.student_no, &a.name),
                    dept_label: a.dept_label.clone(),
                    occurred_on: a.occurred_on.clone(),
                    item_code: f.item_code.clone(),
                    item_name: f.item_name.clone(),
                    saved: f.amount,
                    suggested: suggested.max(0),
                    checked_charge: f.checked_charge,
                    current_charge: f.current_charge,
                    base_amount: f.base_amount,
                    negative: suggested < 0,
                });
            }
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────── 확인

/// 변경내역을 확인 처리한다.
///
/// * `KEEP`  — 금액은 그대로 두고 확인만 한다. 이미 그 금액으로 안내했을 때.
/// * `APPLY` — 지금 원본을 기준으로 금액을 다시 잡는다.
///
/// 어느 쪽이든 `checked_charge` 를 지금 값으로 옮기므로 경고가 사라진다.
/// 그 뒤 원본이 또 바뀌면 다시 달라져 새로운 확인 필요가 된다.
pub fn confirm(conn: &Connection, ids: &[i64], mode: &str) -> AppResult<i64> {
    if mode != KEEP && mode != APPLY {
        return Err(AppError::invalid("알 수 없는 확인 방식입니다."));
    }
    let mut done = 0;
    for id in ids {
        let kind: String = conn
            .query_row(
                "SELECT kind FROM billing_adjustment WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("기록을 찾지 못했습니다."))?;

        if mode == APPLY {
            // 음수가 생기는 환불은 반영하지 않는다 — 확인이 필요한 상태다.
            let bad: i64 = conn.query_row(
                "SELECT COUNT(*)
                   FROM billing_adjustment_amount m
                   JOIN billing_adjustment a ON a.id = m.adjustment_id
                   LEFT JOIN charge c
                          ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
                  WHERE m.adjustment_id = ?1
                    AND a.kind = 'REFUND'
                    AND m.base_amount - COALESCE(c.amount, 0) < 0",
                params![id],
                |r| r.get(0),
            )?;
            if bad > 0 {
                return Err(AppError::invalid(
                    "취소 후 금액이 취소 직전보다 많아 환불액이 음수가 됩니다. \
                     수강생 명단에서 금액을 먼저 확인해 주세요.",
                ));
            }

            if kind == REFUND {
                conn.execute(
                    "UPDATE billing_adjustment_amount AS m
                        SET amount = m.base_amount - COALESCE(
                              (SELECT c.amount FROM charge c
                                JOIN billing_adjustment a ON a.id = m.adjustment_id
                               WHERE c.enrollment_id = a.enrollment_id
                                 AND c.item_code = m.item_code), 0)
                      WHERE m.adjustment_id = ?1",
                    params![id],
                )?;
            } else {
                conn.execute(
                    "UPDATE billing_adjustment_amount AS m
                        SET amount = COALESCE(
                              (SELECT c.amount FROM charge c
                                JOIN billing_adjustment a ON a.id = m.adjustment_id
                               WHERE c.enrollment_id = a.enrollment_id
                                 AND c.item_code = m.item_code), 0)
                      WHERE m.adjustment_id = ?1",
                    params![id],
                )?;
            }
        }

        // 어느 쪽이든 '여기까지 확인했다'를 남긴다.
        conn.execute(
            "UPDATE billing_adjustment_amount AS m
                SET checked_charge = COALESCE(
                      (SELECT c.amount FROM charge c
                        JOIN billing_adjustment a ON a.id = m.adjustment_id
                       WHERE c.enrollment_id = a.enrollment_id
                         AND c.item_code = m.item_code), 0)
              WHERE m.adjustment_id = ?1",
            params![id],
        )?;
        conn.execute(
            "UPDATE billing_adjustment
                SET updated_at = datetime('now', 'localtime') WHERE id = ?1",
            params![id],
        )?;
        done += 1;
    }
    Ok(done)
}

/// 기록을 지운다. 사람이 고른 것만 지운다.
pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM billing_adjustment WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

// ─────────────────────────────────────────────── 지우기를 막는다

/// 이 조건에 걸리는 조정이 있으면 사람이 읽을 수 있는 까닭으로 막는다.
///
/// `ON DELETE RESTRICT` 가 마지막 방어선이지만, 그것만 두면 "FOREIGN KEY
/// constraint failed" 라는 말만 보게 된다.
fn block(conn: &Connection, sql: &str, args: &[&dyn ToSql], what: &str) -> AppResult<()> {
    let n: i64 = conn.query_row(sql, args, |r| r.get(0))?;
    if n > 0 {
        return Err(AppError::new(
            "ADJUSTMENT_EXISTS",
            format!(
                "{what}에 딸린 추가징수·환불 기록이 {n}건 있어 지울 수 없습니다. \
                 [수강 관리 › 추가·취소 관리]에서 그 기록을 먼저 지워 주세요."
            ),
        ));
    }
    Ok(())
}

pub fn block_for_students(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    for id in ids {
        block(
            conn,
            "SELECT COUNT(*) FROM billing_adjustment WHERE student_id = ?1",
            &[id],
            "학생",
        )?;
    }
    Ok(())
}

pub fn block_for_year(conn: &Connection, year_id: i64) -> AppResult<()> {
    block(
        conn,
        "SELECT COUNT(*) FROM billing_adjustment a
           JOIN student s ON s.id = a.student_id
          WHERE s.year_id = ?1",
        &[&year_id],
        "이 학년도",
    )
}

pub fn block_for_departments(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    for id in ids {
        block(
            conn,
            "SELECT COUNT(*) FROM billing_adjustment WHERE department_id = ?1",
            &[id],
            "부서",
        )?;
    }
    Ok(())
}

pub fn block_for_workspace(conn: &Connection, workspace_id: i64) -> AppResult<()> {
    block(
        conn,
        "SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1",
        &[&workspace_id],
        "작업공간",
    )
}
