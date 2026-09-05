//! 지원대상자 — 학생 × 지원제도 × 유효기간 (설계안 0-2).
//!
//! 대상학년 정책과 어긋나는 학생이 있어도 **자동으로 지우거나 고치지 않는다.**
//! `grade_mismatch` 표시만 올려 보내고 판단은 사람이 한다 (설계안 0-3).

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{grade_matches, parse_grades};
use crate::error::{AppError, AppResult};
use crate::model::{Eligibility, EligibilityInput};
use crate::repo::check_date;

/// Excel 업로드 한 줄.
#[derive(Debug, Clone)]
pub struct EligibilityRow {
    pub student_id: i64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub note: String,
}

fn valid_program(program: &str) -> AppResult<&str> {
    match program {
        "VOUCHER" | "FREE_VOUCHER" => Ok(program),
        _ => Err(AppError::invalid("알 수 없는 지원제도입니다.")),
    }
}

/// 그 학년도·제도의 대상학년 목록. 비어 있으면 전 학년이 대상이다.
pub fn target_grades(conn: &Connection, year_id: i64, program: &str) -> AppResult<Vec<i64>> {
    let text: Option<String> = conn
        .query_row(
            "SELECT target_grades FROM support_policy WHERE year_id = ?1 AND program = ?2",
            params![year_id, program],
            |r| r.get(0),
        )
        .optional()?;
    Ok(parse_grades(&text.unwrap_or_default()))
}

