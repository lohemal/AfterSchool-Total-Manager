//! 학생정보 — 전교생. 학년도 소속이며 작업공간마다 다시 올리지 않는다.
//!
//! `지원유형`은 컬럼이 아니라 `support_eligibility`에서 파생된다 (설계안 4-2).

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::domain::class_no;
use crate::error::{AppError, AppResult};
use crate::model::{Student, StudentFilter, StudentInput, TransferCheck};
use crate::repo::required_text;

/// Excel 업로드 한 줄. 검증을 마친 상태로 들어온다.
#[derive(Debug, Clone)]
pub struct StudentRow {
    pub grade: i64,
    pub class_no: String,
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
                COALESCE((SELECT GROUP_CONCAT(program) FROM elig WHERE student_id = s.id), ''),
                s.transfer_out_date
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
    if let Some(c) = f.class_no.as_ref().map(|s| class_no::normalize(s)).filter(|s| !s.is_empty()) {
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
    // class_sort 는 숫자 반을 자연 정렬하기 위한 값이다 (domain::class_no 참고).
    // class_no 까지 넣어 차례가 늘 하나로 정해지게 한다.
    sql.push_str(" ORDER BY s.grade, s.class_sort, s.class_no, s.student_no");

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
            let transfer_out_date: Option<String> = r.get(7)?;
            Ok(Student {
                id: r.get(0)?,
                grade: r.get(1)?,
                class_no: r.get(2)?,
                student_no: r.get(3)?,
                name: r.get(4)?,
                note: r.get(5)?,
                programs,
                enrolment: enrolment_code(transfer_out_date.as_deref()).to_string(),
                transfer_out_date,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // 재학상태는 오늘과 견주어 나오는 값이라 SQL 로 거르지 않는다 — 날짜 비교를
    // SQL 에 적으면 '오늘'의 뜻이 두 곳에 생긴다.
    let rows: Vec<Student> = match f.enrolment.as_deref() {
        Some(code @ ("ATTENDING" | "LEAVING" | "LEFT")) => {
            rows.into_iter().filter(|s| s.enrolment == code).collect()
        }
        _ => rows,
    };

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

/// 다듬은 `(반, 이름)`을 돌려준다. 반은 앞뒤 공백을 뗀 값이라 저장할 때 이걸 쓴다.
fn check(input: &StudentInput) -> AppResult<(String, String)> {
    if !(1..=9).contains(&input.grade) {
        return Err(AppError::invalid("학년은 1~9 사이여야 합니다."));
    }
    let class_no = class_no::check(&input.class_no).map_err(AppError::invalid)?;
    if !(1..=99).contains(&input.student_no) {
        return Err(AppError::invalid("번호는 1~99 사이여야 합니다."));
    }
    let name = required_text("이름", &input.name)?;
    Ok((class_no, name))
}

pub fn create(conn: &Connection, year_id: i64, input: &StudentInput) -> AppResult<i64> {
    let (class_no, name) = check(input)?;
    conn.execute(
        "INSERT INTO student (year_id, grade, class_no, student_no, name, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            year_id,
            input.grade,
            class_no,
            input.student_no,
            name,
            input.note.clone().unwrap_or_default()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &StudentInput) -> AppResult<()> {
    let (class_no, name) = check(input)?;
    let n = conn.execute(
        "UPDATE student SET grade = ?2, class_no = ?3, student_no = ?4, name = ?5, note = ?6
          WHERE id = ?1",
        params![
            id,
            input.grade,
            class_no,
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

/// 학생을 지운다. **잘못 넣었거나 두 번 넣은 자료를 치울 때만** 쓴다.
///
/// ## 왜 자료가 딸린 학생을 막는가
///
/// `enrollment` 는 `ON DELETE CASCADE` 라서, 학생을 지우면 그 학생의 수강과
/// `charge` 가 **말없이 함께 사라진다.** 그런데 `settlement_alloc` ·
/// `settlement_budget` 은 외래키가 없어 그대로 남는다. 그러면 정산 스냅샷은
/// 있는 학생을 가리키지 못하고, 합계와 명단이 어긋난 채로 남는다.
///
/// 되돌릴 수 없는 손상이므로 경고창만으로 넘기지 않고 **아예 막는다.**
/// 실제로 학교를 떠난 학생은 지우는 것이 아니라 [전출 처리] 할 일이다.
///
/// 아무 기록도 없는 오입력 학생은 그대로 지워진다.
pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    // 돈이 오간 기록이 딸려 있으면 지우지 않는다 (설계안 26-7).
    crate::repo::adjustment::block_for_students(conn, ids)?;
    for id in ids {
        guard_deletable(conn, *id)?;
    }
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM student WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

/// 지워도 되는 학생인가. 걸리는 것이 있으면 무엇인지 우리말로 알려 준다.
pub fn guard_deletable(conn: &Connection, id: i64) -> AppResult<()> {
    let enrollments: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollment WHERE student_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let settled: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM settlement_alloc WHERE student_id = ?1)
              + (SELECT COUNT(*) FROM settlement_budget WHERE student_id = ?1)",
        params![id],
        |r| r.get(0),
    )?;
    if enrollments == 0 && settled == 0 {
        return Ok(());
    }

    let label = label_of(conn, id)?;
    let mut 걸린것 = Vec::new();
    if enrollments > 0 {
        걸린것.push(format!("수강 기록 {enrollments}건"));
    }
    if settled > 0 {
        걸린것.push(format!("정산 기록 {settled}건"));
    }
    Err(AppError::invalid(format!(
        "{label} 학생에게는 {}이 있어 삭제할 수 없습니다. \
         삭제는 잘못 등록했거나 두 번 등록한 학생을 치울 때만 쓰는 기능입니다. \
         실제로 학교를 떠난 학생은 [전출 처리]를 이용해 주세요 — \
         전출해도 지난 수강·금액·정산 기록은 그대로 남습니다.",
        걸린것.join(" · ")
    )))
}

pub fn delete_all(conn: &Connection, year_id: i64) -> AppResult<usize> {
    crate::repo::adjustment::block_for_year(conn, year_id)?;
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
    class_no: &str,
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

// ─────────────────────────────────────────────── 전출 (v0.1.8)

/// 전출일로 재학상태 코드를 낸다. 저장하지 않고 **오늘과 견주어** 판정한다.
pub fn enrolment_code(transfer_out_date: Option<&str>) -> &'static str {
    use crate::domain::school_date::{enrolment_on, today, Enrolment};
    match enrolment_on(transfer_out_date, today()) {
        Enrolment::Attending => "ATTENDING",
        Enrolment::Leaving => "LEAVING",
        Enrolment::Left => "LEFT",
    }
}

fn label_of(conn: &Connection, id: i64) -> AppResult<String> {
    conn.query_row(
        "SELECT grade, class_no, student_no, name FROM student WHERE id = ?1",
        params![id],
        |r| {
            Ok(format!(
                "{}학년 {}반 {}번 {}",
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?
            ))
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("학생을 찾지 못했습니다."))
}

/// 전출 처리 전에 무엇이 걸려 있는지 살펴본다. **아무것도 고치지 않는다.**
///
/// 전출일이 오늘이거나 지났는데 수강이 남아 있으면 `blocked` 다. 그 수강을
/// 자동으로 취소하지 않는 까닭은, 중도 취소 금액을 얼마로 할지는 사람이
/// 실제 상황을 보고 정해야 하기 때문이다 (설계안 29-4).
pub fn transfer_check(conn: &Connection, id: i64, date: Option<&str>) -> AppResult<TransferCheck> {
    let student_label = label_of(conn, id)?;

    let mut st = conn.prepare(
        "SELECT d.name || d.class_name
           FROM enrollment e JOIN department d ON d.id = e.department_id
          WHERE e.student_id = ?1 AND e.status = 'ACTIVE'
          ORDER BY d.name, d.class_name",
    )?;
    let active_departments: Vec<String> = st
        .query_map(params![id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    let past_enrollments: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollment WHERE student_id = ?1 AND status <> 'ACTIVE'",
        params![id],
        |r| r.get(0),
    )?;

    let active_count = active_departments.len() as i64;
    // 전출 예정(미래 날짜)은 수강이 있어도 막지 않는다 — 그날까지는 정상으로
    // 수업을 듣기 때문이다.
    let already_left = matches!(
        crate::domain::school_date::enrolment_on(date, crate::domain::school_date::today()),
        crate::domain::school_date::Enrolment::Left
    );

    Ok(TransferCheck {
        student_id: id,
        student_label,
        active_count,
        active_departments,
        past_enrollments,
        blocked: already_left && active_count > 0,
    })
}

/// 전출일을 지정하거나 고친다. `None` 이면 전출 정보를 지운다(재학으로 되돌림).
///
/// ## 수강·금액·정산을 건드리지 않는다
///
/// 전출은 **학생의 재학 상태**일 뿐이다. 수강료 확정은 기존 취소 기능이 하고,
/// 이미 만든 정산 스냅샷은 그대로 둔다. `student` 의 자료판 트리거는 학년이
/// 바뀔 때만 돌므로 이 함수가 정산을 낡음으로 만들지 않는다.
pub fn transfer_set(conn: &Connection, id: i64, date: Option<&str>) -> AppResult<()> {
    let date = date.map(str::trim).filter(|s| !s.is_empty());
    if let Some(d) = date {
        // 형식을 여기서 한 번 더 본다 — DB CHECK 가 막아 주지만, 사람에게는
        // 우리말로 알려 주는 편이 낫다.
        crate::domain::school_date::parse(d)?;
    }

    let before: Option<String> = conn
        .query_row(
            "SELECT transfer_out_date FROM student WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    // 전출일이 이미 지난 날짜인데 수강이 남아 있으면 막는다.
    let check = transfer_check(conn, id, date)?;
    if check.blocked {
        return Err(AppError::invalid(format!(
            "현재 수강 중인 프로그램이 {}건 있습니다 ({}). \
             전출 처리 전에 [추가·취소 관리]에서 수강 취소를 완료하여 \
             최종 수강료를 확정해 주세요.",
            check.active_count,
            check.active_departments.join(" · ")
        )));
    }

    let n = conn.execute(
        "UPDATE student SET transfer_out_date = ?2 WHERE id = ?1",
        params![id, date],
    )?;
    if n == 0 {
        return Err(AppError::not_found("학생을 찾지 못했습니다."));
    }

    if before.as_deref() == date {
        return Ok(()); // 바뀐 것이 없으면 이력도 남기지 않는다
    }

    use crate::repo::change_log as log;
    let kind = match (before.as_deref(), date) {
        (None, Some(_)) => log::STUDENT_TRANSFER_SET,
        (Some(_), Some(_)) => log::STUDENT_TRANSFER_EDIT,
        (Some(_), None) => log::STUDENT_TRANSFER_CLEAR,
        (None, None) => return Ok(()),
    };
    let year_id: i64 = conn.query_row(
        "SELECT year_id FROM student WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let say = |v: Option<&str>| v.map(|s| s.to_string()).unwrap_or_else(|| "재학".into());
    log::write(
        conn,
        year_id,
        None,
        kind,
        Some(id),
        None,
        &check.student_label,
        &say(before.as_deref()),
        &say(date),
        "",
    )
}

/// 이 학생을 새 수강에 넣을 수 있는가.
///
/// 전출을 마친 학생은 넣을 수 없다. 전출 **예정**은 그날까지 재학생이므로
/// 넣을 수 있다 — 10월 15일에 떠나는 학생이 10월 7일에 신청할 수 있다.
pub fn guard_enrollable(conn: &Connection, id: i64) -> AppResult<()> {
    let date: Option<String> = conn
        .query_row(
            "SELECT transfer_out_date FROM student WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if enrolment_code(date.as_deref()) == "LEFT" {
        let label = label_of(conn, id)?;
        return Err(AppError::invalid(format!(
            "{label} 학생은 {} 자로 전출 처리되어 새 수강을 넣을 수 없습니다.",
            date.unwrap_or_default()
        )));
    }
    Ok(())
}
