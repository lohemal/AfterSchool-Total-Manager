//! 학생정보 — 전교생. 학년도 소속이며 작업공간마다 다시 올리지 않는다.
//!
//! `지원유형`은 컬럼이 아니라 `support_eligibility`에서 파생된다 (설계안 4-2).

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::error::{AppError, AppResult};
use crate::model::{Student, StudentFilter, StudentInput};
use crate::repo::required_text;

/// Excel 업로드 한 줄. 검증을 마친 상태로 들어온다.
#[derive(Debug, Clone)]
pub struct StudentRow {
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    pub note: String,
}

pub fn list(conn: &Connection, year_id: i64, f: &StudentFilter) -> AppResult<Vec<Student>> {
    // 자격 유효 판정 기준: 작업공간이 지정되면 그 기간, 아니면 학년도 전체
    let (ws_start, ws_end) = match f.workspace_id {
        Some(id) => conn
            .query_row(
                "SELECT start_date, end_date FROM workspace WHERE id = ?1",
                params![id],
                |r| Ok((Some(r.get::<_, String>(0)?), Some(r.get::<_, String>(1)?))),
            )
            .optional()?
            .unwrap_or((None, None)),
        None => (None, None),
    };

    let mut sql = String::from(
        "WITH elig AS (
           SELECT DISTINCT student_id, program
             FROM support_eligibility
            WHERE year_id = ?1
              AND (?2 IS NULL OR (
                    (valid_from IS NULL OR valid_from <= ?3)
                AND (valid_to   IS NULL OR valid_to   >= ?2)))
         )
         SELECT s.id, s.grade, s.class_no, s.student_no, s.name, s.note,
                COALESCE((SELECT GROUP_CONCAT(program) FROM elig WHERE student_id = s.id), '')
           FROM student s
          WHERE s.year_id = ?1",
    );
    let mut args: Vec<Box<dyn ToSql>> = vec![
        Box::new(year_id),
        Box::new(ws_start.clone()),
        Box::new(ws_end.clone()),
    ];

    if let Some(g) = f.grade {
        args.push(Box::new(g));
        sql.push_str(&format!(" AND s.grade = ?{}", args.len()));
    }
    if let Some(c) = f.class_no {
        args.push(Box::new(c));
        sql.push_str(&format!(" AND s.class_no = ?{}", args.len()));
    }
    if let Some(n) = f.student_no {
        args.push(Box::new(n));
        sql.push_str(&format!(" AND s.student_no = ?{}", args.len()));
    }
    if let Some(name) = f.name.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        args.push(Box::new(format!("%{name}%")));
        sql.push_str(&format!(" AND s.name LIKE ?{}", args.len()));
    }
    if let Some(q) = f.query.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        args.push(Box::new(format!("%{q}%")));
        let i = args.len();
        sql.push_str(&format!(" AND (s.name LIKE ?{i} OR s.note LIKE ?{i})"));
    }
    sql.push_str(" ORDER BY s.grade, s.class_no, s.student_no");

    let mut st = conn.prepare(&sql)?;
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = st
        .query_map(refs.as_slice(), |r| {
            let joined: String = r.get(6)?;
            let mut programs: Vec<String> = joined
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            programs.sort();
            programs.dedup();
            Ok(Student {
                id: r.get(0)?,
                grade: r.get(1)?,
                class_no: r.get(2)?,
                student_no: r.get(3)?,
                name: r.get(4)?,
                note: r.get(5)?,
                programs,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // 지원유형 필터는 결과가 나온 뒤 거른다 (한 학교 규모에서는 이 편이 단순하고 빠르다).
    let filtered = match f.program.as_deref() {
        Some("VOUCHER") => rows
            .into_iter()
            .filter(|s| s.programs.iter().any(|p| p == "VOUCHER"))
            .collect(),
        Some("FREE_VOUCHER") => rows
            .into_iter()
            .filter(|s| s.programs.iter().any(|p| p == "FREE_VOUCHER"))
            .collect(),
        Some("BOTH") => rows.into_iter().filter(|s| s.programs.len() >= 2).collect(),
        Some("NONE") => rows.into_iter().filter(|s| s.programs.is_empty()).collect(),
        _ => rows,
    };
    Ok(filtered)
}

fn check(input: &StudentInput) -> AppResult<String> {
    if !(1..=9).contains(&input.grade) {
        return Err(AppError::invalid("학년은 1~9 사이여야 합니다."));
    }
    if !(1..=99).contains(&input.class_no) {
        return Err(AppError::invalid("반은 1~99 사이여야 합니다."));
    }
    if !(1..=99).contains(&input.student_no) {
        return Err(AppError::invalid("번호는 1~99 사이여야 합니다."));
    }
    required_text("이름", &input.name)
}

pub fn create(conn: &Connection, year_id: i64, input: &StudentInput) -> AppResult<i64> {
    let name = check(input)?;
    conn.execute(
        "INSERT INTO student (year_id, grade, class_no, student_no, name, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            year_id,
            input.grade,
            input.class_no,
            input.student_no,
            name,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &StudentInput) -> AppResult<()> {
    let name = check(input)?;
    let n = conn.execute(
        "UPDATE student SET grade = ?2, class_no = ?3, student_no = ?4, name = ?5, note = ?6
          WHERE id = ?1",
        params![
            id,
            input.grade,
            input.class_no,
            input.student_no,
            name,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    if n == 0 {
        return Err(AppError::not_found("학생을 찾지 못했습니다."));
    }
    Ok(())
}

pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM student WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub fn delete_all(conn: &Connection, year_id: i64) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM student WHERE year_id = ?1", params![year_id])?)
}

/// Excel 업로드 반영. `학년+반+번호`가 같은 학생은 이름·비고를 갱신하고,
/// 없는 행만 새로 만든다 (설계안 0-10). 기존 학생을 지우지 않는다.
pub fn upsert_bulk(conn: &Connection, year_id: i64, rows: &[StudentRow]) -> AppResult<(usize, usize)> {
    let mut added = 0;
    let mut updated = 0;
    for r in rows {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM student
                  WHERE year_id = ?1 AND grade = ?2 AND class_no = ?3 AND student_no = ?4",
                params![year_id, r.grade, r.class_no, r.student_no],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE student SET name = ?2, note = ?3 WHERE id = ?1",
                    params![id, r.name, r.note],
                )?;
                updated += 1;
            }
            None => {
                conn.execute(
                    "INSERT INTO student (year_id, grade, class_no, student_no, name, note)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![year_id, r.grade, r.class_no, r.student_no, r.name, r.note],
                )?;
                added += 1;
            }
        }
    }
    Ok((added, updated))
}

/// `학년-반-번호`로 학생을 찾는다. Excel 업로드 검증에서 쓴다.
pub fn find_by_key(
    conn: &Connection,
    year_id: i64,
    grade: i64,
    class_no: i64,
    student_no: i64,
) -> AppResult<Option<(i64, String)>> {
    Ok(conn
        .query_row(
            "SELECT id, name FROM student
              WHERE year_id = ?1 AND grade = ?2 AND class_no = ?3 AND student_no = ?4",
            params![year_id, grade, class_no, student_no],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub fn count(conn: &Connection, year_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM student WHERE year_id = ?1",
        params![year_id],
        |r| r.get(0),
    )?)
}