pub fn list(conn: &Connection, year_id: i64, program: &str, query: Option<&str>) -> AppResult<Vec<Eligibility>> {
    valid_program(program)?;
    let targets = target_grades(conn, year_id, program)?;

    let like = query
        .map(|q| q.trim())
        .filter(|q| !q.is_empty())
        .map(|q| format!("%{q}%"));

    let mut st = conn.prepare(
        "SELECT e.id, e.student_id, e.program, e.valid_from, e.valid_to, e.source, e.note,
                s.grade, s.class_no, s.student_no, s.name
           FROM support_eligibility e
           JOIN student s ON s.id = e.student_id
          WHERE e.year_id = ?1 AND e.program = ?2
            AND (?3 IS NULL OR s.name LIKE ?3 OR e.note LIKE ?3)
          ORDER BY s.grade, s.class_no, s.student_no",
    )?;
    let rows = st
        .query_map(params![year_id, program, like], |r| {
            let grade: i64 = r.get(7)?;
            Ok(Eligibility {
                id: r.get(0)?,
                student_id: r.get(1)?,
                program: r.get(2)?,
                valid_from: r.get(3)?,
                valid_to: r.get(4)?,
                source: r.get(5)?,
                note: r.get(6)?,
                grade,
                class_no: r.get(8)?,
                student_no: r.get(9)?,
                name: r.get(10)?,
                grade_mismatch: !grade_matches(&targets, grade),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn check_range(from: Option<&str>, to: Option<&str>) -> AppResult<()> {
    if let Some(f) = from {
        check_date("적용 시작일", f)?;
    }
    if let Some(t) = to {
        check_date("적용 종료일", t)?;
    }
    if let (Some(f), Some(t)) = (from, to) {
        if f > t {
            return Err(AppError::invalid("적용 종료일이 시작일보다 빠릅니다."));
        }
    }
    Ok(())
}

/// 같은 학생·제도에서 기간이 겹치는 행이 있으면 막는다 (중복 지원 방지).
fn guard_overlap(
    conn: &Connection,
    year_id: i64,
    student_id: i64,
    program: &str,
    from: Option<&str>,
    to: Option<&str>,
    exclude_id: Option<i64>,
) -> AppResult<()> {
    let f = from.unwrap_or("0000-01-01");
    let t = to.unwrap_or("9999-12-31");
    let hit: Option<i64> = conn
        .query_row(
            "SELECT id FROM support_eligibility
              WHERE year_id = ?1 AND student_id = ?2 AND program = ?3
                AND id <> COALESCE(?6, -1)
                AND COALESCE(valid_from, '0000-01-01') <= ?5
                AND COALESCE(valid_to,   '9999-12-31') >= ?4
              LIMIT 1",
            params![year_id, student_id, program, f, t, exclude_id],
            |r| r.get(0),
        )
        .optional()?;
    if hit.is_some() {
        return Err(AppError::new(
            "CONFLICT",
            "같은 학생의 지원기간이 이미 등록되어 있습니다. 기존 행을 수정해 주세요.",
        ));
    }
    Ok(())
}

pub fn create(conn: &Connection, year_id: i64, input: &EligibilityInput) -> AppResult<i64> {
    valid_program(&input.program)?;
    check_range(input.valid_from.as_deref(), input.valid_to.as_deref())?;
    guard_overlap(
        conn,
        year_id,
        input.student_id,
        &input.program,
        input.valid_from.as_deref(),
        input.valid_to.as_deref(),
        None,
    )?;
    conn.execute(
        "INSERT INTO support_eligibility
           (year_id, student_id, program, valid_from, valid_to, source, note)
         VALUES (?1, ?2, ?3, ?4, ?5, 'MANUAL', ?6)",
        params![
            year_id,
            input.student_id,
            input.program,
            input.valid_from,
            input.valid_to,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &EligibilityInput) -> AppResult<()> {
    valid_program(&input.program)?;
    check_range(input.valid_from.as_deref(), input.valid_to.as_deref())?;
    let year_id: i64 = conn
        .query_row(
            "SELECT year_id FROM support_eligibility WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("지원대상자를 찾지 못했습니다."))?;
    guard_overlap(
        conn,
        year_id,
        input.student_id,
        &input.program,
        input.valid_from.as_deref(),
        input.valid_to.as_deref(),
        Some(id),
    )?;
    conn.execute(
        "UPDATE support_eligibility
            SET student_id = ?2, valid_from = ?3, valid_to = ?4, note = ?5
          WHERE id = ?1",
        params![
            id,
            input.student_id,
            input.valid_from,
            input.valid_to,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    Ok(())
}

pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM support_eligibility WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub fn delete_all(conn: &Connection, year_id: i64, program: &str) -> AppResult<usize> {
    valid_program(program)?;
    Ok(conn.execute(
        "DELETE FROM support_eligibility WHERE year_id = ?1 AND program = ?2",
        params![year_id, program],
    )?)
}

/// Excel 업로드 반영. 같은 학생·제도에 이미 행이 있으면 기간·비고를 갱신한다.
pub fn upsert_bulk(
    conn: &Connection,
    year_id: i64,
    program: &str,
    rows: &[EligibilityRow],
) -> AppResult<(usize, usize)> {
    valid_program(program)?;
    let mut added = 0;
    let mut updated = 0;
    for r in rows {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM support_eligibility
                  WHERE year_id = ?1 AND student_id = ?2 AND program = ?3
                  ORDER BY id LIMIT 1",
                params![year_id, r.student_id, program],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE support_eligibility
                        SET valid_from = ?2, valid_to = ?3, note = ?4, source = 'EXCEL'
                      WHERE id = ?1",
                    params![id, r.valid_from, r.valid_to, r.note],
                )?;
                updated += 1;
            }
            None => {
                conn.execute(
                    "INSERT INTO support_eligibility
                       (year_id, student_id, program, valid_from, valid_to, source, note)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'EXCEL', ?6)",
                    params![year_id, r.student_id, program, r.valid_from, r.valid_to, r.note],
                )?;
                added += 1;
            }
        }
    }
    Ok((added, updated))
}

/// 대상학년과 어긋나는 대상자 수. 화면 상단 경고에 쓴다.
pub fn mismatch_count(conn: &Connection, year_id: i64, program: &str) -> AppResult<i64> {
    let targets = target_grades(conn, year_id, program)?;
    if targets.is_empty() {
        return Ok(0);
    }
    let list = targets
        .iter()
        .map(|g| g.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT COUNT(*) FROM support_eligibility e JOIN student s ON s.id = e.student_id
          WHERE e.year_id = ?1 AND e.program = ?2 AND s.grade NOT IN ({list})"
    );
    Ok(conn.query_row(&sql, params![year_id, program], |r| r.get(0))?)
}
