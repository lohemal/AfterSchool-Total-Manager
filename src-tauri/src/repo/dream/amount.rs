//! 방과후드림 기준 금액 — 최신 **유효** 정산에서 학생별로 읽는다. 아무것도 고치지 않는다.
//!
//! ```text
//! V        = 이 학생의 VOUCHER 합계          → P열(이용권)에 나눌 총액
//! F        = 이 학생의 FREE_VOUCHER 합계     → Q열(자유수강권)에 나눌 총액
//! T        = 이 학생의 배분 전체 합계        = 정산에 들어간 charge 합계
//! personal = T − V − F = SELF_PAY + VOUCHER_OVER → O열(개인부담금)
//! ```
//!
//! `VOUCHER_OVER` 는 이용권 대상자가 낼 돈이다. 지원금으로 다시 더하지 않는다.
//!
//! 정산이 FRESH 가 아니면(NONE · STALE_DATA · STALE_YEAR · STALE_PRIOR) 값을 주지 않는다.
//! 금액은 학생 정산 상세 팝업과 같은 함수(`settle::student_allocs`)로 읽고, 한도 기록
//! (`settlement_budget.used_now`)과 맞대어 본다. 둘이 어긋나면 고쳐 맞추지 않고 오류다.

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::domain::Program;
use crate::error::{AppError, AppResult};
use crate::repo;
use crate::repo::settle;

/// 기준으로 삼은 정산 — 처리 이력에 그대로 적고, 내보내기 직전에 다시 맞대어 본다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshSettlement {
    pub workspace_id: i64,
    pub settlement_id: i64,
    pub created_at: String,
    pub ws_data_version: i64,
    pub year_data_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudentAmount {
    pub student_id: i64,
    /// T
    pub total: i64,
    /// V
    pub voucher: i64,
    /// F
    pub free_voucher: i64,
    /// T − V − F
    pub personal: i64,
    pub self_pay: i64,
    pub voucher_over: i64,
}

/// 최신 정산이 지금 유효한가. 유효하지 않으면 기존 Excel 과 같은 문장으로 거절한다.
pub fn fresh_settlement(conn: &Connection, workspace_id: i64) -> AppResult<FreshSettlement> {
    let settlement_id = settle::require_fresh(conn, workspace_id)?;
    let (latest, created_at) = settle::latest_id(conn, workspace_id)?
        .ok_or_else(|| AppError::not_found("정산 결과를 찾지 못했습니다."))?;
    if latest != settlement_id {
        return Err(AppError::invalid("정산이 바뀌는 중입니다. 잠시 후 다시 시도해 주세요."));
    }
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;
    Ok(FreshSettlement {
        workspace_id,
        settlement_id,
        created_at,
        ws_data_version: ws.data_version,
        year_data_version: year.data_version,
    })
}

fn conflict(what: &str) -> AppError {
    AppError::new(
        "DREAM_AMOUNT_CONFLICT",
        "정산 결과와 지원금 기록이 서로 맞지 않습니다. 정산을 다시 만든 뒤 시도해 주세요.",
    )
    .detail(what.to_string())
}

/// 한 학생의 기준 금액. `fresh` 는 `fresh_settlement` 로 막 확인한 것이어야 한다.
fn compute(
    conn: &Connection,
    fresh: &FreshSettlement,
    student_id: i64,
    voucher_used: &std::collections::HashMap<i64, crate::model::BudgetView>,
    free_used: &std::collections::HashMap<i64, crate::model::BudgetView>,
) -> AppResult<StudentAmount> {
    let mut a = StudentAmount {
        student_id,
        total: 0,
        voucher: 0,
        free_voucher: 0,
        personal: 0,
        self_pay: 0,
        voucher_over: 0,
    };
    // 학생 정산 상세 팝업과 같은 함수
    for r in settle::student_allocs(conn, fresh.workspace_id, student_id)? {
        a.total += r.amount;
        match r.fund.as_str() {
            "VOUCHER" => a.voucher += r.amount,
            "FREE_VOUCHER" => a.free_voucher += r.amount,
            "SELF_PAY" => a.self_pay += r.amount,
            "VOUCHER_OVER" => a.voucher_over += r.amount,
            other => return Err(conflict(&format!("알 수 없는 재원 {other}"))),
        }
    }
    a.personal = a.total - a.voucher - a.free_voucher;
    if a.personal != a.self_pay + a.voucher_over {
        return Err(conflict("T-V-F != SELF_PAY+VOUCHER_OVER"));
    }

    // 한도 기록과 맞대어 본다. 기록이 없으면 그 제도를 한 푼도 쓰지 않은 것이어야 한다.
    let used = |m: &std::collections::HashMap<i64, crate::model::BudgetView>| {
        m.get(&student_id).map(|b| b.used_now).unwrap_or(0)
    };
    if a.voucher != used(voucher_used) {
        return Err(conflict(&format!("학생 {student_id} VOUCHER != used_now")));
    }
    if a.free_voucher != used(free_used) {
        return Err(conflict(&format!("학생 {student_id} FREE_VOUCHER != used_now")));
    }
    Ok(a)
}

/// 한 학생의 기준 금액. 정산이 유효하지 않으면 거절한다.
pub fn student_amount(conn: &Connection, workspace_id: i64, student_id: i64) -> AppResult<(FreshSettlement, StudentAmount)> {
    let fresh = fresh_settlement(conn, workspace_id)?;
    let v = settle::budget_map(conn, fresh.settlement_id, Program::Voucher)?;
    let f = settle::budget_map(conn, fresh.settlement_id, Program::FreeVoucher)?;
    let a = compute(conn, &fresh, student_id, &v, &f)?;
    Ok((fresh, a))
}

/// 정산에 들어간 모든 학생의 기준 금액(학생 id 차례).
pub fn workspace_amounts(conn: &Connection, workspace_id: i64) -> AppResult<(FreshSettlement, Vec<StudentAmount>)> {
    let fresh = fresh_settlement(conn, workspace_id)?;
    let v = settle::budget_map(conn, fresh.settlement_id, Program::Voucher)?;
    let f = settle::budget_map(conn, fresh.settlement_id, Program::FreeVoucher)?;

    let mut ids: Vec<i64> = conn
        .prepare("SELECT DISTINCT student_id FROM settlement_alloc WHERE settlement_id = ?1")?
        .query_map(params![fresh.settlement_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    // 배분은 없는데 한도 기록만 있는 학생 — 쓴 돈이 0 이어야 한다
    let seen: HashSet<i64> = ids.iter().copied().collect();
    for (id, b) in v.iter().chain(f.iter()) {
        if !seen.contains(id) && b.used_now != 0 {
            return Err(conflict(&format!("학생 {id} 배분 없이 used_now")));
        }
    }
    ids.sort_unstable();
    let out = ids
        .into_iter()
        .map(|id| compute(conn, &fresh, id, &v, &f))
        .collect::<AppResult<Vec<_>>>()?;
    Ok((fresh, out))
}
