//! 품의 집계 (요구사항 §6~§13).
//!
//! ## 이 파일이 하는 일과 하지 않는 일
//!
//! * **한다** — 최신 유효 정산의 `settlement_alloc`을 **부서명** × 재원으로 모으고,
//!   그 학년도의 대상학년에서 열 이름표를 만든다. 같은 부서명의 A반·B반은 한 줄이다.
//! * **하지 않는다** — 금액을 다시 계산하지 않는다. 원본 `charge`를 보지 않는다.
//!   정산이 없거나 낡았으면 아예 만들지 않는다.
//!
//! Excel writer는 이 결과를 받아 파일에 쓰기만 한다. 학교 양식이 바뀌어도
//! 이 파일은 그대로다 (요구사항 §14).

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::domain::{parse_grades, Fund};
use crate::error::{AppError, AppResult};
use crate::model::{CostItem, Proposal, ProposalColumn, ProposalKind, ProposalRow};
use crate::repo;

/// 품의 열 차례 — 요구사항 §6의 표 그대로.
///
/// **일반 수익자와 이용권 초과금을 합치지 않는다.** 학교 회계에서 재원이 다르다.
const FUND_ORDER: [Fund; 4] = [
    Fund::SelfPay,
    Fund::VoucherOver,
    Fund::Voucher,
    Fund::FreeVoucher,
];

/// 품의로 뽑을 수 있는 종류. 교재·재료비 통합은 두 항목을 합산한다.
pub fn kinds(items: &[CostItem]) -> Vec<ProposalKind> {
    let mut out: Vec<ProposalKind> = items
        .iter()
        .map(|i| ProposalKind {
            key: i.code.clone(),
            label: i.name.clone(),
            item_codes: vec![i.code.clone()],
        })
        .collect();

    // 교재비와 재료비가 둘 다 있으면 통합 선택을 더한다.
    let has = |code: &str| items.iter().any(|i| i.code == code);
    if has("TEXTBOOK") && has("MATERIAL") {
        let name = |code: &str| {
            items
                .iter()
                .find(|i| i.code == code)
                .map(|i| i.name.clone())
                .unwrap_or_default()
        };
        out.push(ProposalKind {
            key: "TEXTBOOK+MATERIAL".to_string(),
            label: format!("{}·{}", name("TEXTBOOK"), name("MATERIAL")),
            item_codes: vec!["TEXTBOOK".to_string(), "MATERIAL".to_string()],
        });
    }
    out
}

pub fn kind_of(items: &[CostItem], key: &str) -> AppResult<ProposalKind> {
    kinds(items)
        .into_iter()
        .find(|k| k.key == key)
        .ok_or_else(|| AppError::invalid("알 수 없는 품의 종류입니다."))
}

/// `3학년` 꼴로 만든다. 대상학년이 여럿이면 `3·4학년`.
fn grade_text(grades: &[i64]) -> String {
    if grades.is_empty() {
        return String::new();
    }
    format!(
        "{}학년",
        grades
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>()
            .join("·")
    )
}

