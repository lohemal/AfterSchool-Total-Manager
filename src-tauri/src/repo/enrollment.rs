//! 수강 · 청구액 (Phase 2).
//!
//! 이 파일이 지키는 두 가지 원칙
//!
//! 1. **부서 기준금액을 고쳐도 이미 등록된 학생의 `charge`는 그대로 둔다** (§42-3).
//!    반영은 사람이 [부서금액 반영]을 눌렀을 때만 일어나고, 그때도 무엇이 바뀌는지
//!    먼저 보여 준다.
//! 2. **수강 취소는 행을 지우지 않는다** (§42-4). `status`만 `CANCELLED`로 바꾸고
//!    사유를 받는다. 부분 유니크 인덱스 덕분에 취소 후 다시 등록할 수 있다.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::domain::{eligibility_active, grade_matches, parse_grades};
use crate::error::{AppError, AppResult};
use crate::model::{
    ApplyResult, CostItem, Enrollment, EnrollmentFilter, EnrollmentInput, Fee, FeeDiff, FeePick,
    Student, StudentDetail, SupportView, WorkspaceEnrollments,
};
use crate::repo::change_log as log;

/// Excel 업로드 한 줄 (검증을 마친 상태).
#[derive(Debug, Clone)]
pub struct EnrollmentRow {
    pub student_id: i64,
    pub department_id: i64,
}

/// 학생별 수정 팝업에서 저장할 한 학생분.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentFeeEdit {
    pub enrollment_id: i64,
    pub fees: Vec<Fee>,
}

// ─────────────────────────────────────────────── 표시용 문자열

