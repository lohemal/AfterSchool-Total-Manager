//! 행정자료 Excel 출력 (요구사항 §5·§14).
//!
//! **이 파일은 계산하지 않는다.** 집계된 결과를 받아 파일에 쓰기만 한다.
//! 학교의 품의 양식이 바뀌어도 정산 엔진과 집계 서비스는 그대로이고 여기만 고친다.
//!
//! 금액은 모두 **숫자 셀**로 쓴다 (`write_number_with_format`). 문자열로 쓰면
//! Excel에서 합계를 낼 수 없다.

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::excel::write::{self, DEFAULT_WIDTH};
use crate::excel::{done, ExportResult};
use crate::model::{CostItem, Proposal, ProgramRow, SelfPayRow};

/// 이름 열은 좁으면 잘리므로 조금 넓힌다. 금액 열은 천 단위가 들어가므로 12.
const W_NAME: f64 = 11.0;
const W_DEPT: f64 = 16.0;
const W_MONEY: f64 = 12.0;
const W_WIDE: f64 = 18.0;

/// 품의 — 부서 × 재원 (요구사항 §6).
///
/// 표 구조는 집계 결과가 정하고, 여기서는 열 너비와 서식만 얹는다.
pub fn write_proposal(p: &Proposal, dir: &Path) -> AppResult<ExportResult> {
    if !p.balanced {
        return Err(AppError::new(
            "PROPOSAL_UNBALANCED",
            "품의 금액이 정산 결과와 맞지 않아 파일을 만들지 않았습니다. 다시 정산해 주세요.",
        )
        .detail(format!(
            "부서 합계 {} / 정산 총액 {}",
            p.total.total, p.settlement_total
        )));
    }

    let mut headers: Vec<String> = vec!["부서명".to_string()];
    headers.extend(p.columns.iter().map(|c| c.label.clone()));
    headers.push("합계".to_string());
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();

    let mut rows: Vec<Vec<String>> = p
        .rows
        .iter()
        .map(|r| {
            let mut row = vec![r.dept_label.clone()];
            row.extend(r.amounts.iter().map(|a| a.to_string()));
            row.push(r.total.to_string());
            row
        })
        .collect();
    // 마지막 전체 합계 행
    let mut last = vec![p.total.dept_label.clone()];
    last.extend(p.total.amounts.iter().map(|a| a.to_string()));
    last.push(p.total.total.to_string());
    rows.push(last);

    let money: Vec<usize> = (1..headers.len()).collect();
    let widths = width_plan(&[(0, W_DEPT)], headers.len(), W_MONEY);

    let path = write::export_path(
        dir,
        &format!("품의_{}", p.item_label),
        &[&p.year_name, &p.workspace_name],
    )?;
    let n = rows.len();
    write::write_sheet_sized(
        &path,
        &format!("{} 품의", p.item_label),
        &head,
        &rows,
        &money,
        None,
        &widths,
        true,
    )?;
    Ok(done(path, n))
}

/// 수익자 — 학생 × 부서 (요구사항 §2).
pub fn write_self_pay(
    rows: &[SelfPayRow],
    items: &[CostItem],
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let mut headers: Vec<String> = ["학년", "반", "번호", "이름", "부서"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("합계".to_string());
    headers.push("발생원인".to_string());
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();

    let money_from = 5;
    let money: Vec<usize> = (money_from..money_from + items.len() + 1).collect();

    let out: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![
                r.grade.to_string(),
                r.class_no.to_string(),
                r.student_no.to_string(),
                r.name.clone(),
                r.dept_label.clone(),
            ];
            for it in items {
                row.push(
                    r.fees
                        .iter()
                        .find(|f| f.item_code == it.code)
                        .map(|f| f.amount)
                        .unwrap_or(0)
                        .to_string(),
                );
            }
            row.push(r.total.to_string());
            row.push(origin_text(r).to_string());
            row
        })
        .collect();

    let widths = width_plan(
        &[(3, W_NAME), (4, W_DEPT), (headers.len() - 1, W_WIDE)],
        headers.len(),
        W_MONEY,
    );
    let path = write::export_path(dir, "수익자", scope)?;
    let n = out.len();
    write::write_sheet_sized(&path, "수익자", &head, &out, &money, None, &widths, true)?;
    Ok(done(path, n))
}

