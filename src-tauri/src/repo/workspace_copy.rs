//! 새 작업공간을 만들며 기존 작업공간의 운영자료를 가져온다 (v0.1.6, 설계안 27장).
//!
//! ## 무엇을 가져오고 무엇을 가져오지 않는가
//!
//! ```text
//! 가져온다      부서 · 부서 기준금액 · 차감 우선순위 · 수강중인 수강 관계
//! 가져오지 않는다  학생 · 지원대상자   (학년도 소속이라 이미 공유된다)
//!                 학생별 최종 금액·override  (지난 기간의 결과다)
//!                 추가징수 · 환불      (지난 기간에 일어난 금전 행정기록이다)
//!                 정산 · 배분 · 지원금 사용 결과
//! ```
//!
//! ## 왜 금액을 가져오지 않는가
//!
//! 새 작업공간은 **새 징수명단을 만드는 기간**이다. 지난 기간에 학생별로 깎아
//! 둔 금액을 그대로 이어받으면, 담당자가 그 금액이 이번 기간에도 맞는지 다시
//! 판단할 기회를 잃는다. 그래서 가져온 수강의 금액은 **새 작업공간에 복사된
//! 부서의 기준금액**에서 시작한다. 그 뒤 부서정보에서 금액을 고치고 [부서금액
//! 반영]으로 정리하면 된다.
//!
//! ## 과거는 건드리지 않는다
//!
//! 읽기만 한다. 기준 작업공간의 어떤 행도 바뀌지 않으므로 그쪽 정산이 낡음이
//! 되지 않는다. 새 작업공간의 자료판은 올라가지만 거기에는 아직 정산이 없다.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::model::{
    CostItem, DepartmentInput, WorkspaceCopyInput, WorkspaceCopyPreview, WorkspaceCopyResult,
};
use crate::repo;
use crate::repo::change_log as log;

/// 만들기 전에 보여 줄 요약.
pub fn preview(conn: &Connection, source_workspace_id: i64) -> AppResult<WorkspaceCopyPreview> {
    let ws = repo::year::get_workspace(conn, source_workspace_id)?;

    let one = |sql: &str| -> AppResult<i64> {
        Ok(conn.query_row(sql, params![source_workspace_id], |r| r.get(0))?)
    };

    let departments = one("SELECT COUNT(*) FROM department WHERE workspace_id = ?1")?;
    let active_enrollments =
        one("SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1 AND status = 'ACTIVE'")?;
    let cancelled_enrollments =
        one("SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1 AND status = 'CANCELLED'")?;
    let adjustments = one("SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1")?;
    let settlements = one("SELECT COUNT(*) FROM settlement WHERE workspace_id = ?1")?;
    let overridden_cells = one(
        "SELECT COUNT(*) FROM charge c
           JOIN enrollment e ON e.id = c.enrollment_id
          WHERE e.workspace_id = ?1 AND c.is_overridden = 1",
    )?;
    let has_priority = one("SELECT COUNT(*) FROM dept_priority WHERE workspace_id = ?1")? > 0;

    // 조용히 건너뛰지 않는다 — 눈에 띄는 것은 미리 적어 보여 준다.
    let mut warnings = Vec::new();
    if departments == 0 {
        warnings.push("이 작업공간에는 부서가 없습니다.".to_string());
    }
    if active_enrollments == 0 && departments > 0 {
        warnings.push("수강중인 학생이 없습니다. 부서만 가져옵니다.".to_string());
    }
    let 기준없는부서: i64 = conn.query_row(
        "SELECT COUNT(*) FROM department d
          WHERE d.workspace_id = ?1
            AND NOT EXISTS (SELECT 1 FROM department_fee f
                             WHERE f.department_id = d.id AND f.amount > 0)",
        params![source_workspace_id],
        |r| r.get(0),
    )?;
    if 기준없는부서 > 0 {
        warnings.push(format!(
            "기준금액이 모두 0원인 부서가 {기준없는부서}개 있습니다. \
             가져온 수강생의 금액도 0원으로 시작합니다."
        ));
    }

    Ok(WorkspaceCopyPreview {
        source_workspace_id,
        source_name: ws.name,
        departments,
        active_enrollments,
        cancelled_enrollments,
        adjustments,
        settlements,
        overridden_cells,
        has_priority,
        warnings,
    })
}