/// `40000` → `40,000`. 변경이력 문구에 쓴다.
pub fn won(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

pub fn dept_label(name: &str, class_name: &str) -> String {
    if class_name.trim().is_empty() {
        name.to_string()
    } else {
        format!("{name}{class_name}")
    }
}

pub fn student_label(grade: i64, class_no: i64, student_no: i64, name: &str) -> String {
    format!("{grade}학년 {class_no}반 {student_no}번 {name}")
}

/// `강사료 40,000 · 수용비 0 · 교재비 30,000 · 재료비 0` — 이력의 이전값/변경값.
fn fees_text(items: &[CostItem], fees: &[Fee]) -> String {
    items
        .iter()
        .map(|it| {
            let amount = fees
                .iter()
                .find(|f| f.item_code == it.code)
                .map(|f| f.amount)
                .unwrap_or(0);
            format!("{} {}", it.name, won(amount))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ─────────────────────────────────────────────── 조회

struct Raw {
    id: i64,
    workspace_id: i64,
    student_id: i64,
    department_id: i64,
    grade: i64,
    class_no: i64,
    student_no: i64,
    name: String,
    dept_name: String,
    dept_class_name: String,
    status: String,
    change_reason: String,
    updated_at: String,
}

const RAW_SELECT: &str = "
SELECT e.id, e.workspace_id, e.student_id, e.department_id,
       s.grade, s.class_no, s.student_no, s.name,
       d.name, d.class_name,
       e.status, e.change_reason, e.updated_at
FROM enrollment e
JOIN student s    ON s.id = e.student_id
JOIN department d ON d.id = e.department_id";

const RAW_ORDER: &str = " ORDER BY d.name, d.class_name, s.grade, s.class_no, s.student_no";

fn map_raw(r: &rusqlite::Row) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        student_id: r.get(2)?,
        department_id: r.get(3)?,
        grade: r.get(4)?,
        class_no: r.get(5)?,
        student_no: r.get(6)?,
        name: r.get(7)?,
        dept_name: r.get(8)?,
        dept_class_name: r.get(9)?,
        status: r.get(10)?,
        change_reason: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

/// 여러 수강 건의 청구액을 한 번에 읽는다 (수강 수만큼 질의하지 않는다).
///
/// 돌려주는 둘째 값은 '학생별로 고친 금액이 하나라도 있는 수강'의 표시다.
#[allow(clippy::type_complexity)]
fn charges_of(
    conn: &Connection,
    ids: &[i64],
    items: &[CostItem],
) -> AppResult<(HashMap<i64, Vec<Fee>>, HashMap<i64, bool>)> {
    let mut out: HashMap<i64, Vec<Fee>> = HashMap::new();
    if ids.is_empty() {
        return Ok((out, HashMap::new()));
    }
    let list = ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT enrollment_id, item_code, amount, is_overridden
           FROM charge WHERE enrollment_id IN ({list})"
    );
    let mut st = conn.prepare(&sql)?;
    let mut over: HashMap<i64, bool> = HashMap::new();
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? == 1,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (eid, code, amount, is_over) in rows {
        out.entry(eid).or_default().push(Fee {
            item_code: code,
            amount,
        });
        if is_over {
            over.insert(eid, true);
        }
    }
    // 비용항목 차례대로 정렬하고, 없는 항목은 0원으로 채운다.
    for fees in out.values_mut() {
        let mut sorted = Vec::with_capacity(items.len());
        for it in items {
            let amount = fees
                .iter()
                .find(|f| f.item_code == it.code)
                .map(|f| f.amount)
                .unwrap_or(0);
            sorted.push(Fee {
                item_code: it.code.clone(),
                amount,
            });
        }
        *fees = sorted;
    }
    Ok((out, over))
}

/// 학년도의 지원자격을 한 번에 읽어 (학생 → [제도, 시작, 끝]) 로 만든다.
fn eligibility_map(
    conn: &Connection,
    year_id: i64,
) -> AppResult<HashMap<i64, Vec<(String, Option<String>, Option<String>)>>> {
    let mut st = conn.prepare(
        "SELECT student_id, program, valid_from, valid_to
           FROM support_eligibility WHERE year_id = ?1",
    )?;
    let mut out: HashMap<i64, Vec<(String, Option<String>, Option<String>)>> = HashMap::new();
    let rows = st
        .query_map(params![year_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (sid, program, from, to) in rows {
        out.entry(sid).or_default().push((program, from, to));
    }
    Ok(out)
}

/// 그 작업공간 기간에 유효한 제도만 뽑는다.
fn programs_at(
    elig: &HashMap<i64, Vec<(String, Option<String>, Option<String>)>>,
    student_id: i64,
    start: &str,
    end: &str,
) -> Vec<String> {
    let mut out: Vec<String> = elig
        .get(&student_id)
        .map(|rows| {
            rows.iter()
                .filter(|(_, f, t)| eligibility_active(f.as_deref(), t.as_deref(), start, end))
                .map(|(p, _, _)| p.clone())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn enrich(
    conn: &Connection,
    raws: Vec<Raw>,
    items: &[CostItem],
    year_id: i64,
    ws_dates: &HashMap<i64, (String, String)>,
) -> AppResult<Vec<Enrollment>> {
    let ids: Vec<i64> = raws.iter().map(|r| r.id).collect();
    let (charges, overrides) = charges_of(conn, &ids, items)?;
    let elig = eligibility_map(conn, year_id)?;

    let empty = (String::new(), String::new());
    Ok(raws
        .into_iter()
        .map(|r| {
            let fees = charges.get(&r.id).cloned().unwrap_or_else(|| {
                items
                    .iter()
                    .map(|it| Fee {
                        item_code: it.code.clone(),
                        amount: 0,
                    })
                    .collect()
            });
            let total = fees.iter().map(|f| f.amount).sum();
            let (start, end) = ws_dates.get(&r.workspace_id).unwrap_or(&empty);
            Enrollment {
                id: r.id,
                student_id: r.student_id,
                department_id: r.department_id,
                grade: r.grade,
                class_no: r.class_no,
                student_no: r.student_no,
                name: r.name,
                programs: programs_at(&elig, r.student_id, start, end),
                dept_label: dept_label(&r.dept_name, &r.dept_class_name),
                dept_name: r.dept_name,
                dept_class_name: r.dept_class_name,
                fees,
                total,
                has_override: overrides.get(&r.id).copied().unwrap_or(false),
                status: r.status,
                change_reason: r.change_reason,
                updated_at: r.updated_at,
            }
        })
        .collect())
}

fn workspace_dates(conn: &Connection, year_id: i64) -> AppResult<HashMap<i64, (String, String)>> {
    let mut st =
        conn.prepare("SELECT id, start_date, end_date FROM workspace WHERE year_id = ?1")?;
    let rows = st
        .query_map(params![year_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn year_of_workspace(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    conn.query_row(
        "SELECT year_id FROM workspace WHERE id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("작업공간을 찾지 못했습니다."))
}

/// 수강생 명단. 실제 수강 자료가 있는 학생만 나온다 (요구사항 §5).
pub fn list(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
    f: &EnrollmentFilter,
) -> AppResult<Vec<Enrollment>> {
    let year_id = year_of_workspace(conn, workspace_id)?;

    let mut sql = format!("{RAW_SELECT} WHERE e.workspace_id = ?1");
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(workspace_id)];

    if let Some(d) = f.department_id {
        args.push(Box::new(d));
        sql.push_str(&format!(" AND e.department_id = ?{}", args.len()));
    }
    if let Some(g) = f.grade {
        args.push(Box::new(g));
        sql.push_str(&format!(" AND s.grade = ?{}", args.len()));
    }
    if let Some(c) = f.class_no {
        args.push(Box::new(c));
        sql.push_str(&format!(" AND s.class_no = ?{}", args.len()));
    }
    if let Some(st) = f.status.as_deref().filter(|s| !s.is_empty()) {
        args.push(Box::new(st.to_string()));
        sql.push_str(&format!(" AND e.status = ?{}", args.len()));
    }
    if let Some(q) = f.query.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        args.push(Box::new(format!("%{q}%")));
        let i = args.len();
        sql.push_str(&format!(
            " AND (s.name LIKE ?{i} OR d.name LIKE ?{i} OR d.class_name LIKE ?{i})"
        ));
    }
    sql.push_str(RAW_ORDER);

    let mut st = conn.prepare(&sql)?;
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let raws = st
        .query_map(refs.as_slice(), map_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let ws = workspace_dates(conn, year_id)?;
    let rows = enrich(conn, raws, items, year_id, &ws)?;

    // 지원유형 필터는 자격을 계산한 뒤에 거른다.
    Ok(match f.program.as_deref() {
        Some("VOUCHER") => rows
            .into_iter()
            .filter(|r| r.programs.iter().any(|p| p == "VOUCHER"))
            .collect(),
        Some("FREE_VOUCHER") => rows
            .into_iter()
            .filter(|r| r.programs.iter().any(|p| p == "FREE_VOUCHER"))
            .collect(),
        Some("BOTH") => rows.into_iter().filter(|r| r.programs.len() >= 2).collect(),
        Some("NONE") => rows.into_iter().filter(|r| r.programs.is_empty()).collect(),
        _ => rows,
    })
}

pub fn get(conn: &Connection, id: i64, items: &[CostItem]) -> AppResult<Enrollment> {
    let sql = format!("{RAW_SELECT} WHERE e.id = ?1");
    let raw = conn
        .query_row(&sql, params![id], map_raw)
        .optional()?
        .ok_or_else(|| AppError::not_found("수강 자료를 찾지 못했습니다."))?;
    let year_id = year_of_workspace(conn, raw.workspace_id)?;
    let ws = workspace_dates(conn, year_id)?;
    let mut rows = enrich(conn, vec![raw], items, year_id, &ws)?;
    rows.pop()
        .ok_or_else(|| AppError::not_found("수강 자료를 찾지 못했습니다."))
}

/// 한 부서의 수강생 (학생별 금액 수정 팝업).
pub fn by_department(
    conn: &Connection,
    workspace_id: i64,
    department_id: i64,
    items: &[CostItem],
) -> AppResult<Vec<Enrollment>> {
    list(
        conn,
        workspace_id,
        items,
        &EnrollmentFilter {
            department_id: Some(department_id),
            status: Some("ACTIVE".into()),
            ..Default::default()
        },
    )
}

// ─────────────────────────────────────────────── 부서 기준금액

/// 부서에 설정된 기준 수강료. 없는 항목은 0원.
fn base_fees(conn: &Connection, department_id: i64, items: &[CostItem]) -> AppResult<Vec<Fee>> {
    let mut st =
        conn.prepare("SELECT item_code, amount FROM department_fee WHERE department_id = ?1")?;
    let found: HashMap<String, i64> = st
        .query_map(params![department_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect();
    Ok(items
        .iter()
        .map(|it| Fee {
            item_code: it.code.clone(),
            amount: found.get(&it.code).copied().unwrap_or(0),
        })
        .collect())
}

/// 청구액을 쓴다. 부서 기준과 다르면 `is_overridden = 1`로 표시한다.
fn write_charges(
    conn: &Connection,
    enrollment_id: i64,
    fees: &[Fee],
    base: &[Fee],
) -> AppResult<()> {
    for f in fees {
        if f.amount < 0 {
            return Err(AppError::invalid("금액은 0원 이상이어야 합니다."));
        }
        let base_amount = base
            .iter()
            .find(|b| b.item_code == f.item_code)
            .map(|b| b.amount)
            .unwrap_or(0);
        let overridden = i64::from(f.amount != base_amount);
        conn.execute(
            "INSERT INTO charge (enrollment_id, item_code, amount, is_overridden)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(enrollment_id, item_code)
               DO UPDATE SET amount = excluded.amount, is_overridden = excluded.is_overridden",
            params![enrollment_id, f.item_code, f.amount, overridden],
        )?;
    }
    Ok(())
}

// ─────────────────────────────────────────────── 추가 · 수정 · 취소

struct Ctx {
    year_id: i64,
    student_label: String,
    dept_label: String,
    student_id: i64,
    department_id: i64,
}

fn ctx_of(conn: &Connection, workspace_id: i64, student_id: i64, department_id: i64) -> AppResult<Ctx> {
    let year_id = year_of_workspace(conn, workspace_id)?;
    let (grade, class_no, student_no, name): (i64, i64, i64, String) = conn
        .query_row(
            "SELECT grade, class_no, student_no, name FROM student WHERE id = ?1",
            params![student_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("학생을 찾지 못했습니다."))?;
    let (dname, dclass): (String, String) = conn
        .query_row(
            "SELECT name, class_name FROM department WHERE id = ?1",
            params![department_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("부서를 찾지 못했습니다."))?;
    Ok(Ctx {
        year_id,
        student_label: student_label(grade, class_no, student_no, &name),
        dept_label: dept_label(&dname, &dclass),
        student_id,
        department_id,
    })
}

pub fn create(
    conn: &Connection,
    workspace_id: i64,
    input: &EnrollmentInput,
    items: &[CostItem],
) -> AppResult<i64> {
    // 부서가 이 작업공간의 것인지 확인한다 — 다른 기간의 부서에 넣으면 안 된다.
    let dept_ws: i64 = conn
        .query_row(
            "SELECT workspace_id FROM department WHERE id = ?1",
            params![input.department_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("부서를 찾지 못했습니다."))?;
    if dept_ws != workspace_id {
        return Err(AppError::invalid(
            "다른 작업공간의 부서입니다. 현재 작업공간의 부서를 골라 주세요.",
        ));
    }

    let ctx = ctx_of(conn, workspace_id, input.student_id, input.department_id)?;

    conn.execute(
        "INSERT INTO enrollment (workspace_id, student_id, department_id) VALUES (?1, ?2, ?3)",
        params![workspace_id, input.student_id, input.department_id],
    )
    .map_err(|e| {
        if e.to_string().contains("enrollment.department_id") {
            AppError::new(
                "CONFLICT",
                format!("{}은(는) {} 부서를 이미 수강 중입니다.", ctx.student_label, ctx.dept_label),
            )
        } else {
            e.into()
        }
    })?;
    let id = conn.last_insert_rowid();

    let base = base_fees(conn, input.department_id, items)?;
    let fees = if input.fees.is_empty() {
        base.clone()
    } else {
        input.fees.clone()
    };
    write_charges(conn, id, &fees, &base)?;

    log::write(
        conn,
        ctx.year_id,
        Some(workspace_id),
        log::ENROLL_ADD,
        Some(ctx.student_id),
        Some(ctx.department_id),
        &format!("{} / {}", ctx.student_label, ctx.dept_label),
        "",
        &fees_text(items, &fees),
        input.reason.as_deref().unwrap_or(""),
    )?;
    Ok(id)
}

/// Excel 업로드 반영. 이미 수강 중인 건은 건너뛴다(검증에서 이미 걸렀지만 한 번 더).
pub fn create_bulk(
    conn: &Connection,
    workspace_id: i64,
    rows: &[EnrollmentRow],
    items: &[CostItem],
) -> AppResult<(usize, usize)> {
    let mut added = 0;
    let mut skipped = 0;
    for r in rows {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM enrollment
                  WHERE workspace_id = ?1 AND student_id = ?2 AND department_id = ?3
                    AND status = 'ACTIVE'",
                params![workspace_id, r.student_id, r.department_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            skipped += 1;
            continue;
        }
        create(
            conn,
            workspace_id,
            &EnrollmentInput {
                student_id: r.student_id,
                department_id: r.department_id,
                fees: Vec::new(),
                reason: Some("Excel 업로드".into()),
            },
            items,
        )?;
        added += 1;
    }
    Ok((added, skipped))
}

/// 금액만 고친다. 학생·부서는 식별정보이므로 바꾸지 않는다
/// (부서를 옮기려면 취소하고 다시 등록한다).
pub fn update_fees(
    conn: &Connection,
    id: i64,
    fees: &[Fee],
    reason: &str,
    items: &[CostItem],
) -> AppResult<()> {
    let before = get(conn, id, items)?;
    let base = base_fees(conn, before.department_id, items)?;
    write_charges(conn, id, fees, &base)?;
    conn.execute(
        "UPDATE enrollment SET updated_at = datetime('now', 'localtime') WHERE id = ?1",
        params![id],
    )?;

    let after_text = fees_text(items, fees);
    let before_text = fees_text(items, &before.fees);
    if before_text == after_text {
        return Ok(()); // 바뀐 것이 없으면 이력을 남기지 않는다
    }
    let year_id = year_of_workspace(conn, workspace_of(conn, id)?)?;
    log::write(
        conn,
        year_id,
        Some(workspace_of(conn, id)?),
        log::CHARGE_EDIT,
        Some(before.student_id),
        Some(before.department_id),
        &format!(
            "{} / {}",
            student_label(before.grade, before.class_no, before.student_no, &before.name),
            before.dept_label
        ),
        &before_text,
        &after_text,
        reason,
    )?;
    Ok(())
}

fn workspace_of(conn: &Connection, enrollment_id: i64) -> AppResult<i64> {
    conn.query_row(
        "SELECT workspace_id FROM enrollment WHERE id = ?1",
        params![enrollment_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("수강 자료를 찾지 못했습니다."))
}

/// 수강 취소 — 행을 지우지 않고 상태만 바꾼다 (§42-4).
pub fn cancel(conn: &Connection, id: i64, reason: &str, items: &[CostItem]) -> AppResult<()> {
    let row = get(conn, id, items)?;
    if row.status == "CANCELLED" {
        return Err(AppError::invalid("이미 취소된 수강입니다."));
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::invalid("변경사유를 입력해 주세요."));
    }
    let ws = workspace_of(conn, id)?;
    conn.execute(
        "UPDATE enrollment
            SET status = 'CANCELLED', change_reason = ?2,
                updated_at = datetime('now', 'localtime')
          WHERE id = ?1",
        params![id, reason],
    )?;
    log::write(
        conn,
        year_of_workspace(conn, ws)?,
        Some(ws),
        log::ENROLL_CANCEL,
        Some(row.student_id),
        Some(row.department_id),
        &format!(
            "{} / {}",
            student_label(row.grade, row.class_no, row.student_no, &row.name),
            row.dept_label
        ),
        "수강중",
        "취소",
        reason,
    )?;
    Ok(())
}

/// 취소한 수강을 되돌린다.
pub fn restore(conn: &Connection, id: i64, reason: &str, items: &[CostItem]) -> AppResult<()> {
    let row = get(conn, id, items)?;
    if row.status == "ACTIVE" {
        return Err(AppError::invalid("이미 수강 중입니다."));
    }
    let ws = workspace_of(conn, id)?;
    conn.execute(
        "UPDATE enrollment
            SET status = 'ACTIVE', change_reason = ?2,
                updated_at = datetime('now', 'localtime')
          WHERE id = ?1",
        params![id, reason.trim()],
    )
    .map_err(|e| {
        if e.to_string().contains("enrollment.department_id") {
            AppError::new(
                "CONFLICT",
                "같은 학생이 같은 부서를 이미 수강 중이라 되돌릴 수 없습니다. 새로 등록된 건을 확인해 주세요.",
            )
        } else {
            e.into()
        }
    })?;
    log::write(
        conn,
        year_of_workspace(conn, ws)?,
        Some(ws),
        log::ENROLL_RESTORE,
        Some(row.student_id),
        Some(row.department_id),
        &format!(
            "{} / {}",
            student_label(row.grade, row.class_no, row.student_no, &row.name),
            row.dept_label
        ),
        "취소",
        "수강중",
        reason.trim(),
    )?;
    Ok(())
}

/// 학생별 금액 수정 팝업 저장 — 바뀐 학생의 `charge`만 고친다.
/// 부서 기준금액은 건드리지 않는다 (요구사항 §12).
pub fn save_student_fees(
    conn: &Connection,
    edits: &[StudentFeeEdit],
    reason: &str,
    items: &[CostItem],
) -> AppResult<i64> {
    let mut changed = 0;
    for e in edits {
        let before = get(conn, e.enrollment_id, items)?;
        if fees_text(items, &before.fees) == fees_text(items, &e.fees) {
            continue;
        }
        update_fees(conn, e.enrollment_id, &e.fees, reason, items)?;
        changed += 1;
    }
    Ok(changed)
}

// ─────────────────────────────────────────────── 부서 기준금액 재반영

/// 부서 기준금액과 실제 청구액이 어긋난 칸을 모두 찾는다.
///
/// **바뀔 것만** 돌려준다 — 같은 값은 목록에 넣지 않는다.
pub fn fee_diff(
    conn: &Connection,
    workspace_id: i64,
    department_id: Option<i64>,
    items: &[CostItem],
) -> AppResult<Vec<FeeDiff>> {
    let names: HashMap<&str, &str> = items
        .iter()
        .map(|i| (i.code.as_str(), i.name.as_str()))
        .collect();

    let mut st = conn.prepare(
        "SELECT e.id, s.id, s.grade, s.class_no, s.student_no, s.name,
                d.id, d.name, d.class_name,
                c.item_code, c.amount, c.is_overridden,
                COALESCE(f.amount, 0)
           FROM enrollment e
           JOIN student s        ON s.id = e.student_id
           JOIN department d     ON d.id = e.department_id
           JOIN charge c         ON c.enrollment_id = e.id
           LEFT JOIN department_fee f
                  ON f.department_id = d.id AND f.item_code = c.item_code
          WHERE e.workspace_id = ?1
            AND e.status = 'ACTIVE'
            AND (?2 IS NULL OR e.department_id = ?2)
            AND c.amount <> COALESCE(f.amount, 0)
          ORDER BY d.name, d.class_name, s.grade, s.class_no, s.student_no",
    )?;

    let rows = st
        .query_map(params![workspace_id, department_id], |r| {
            let code: String = r.get(9)?;
            Ok(FeeDiff {
                enrollment_id: r.get(0)?,
                student_id: r.get(1)?,
                grade: r.get(2)?,
                class_no: r.get(3)?,
                student_no: r.get(4)?,
                name: r.get(5)?,
                department_id: r.get(6)?,
                dept_label: dept_label(&r.get::<_, String>(7)?, &r.get::<_, String>(8)?),
                item_name: names.get(code.as_str()).copied().unwrap_or("").to_string(),
                item_code: code,
                current: r.get(10)?,
                is_overridden: r.get::<_, i64>(11)? == 1,
                base: r.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // 비용항목 차례로 다시 정렬한다 (SQL의 item_code 순서는 뜻이 없다).
    let order: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (it.code.as_str(), i))
        .collect();
    let mut rows = rows;
    rows.sort_by_key(|r| {
        (
            r.dept_label.clone(),
            r.grade,
            r.class_no,
            r.student_no,
            order.get(r.item_code.as_str()).copied().unwrap_or(usize::MAX),
        )
    });
    Ok(rows)
}

/// 재반영 방식.
///
/// * `KEEP_EDITED` — 학생별로 고친 금액은 그대로 두고 나머지만 맞춘다 (권장)
/// * `ALL`         — 학생별 수정까지 포함해 전부 기준금액으로 맞춘다
/// * `SELECTED`    — 미리보기에서 고른 칸만 맞춘다
pub fn apply_fees(
    conn: &Connection,
    workspace_id: i64,
    department_id: Option<i64>,
    mode: &str,
    picks: &[FeePick],
    reason: &str,
    items: &[CostItem],
) -> AppResult<ApplyResult> {
    let diffs = fee_diff(conn, workspace_id, department_id, items)?;
    let picked: std::collections::HashSet<(i64, &str)> = picks
        .iter()
        .map(|p| (p.enrollment_id, p.item_code.as_str()))
        .collect();

    let mut kept = 0;
    let mut targets: HashMap<i64, Vec<&FeeDiff>> = HashMap::new();
    for d in &diffs {
        let take = match mode {
            "ALL" => true,
            "KEEP_EDITED" => !d.is_overridden,
            "SELECTED" => picked.contains(&(d.enrollment_id, d.item_code.as_str())),
            _ => return Err(AppError::invalid("알 수 없는 반영 방식입니다.")),
        };
        if take {
            targets.entry(d.enrollment_id).or_default().push(d);
        } else if d.is_overridden {
            kept += 1;
        }
    }

    let mut changed = 0;
    let year_id = year_of_workspace(conn, workspace_id)?;
    for (eid, cells) in &targets {
        let before = get(conn, *eid, items)?;
        let mut fees = before.fees.clone();
        for cell in cells {
            if let Some(f) = fees.iter_mut().find(|f| f.item_code == cell.item_code) {
                f.amount = cell.base;
            }
            changed += 1;
        }
        let base = base_fees(conn, before.department_id, items)?;
        write_charges(conn, *eid, &fees, &base)?;
        conn.execute(
            "UPDATE enrollment SET updated_at = datetime('now', 'localtime') WHERE id = ?1",
            params![eid],
        )?;
        log::write(
            conn,
            year_id,
            Some(workspace_id),
            log::DEPT_APPLY,
            Some(before.student_id),
            Some(before.department_id),
            &format!(
                "{} / {}",
                student_label(before.grade, before.class_no, before.student_no, &before.name),
                before.dept_label
            ),
            &fees_text(items, &before.fees),
            &fees_text(items, &fees),
            reason,
        )?;
    }

    Ok(ApplyResult {
        changed,
        kept,
        enrollments: targets.len() as i64,
    })
}

// ─────────────────────────────────────────────── 학생 상세정보

/// 조회 전용. **지원금 사용액·잔액을 여기서 계산하지 않는다** — Phase 3에서
/// 정산 결과를 연결한다. 없는 숫자를 지어내면 사람이 그것을 믿게 된다.
pub fn student_detail(
    conn: &Connection,
    year_id: i64,
    student_id: i64,
    items: &[CostItem],
) -> AppResult<StudentDetail> {
    let (grade, class_no, student_no, name, note): (i64, i64, i64, String, String) = conn
        .query_row(
            "SELECT grade, class_no, student_no, name, note FROM student WHERE id = ?1",
            params![student_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("학생을 찾지 못했습니다."))?;

    // 지원자격 — 기간까지 문구로 만든다
    let mut supports = Vec::new();
    for (code, label) in [("VOUCHER", "방과후 이용권"), ("FREE_VOUCHER", "자유수강권")] {
        let mut st = conn.prepare(
            "SELECT valid_from, valid_to FROM support_eligibility
              WHERE year_id = ?1 AND student_id = ?2 AND program = ?3
              ORDER BY COALESCE(valid_from, '0000-01-01')",
        )?;
        let periods = st
            .query_map(params![year_id, student_id, code], |r| {
                let from: Option<String> = r.get(0)?;
                let to: Option<String> = r.get(1)?;
                Ok(match (from, to) {
                    (None, None) => "학년도 내내".to_string(),
                    (f, t) => format!(
                        "{} ~ {}",
                        f.unwrap_or_else(|| "학년도 시작".into()),
                        t.unwrap_or_else(|| "학년도 끝".into())
                    ),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(st);

        let targets: Option<String> = conn
            .query_row(
                "SELECT target_grades FROM support_policy WHERE year_id = ?1 AND program = ?2",
                params![year_id, code],
                |r| r.get(0),
            )
            .optional()?;
        let targets = parse_grades(&targets.unwrap_or_default());

        supports.push(SupportView {
            program: code.to_string(),
            program_label: label.to_string(),
            eligible: !periods.is_empty(),
            grade_mismatch: !periods.is_empty() && !grade_matches(&targets, grade),
            periods,
        });
    }

    // 작업공간별 수강내역 — 최근 작업공간이 먼저
    let ws_dates = workspace_dates(conn, year_id)?;
    let mut st = conn.prepare(
        "SELECT id, name, start_date, end_date FROM workspace
          WHERE year_id = ?1 ORDER BY start_date DESC, end_date DESC, id DESC",
    )?;
    let all_ws = st
        .query_map(params![year_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let mut workspaces = Vec::new();
    for (id, name, start, end) in all_ws {
        let sql = format!("{RAW_SELECT} WHERE e.workspace_id = ?1 AND e.student_id = ?2{RAW_ORDER}");
        let mut st = conn.prepare(&sql)?;
        let raws = st
            .query_map(params![id, student_id], map_raw)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(st);
        if raws.is_empty() {
            continue; // 수강이 없는 작업공간은 보여 주지 않는다
        }
        let rows = enrich(conn, raws, items, year_id, &ws_dates)?;
        let active_total = rows
            .iter()
            .filter(|r| r.status == "ACTIVE")
            .map(|r| r.total)
            .sum();
        workspaces.push(WorkspaceEnrollments {
            workspace_id: id,
            workspace_name: name,
            start_date: start,
            end_date: end,
            rows,
            active_total,
        });
    }

    // 지원유형은 학년도 전체 기준 (상세 화면의 머리말용)
    let elig = eligibility_map(conn, year_id)?;
    let mut programs: Vec<String> = elig
        .get(&student_id)
        .map(|v| v.iter().map(|(p, _, _)| p.clone()).collect())
        .unwrap_or_default();
    programs.sort();
    programs.dedup();

    Ok(StudentDetail {
        student: Student {
            id: student_id,
            grade,
            class_no,
            student_no,
            name,
            note,
            programs,
        },
        supports,
        workspaces,
    })
}

pub fn count_active(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1 AND status = 'ACTIVE'",
        params![workspace_id],
        |r| r.get(0),
    )?)
}

/// 학생·부서로 이미 수강 중인지 본다. Excel 검증에서 쓴다.
pub fn active_exists(
    conn: &Connection,
    workspace_id: i64,
    student_id: i64,
    department_id: i64,
) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollment
          WHERE workspace_id = ?1 AND student_id = ?2 AND department_id = ?3 AND status = 'ACTIVE'",
        params![workspace_id, student_id, department_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}