fn origin_text(r: &SelfPayRow) -> &'static str {
    if r.origin_voucher > 0 {
        "이용권 소진 후 발생"
    } else if r.origin_free > 0 {
        "자유수강권 소진 후 발생"
    } else {
        "일반 수익자"
    }
}

/// 방과후 이용권 · 자유수강권 (요구사항 §3·§4).
///
/// `with_over`가 참이면 초과금 열을 함께 쓴다. 자유수강권은 초과분이
/// 수익자로 넘어가므로 열이 없다.
pub fn write_program(
    rows: &[ProgramRow],
    items: &[CostItem],
    title: &str,
    with_over: bool,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let mut headers: Vec<String> = ["학년", "반", "번호", "이름"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("사용 합계".to_string());
    if with_over {
        headers.extend(items.iter().map(|i| format!("초과 {}", i.name)));
        headers.push("초과 합계".to_string());
    }
    headers.extend(
        [
            "지원기간",
            "연간 한도",
            "기간 한도",
            "이월액",
            "이전 사용액",
            "현재 사용액",
            "기간 잔액",
            "연간 누적",
            "연간 잔액",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();

    // 금액 열 위치를 계산해 둔다 (지원기간 이름만 문자열이다)
    let mut money: Vec<usize> = Vec::new();
    let used_from = 4;
    for i in used_from..used_from + items.len() + 1 {
        money.push(i);
    }
    let mut cursor = used_from + items.len() + 1;
    if with_over {
        for i in cursor..cursor + items.len() + 1 {
            money.push(i);
        }
        cursor += items.len() + 1;
    }
    let period_col = cursor; // 지원기간 이름 — 숫자가 아니다
    for i in period_col + 1..headers.len() {
        money.push(i);
    }

    let out: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![
                r.grade.to_string(),
                r.class_no.to_string(),
                r.student_no.to_string(),
                r.name.clone(),
            ];
            for it in items {
                row.push(
                    r.used
                        .iter()
                        .find(|f| f.item_code == it.code)
                        .map(|f| f.amount)
                        .unwrap_or(0)
                        .to_string(),
                );
            }
            row.push(r.used_total.to_string());
            if with_over {
                for it in items {
                    row.push(
                        r.over
                            .iter()
                            .find(|f| f.item_code == it.code)
                            .map(|f| f.amount)
                            .unwrap_or(0)
                            .to_string(),
                    );
                }
                row.push(r.over_total.to_string());
            }
            match &r.budget {
                Some(b) => {
                    row.push(if b.period_name.is_empty() {
                        "연간".to_string()
                    } else {
                        b.period_name.clone()
                    });
                    row.push(b.annual_limit.to_string());
                    row.push(b.period_limit.to_string());
                    row.push(b.carry_in.to_string());
                    row.push(b.used_in_period_before.to_string());
                    row.push(b.used_now.to_string());
                    row.push(b.period_left.to_string());
                    row.push(b.annual_used.to_string());
                    row.push(b.annual_left.to_string());
                }
                None => {
                    row.push(String::new());
                    for _ in 0..8 {
                        row.push("0".to_string());
                    }
                }
            }
            row
        })
        .collect();

    let widths = width_plan(&[(3, W_NAME), (period_col, 12.0)], headers.len(), W_MONEY);
    let path = write::export_path(dir, title, scope)?;
    let n = out.len();
    write::write_sheet_sized(&path, title, &head, &out, &money, None, &widths, true)?;
    Ok(done(path, n))
}

/// 열 너비 계획 — 지정하지 않은 열은 기본값을 쓴다.
fn width_plan(special: &[(usize, f64)], count: usize, money: f64) -> Vec<f64> {
    let mut out = vec![money; count];
    // 학년·반·번호는 좁아도 된다
    for (i, w) in out.iter_mut().enumerate().take(count) {
        if i < 3 {
            *w = DEFAULT_WIDTH * 0.7;
        }
    }
    for (i, w) in special {
        if *i < count {
            out[*i] = *w;
        }
    }
    out
}
