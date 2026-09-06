//! 변경이력 (요구사항 §16).
//!
//! `target`은 그때 화면에 보이던 문구를 그대로 담은 **스냅샷**이다.
//! 학생 이름이나 부서명이 나중에 바뀌어도 이력은 그때의 표현을 지킨다.
//! `student_id`·`department_id`는 조회용이며 외래키를 걸지 않는다 —
//! 원본이 지워져도 이력은 남아야 한다.

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::model::ChangeLog;

pub const ENROLL_ADD: &str = "ENROLL_ADD";
pub const ENROLL_EDIT: &str = "ENROLL_EDIT";
pub const ENROLL_CANCEL: &str = "ENROLL_CANCEL";
pub const ENROLL_RESTORE: &str = "ENROLL_RESTORE";
pub const CHARGE_EDIT: &str = "CHARGE_EDIT";
pub const DEPT_APPLY: &str = "DEPT_APPLY";

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        ENROLL_ADD => "수강 추가",
        ENROLL_EDIT => "수강 수정",
        ENROLL_CANCEL => "수강 취소",
        ENROLL_RESTORE => "수강 복원",
        CHARGE_EDIT => "금액 변경",
        DEPT_APPLY => "부서금액 재반영",
        "ELIGIBILITY" => "지원자격 변경",
        "GRANT" => "지원금 변경",
        "POLICY" => "지원정책 변경",
        _ => "기타",
    }
}

/// 이력 한 줄을 남긴다. 쓰기 트랜잭션 안에서 원본 변경과 같이 부른다.
#[allow(clippy::too_many_arguments)]
pub fn write(
    conn: &Connection,
    year_id: i64,
    workspace_id: Option<i64>,
    kind: &str,
    student_id: Option<i64>,
    department_id: Option<i64>,
    target: &str,
    before_value: &str,
    after_value: &str,
    reason: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO change_log
           (year_id, workspace_id, kind, student_id, department_id,
            target, before_value, after_value, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            year_id,
            workspace_id,
            kind,
            student_id,
            department_id,
            target,
            before_value,
            after_value,
            reason
        ],
    )?;
    Ok(())
}

/// 최근 이력부터. 작업공간·학생·종류로 좁힐 수 있다.
pub fn list(
    conn: &Connection,
    year_id: i64,
    workspace_id: Option<i64>,
    student_id: Option<i64>,
    kind: Option<&str>,
    limit: i64,
) -> AppResult<Vec<ChangeLog>> {
    let mut st = conn.prepare(
        "SELECT c.id, c.at, COALESCE(w.name, ''), c.kind,
                c.target, c.before_value, c.after_value, c.reason
           FROM change_log c
           LEFT JOIN workspace w ON w.id = c.workspace_id
          WHERE c.year_id = ?1
            AND (?2 IS NULL OR c.workspace_id = ?2)
            AND (?3 IS NULL OR c.student_id = ?3)
            AND (?4 IS NULL OR c.kind = ?4)
          ORDER BY c.at DESC, c.id DESC
          LIMIT ?5",
    )?;
    let rows = st
        .query_map(
            params![year_id, workspace_id, student_id, kind, limit],
            |r| {
                let kind: String = r.get(3)?;
                Ok(ChangeLog {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    workspace_name: r.get(2)?,
                    kind_label: kind_label(&kind).to_string(),
                    kind,
                    target: r.get(4)?,
                    before_value: r.get(5)?,
                    after_value: r.get(6)?,
                    reason: r.get(7)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn count(conn: &Connection, year_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM change_log WHERE year_id = ?1",
        params![year_id],
        |r| r.get(0),
    )?)
}