/// 열 이름표를 그 학년도의 **대상학년에서 만든다** (요구사항 §9).
///
/// 2026학년도에 대상학년이 `3`이고 전교생이 1~6학년이면
/// `수익자(1,2,4,5,6학년) · 3학년 초과금 · 3학년 지원금 · 자유수강권`가 된다.
/// 대상학년이 `3,4`로 바뀌면 이름표가 저절로 따라 바뀐다.
pub fn columns(conn: &Connection, year_id: i64) -> AppResult<Vec<ProposalColumn>> {
    let (targets, label_fund, label_over): (String, String, String) = conn.query_row(
        "SELECT target_grades, label_fund, label_over
           FROM support_policy WHERE year_id = ?1 AND program = 'VOUCHER'",
        params![year_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let targets = parse_grades(&targets);

    // 전교생에 실제로 있는 학년에서 대상학년을 뺀다
    let mut st = conn.prepare(
        "SELECT DISTINCT grade FROM student WHERE year_id = ?1 ORDER BY grade",
    )?;
    let all: Vec<i64> = st
        .query_map(params![year_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let rest: Vec<i64> = all.into_iter().filter(|g| !targets.contains(g)).collect();

    let self_label = if !label_fund.trim().is_empty() {
        label_fund.trim().to_string()
    } else if targets.is_empty() || rest.is_empty() {
        "수익자".to_string()
    } else {
        format!(
            "수익자({}학년)",
            rest.iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    };

    let target_label = grade_text(&targets);
    let over_label = if !label_over.trim().is_empty() {
        label_over.trim().to_string()
    } else if target_label.is_empty() {
        "이용권 초과금".to_string()
    } else {
        format!("{target_label} 초과금")
    };
    let voucher_label = if target_label.is_empty() {
        "이용권 지원금".to_string()
    } else {
        format!("{target_label} 지원금")
    };

    // 열 차례는 `FUND_ORDER` 하나가 정한다. 이름표만 학년도별로 달라진다.
    Ok(FUND_ORDER
        .iter()
        .map(|f| ProposalColumn {
            fund: f.code().to_string(),
            label: match f {
                Fund::SelfPay => self_label.clone(),
                Fund::VoucherOver => over_label.clone(),
                Fund::Voucher => voucher_label.clone(),
                Fund::FreeVoucher => "자유수강권".to_string(),
            },
        })
        .collect())
}

/// 품의 집계를 만든다.
///
/// **최신 유효 정산이 없으면 오류다.** 낡은 정산으로 행정자료를 내보내는 것이
/// 이 프로그램에서 가장 위험한 일이기 때문이다 (요구사항 §1).
pub fn build(
    conn: &Connection,
    workspace_id: i64,
    kind_key: &str,
    items: &[CostItem],
) -> AppResult<Proposal> {
    let settlement_id = repo::settle::require_fresh(conn, workspace_id)?;
    let kind = kind_of(items, kind_key)?;

    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;
    let cols = columns(conn, ws.year_id)?;

    let placeholders = kind
        .item_codes
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");
    // 부서명 하나로 묶는다 — `로봇과학A반`과 `로봇과학B반`이 품의에서는 한 줄이다.
    // 반을 가르는 것은 부서정보·수강생 명단의 일이고, 품의는 학교 회계 단위인
    // 부서명으로 올린다. 원본 `department` 행은 그대로 둔다.
    let sql = format!(
        "SELECT d.name, MIN(a.department_id), a.fund, SUM(a.amount)
           FROM settlement_alloc a
           JOIN department d ON d.id = a.department_id
          WHERE a.settlement_id = ?1 AND a.item_code IN ({placeholders})
          GROUP BY d.name, a.fund
          ORDER BY d.name"
    );

    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(settlement_id)];
    for c in &kind.item_codes {
        args.push(Box::new(c.clone()));
    }
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();

    let mut st = conn.prepare(&sql)?;
    let raw = st
        .query_map(refs.as_slice(), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    // 부서명별로 모은다. 재원 구분은 `fund`만 본다 (요구사항 §8).
    let mut order: Vec<String> = Vec::new();
    let mut first_id: HashMap<String, i64> = HashMap::new();
    let mut cells: HashMap<(String, String), i64> = HashMap::new();
    for (name, dept_id, fund, amount) in raw {
        if !first_id.contains_key(&name) {
            order.push(name.clone());
            first_id.insert(name.clone(), dept_id);
        }
        *cells.entry((name, fund)).or_insert(0) += amount;
    }

    let mut rows: Vec<ProposalRow> = Vec::new();
    let mut total_amounts = vec![0i64; cols.len()];
    for name in order {
        let amounts: Vec<i64> = cols
            .iter()
            .map(|c| {
                cells
                    .get(&(name.clone(), c.fund.clone()))
                    .copied()
                    .unwrap_or(0)
            })
            .collect();
        let total: i64 = amounts.iter().sum();
        if total == 0 {
            continue; // 이 항목에 금액이 없는 부서는 품의에 넣지 않는다
        }
        for (i, v) in amounts.iter().enumerate() {
            total_amounts[i] += v;
        }
        rows.push(ProposalRow {
            // 부서명으로 묶었으므로 이 값은 그 묶음의 대표 부서 id 다.
            // 화면에서 줄을 가리는 열쇠로만 쓰고, 부서를 지목하는 데는 쓰지 않는다.
            department_id: first_id.get(&name).copied().unwrap_or(0),
            dept_label: name,
            amounts,
            total,
        });
    }

    let grand = ProposalRow {
        department_id: 0,
        dept_label: "합계".to_string(),
        total: total_amounts.iter().sum(),
        amounts: total_amounts,
    };

    // 교차검증 — 고른 항목의 정산 총액과 견준다 (요구사항 §12)
    let sql = format!(
        "SELECT COALESCE(SUM(amount), 0) FROM settlement_alloc
          WHERE settlement_id = ?1 AND item_code IN ({placeholders})"
    );
    let settlement_total: i64 = conn.query_row(&sql, refs.as_slice(), |r| r.get(0))?;

    // 부서별 행 합계도 각각 확인한다
    let rows_ok = rows.iter().all(|r| r.amounts.iter().sum::<i64>() == r.total);
    let settled_at: String = conn.query_row(
        "SELECT created_at FROM settlement WHERE id = ?1",
        params![settlement_id],
        |r| r.get(0),
    )?;

    Ok(Proposal {
        year_name: year.name,
        workspace_name: ws.name,
        item_label: kind.label,
        item_codes: kind.item_codes,
        columns: cols,
        balanced: rows_ok && grand.total == settlement_total,
        rows,
        total: grand,
        settlement_total,
        settled_at,
    })
}