/// 실제로 가져온다.
///
/// **새 작업공간을 만드는 것과 같은 트랜잭션에서 부른다.** 부르는 쪽
/// (`commands::year`)이 `Db::write` 하나로 감싸므로, 중간에 실패하면 작업공간
/// 자체가 만들어지지 않는다 — "작업공간만 생기고 부서는 절반" 이 남지 않는다.
pub fn run(
    conn: &Connection,
    new_workspace_id: i64,
    input: &WorkspaceCopyInput,
    items: &[CostItem],
) -> AppResult<WorkspaceCopyResult> {
    if input.enrollments && !input.departments {
        return Err(AppError::invalid(
            "수강생 명단을 가져오려면 해당 부서정보도 함께 가져와야 합니다.",
        ));
    }
    if new_workspace_id == input.source_workspace_id {
        return Err(AppError::invalid("자기 자신에서는 가져올 수 없습니다."));
    }

    let src = repo::year::get_workspace(conn, input.source_workspace_id)?;
    let dst = repo::year::get_workspace(conn, new_workspace_id)?;
    if src.year_id != dst.year_id {
        return Err(AppError::invalid(
            "같은 학년도의 작업공간에서만 가져올 수 있습니다.",
        ));
    }

    let mut out = WorkspaceCopyResult {
        workspace_id: new_workspace_id,
        departments: 0,
        enrollments: 0,
        charges: 0,
    };
    if !input.departments {
        return Ok(out);
    }

    // ── 1. 부서. 옛 id → 새 id 를 적어 둔다. 수강을 옮길 때 이 표로 바꾼다.
    let mut dept_map: HashMap<i64, i64> = HashMap::new();
    for d in repo::department::list(conn, input.source_workspace_id, None)? {
        let new_id = repo::department::create(
            conn,
            new_workspace_id,
            &DepartmentInput {
                name: d.name.clone(),
                class_name: Some(d.class_name.clone()),
                teacher: Some(d.teacher.clone()),
                days: Some(d.days.clone()),
                note: Some(d.note.clone()),
                fees: d.fees.clone(),
            },
        )?;
        dept_map.insert(d.id, new_id);
        out.departments += 1;
    }

    // ── 2. 차감 우선순위. 부서 순서는 새 id 로 바꾸고, 가져오지 못한 부서가
    //       있으면 그 줄은 빼고 1부터 다시 매긴다(순서 열이 unique 다).
    let mut st = conn.prepare(
        "SELECT department_id FROM dept_priority WHERE workspace_id = ?1 ORDER BY sort_order",
    )?;
    let old_order: Vec<i64> = st
        .query_map(params![input.source_workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let new_order: Vec<i64> = old_order
        .iter()
        .filter_map(|old| dept_map.get(old).copied())
        .collect();
    if !new_order.is_empty() {
        repo::priority::dept_save(conn, new_workspace_id, &new_order)?;
    }

    // 비용항목 순서는 항목 코드라서 그대로 옮긴다.
    let mut st = conn.prepare(
        "SELECT item_code FROM item_priority WHERE workspace_id = ?1 ORDER BY sort_order",
    )?;
    let codes: Vec<String> = st
        .query_map(params![input.source_workspace_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    if !codes.is_empty() {
        repo::priority::item_save(conn, new_workspace_id, &codes)?;
    }

    if !input.enrollments {
        log_once(conn, &dst, &src.name, &out)?;
        return Ok(out);
    }

    // ── 3. 수강중인 수강 관계만. 금액은 가져오지 않는다.
    let mut st = conn.prepare(
        "SELECT student_id, department_id FROM enrollment
          WHERE workspace_id = ?1 AND status = 'ACTIVE'
          ORDER BY id",
    )?;
    let pairs: Vec<(i64, i64)> = st
        .query_map(params![input.source_workspace_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    // 새 부서의 기준금액을 한 번만 읽어 둔다.
    let mut base_of: HashMap<i64, Vec<crate::model::Fee>> = HashMap::new();

    for (student_id, old_dept) in pairs {
        let Some(&new_dept) = dept_map.get(&old_dept) else {
            // 부서를 모두 가져왔으므로 여기 올 일이 없다. 와도 조용히 넘기지
            // 않는다 — 수강생 수가 맞지 않게 되기 때문이다.
            return Err(AppError::invalid(
                "가져올 수 없는 부서를 가리키는 수강이 있습니다. 부서정보를 확인해 주세요.",
            ));
        };

        conn.execute(
            "INSERT INTO enrollment (workspace_id, student_id, department_id)
             VALUES (?1, ?2, ?3)",
            params![new_workspace_id, student_id, new_dept],
        )?;
        let eid = conn.last_insert_rowid();
        out.enrollments += 1;

        // 금액은 **새 부서의 기준금액**에서 시작한다. 지난 기간의 학생별
        // 수정금액과 override 는 넘기지 않는다.
        let base = match base_of.get(&new_dept) {
            Some(v) => v.clone(),
            None => {
                let v = repo::department::fees_of(conn, new_dept)?;
                base_of.insert(new_dept, v.clone());
                v
            }
        };
        for it in items {
            let amount = base
                .iter()
                .find(|f| f.item_code == it.code)
                .map(|f| f.amount)
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO charge (enrollment_id, item_code, amount, is_overridden)
                 VALUES (?1, ?2, ?3, 0)",
                params![eid, it.code, amount],
            )?;
            out.charges += 1;
        }
    }

    log_once(conn, &dst, &src.name, &out)?;
    Ok(out)
}

/// 가져온 일을 **한 줄로** 남긴다.
///
/// 수강생마다 한 줄씩 쓰면 변경이력이 수백 줄로 덮인다. 그리고 그것은 이 기간에
/// 사람이 한 일이 아니라 한 번의 가져오기다.
fn log_once(
    conn: &Connection,
    dst: &crate::model::Workspace,
    source_name: &str,
    out: &WorkspaceCopyResult,
) -> AppResult<()> {
    log::write(
        conn,
        dst.year_id,
        Some(dst.id),
        log::WS_IMPORT,
        None,
        None,
        &dst.name,
        source_name,
        &format!("부서 {}개 · 수강 {}건", out.departments, out.enrollments),
        "새 작업공간 만들며 가져옴",
    )
}
