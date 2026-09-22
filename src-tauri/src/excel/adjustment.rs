//! 추가징수 · 환불 Excel — 한 파일에 세 장 (v0.1.5).
//!
//! ```text
//! Sheet 1  추가징수 명단   학생당 한 줄
//! Sheet 2  환불 명단       학생당 한 줄
//! Sheet 3  상세 내역       건별. '구분' 열로 둘을 가른다
//! ```
//!
//! **이 파일은 계산하지 않는다.** 집계 서비스(`repo::adjustment`)가 만든 결과를
//! 받아 쓰기만 한다. 그래서 화면에서 본 숫자와 파일의 숫자가 다를 수 없다.
//!
//! ## 확인이 필요한 기록이 있으면 만들지 않는다
//!
//! 이 파일은 실제로 돈을 더 걷거나 돌려주는 데 쓴다. 원본 금액이 달라진 줄이
//! 섞여 있으면 경고만 띄우고 내보내는 것으로는 부족하다 — 그 파일이 이미 밖으로
//! 나간 뒤에는 되돌릴 수 없다. 그래서 **하나라도 있으면 아예 만들지 않는다.**

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::excel::write::{self, DEFAULT_WIDTH};
use crate::excel::{done, ExportResult};
use crate::model::{AdjustmentReport, AdjustmentView, CostItem, Fee};

const W_NAME: f64 = 11.0;
const W_DEPT: f64 = 16.0;
const W_MONEY: f64 = 12.0;
const W_DATE: f64 = 12.0;
const W_NOTE: f64 = 22.0;

fn fee_of(fees: &[Fee], code: &str) -> i64 {
    fees.iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap_or(0)
}

/// 학생별 합계 한 장 — 추가징수와 환불이 같은 모양이다.
fn student_sheet<'a>(
    name: &'a str,
    report: &AdjustmentReport,
    items: &[CostItem],
) -> write::SheetSpec<'a> {
    let mut headers: Vec<String> = ["학년", "반", "번호", "이름"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("합계".to_string());

    let money: Vec<usize> = (4..4 + items.len() + 1).collect();
    let mut rows: Vec<Vec<String>> = report
        .rows
        .iter()
        .map(|r| {
            let mut row = vec![
                r.grade.to_string(),
                r.class_no.clone(),
                r.student_no.to_string(),
                r.name.clone(),
            ];
            for it in items {
                row.push(fee_of(&r.fees, &it.code).to_string());
            }
            row.push(r.total.to_string());
            row
        })
        .collect();

    // 합계 줄 — 집계 서비스가 이미 더한 값을 그대로 적는다.
    if !report.rows.is_empty() {
        let mut foot = vec![
            "합계".to_string(),
            String::new(),
            String::new(),
            format!("학생 {}명", report.students),
        ];
        for it in items {
            foot.push(fee_of(&report.fees, &it.code).to_string());
        }
        foot.push(report.total.to_string());
        rows.push(foot);
    }

    let widths = width_plan(&[(3, W_NAME)], headers.len());
    write::SheetSpec {
        name,
        headers,
        rows,
        money_cols: money,
        widths,
        bold_last_row: true,
    }
}

/// 상세 한 장 — 두 종류를 `구분` 열로 가른다.
fn detail_sheet<'a>(name: &'a str, view: &AdjustmentView, items: &[CostItem]) -> write::SheetSpec<'a> {
    let mut headers: Vec<String> = ["구분", "학년", "반", "번호", "이름", "부서"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("합계".to_string());
    headers.push("발생일".to_string());
    headers.push("사유".to_string());

    let money_from = 6;
    let money: Vec<usize> = (money_from..money_from + items.len() + 1).collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (label, report) in [
        ("추가징수", &view.additional),
        ("환불", &view.refund),
    ] {
        for a in &report.details {
            let mut row = vec![
                label.to_string(),
                a.grade.to_string(),
                a.class_no.clone(),
                a.student_no.to_string(),
                a.name.clone(),
                a.dept_label.clone(),
            ];
            for it in items {
                row.push(
                    a.fees
                        .iter()
                        .find(|f| f.item_code == it.code)
                        .map(|f| f.amount)
                        .unwrap_or(0)
                        .to_string(),
                );
            }
            row.push(a.total.to_string());
            row.push(a.occurred_on.clone());
            row.push(a.note.clone());
            rows.push(row);
        }
    }

    let last = headers.len() - 1;
    let widths = width_plan(&[(4, W_NAME), (5, W_DEPT), (last - 1, W_DATE), (last, W_NOTE)], headers.len());
    write::SheetSpec {
        name,
        headers,
        rows,
        money_cols: money,
        widths,
        bold_last_row: false,
    }
}

fn width_plan(special: &[(usize, f64)], count: usize) -> Vec<f64> {
    let mut out = vec![W_MONEY; count];
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

/// 한 파일 · 세 장.
///
/// 한쪽 명단이 비어 있어도 파일은 만든다 — 추가징수만 있고 환불이 없는 달이
/// 흔하기 때문이다. 그때 그 장은 머리글만 있는 빈 장이 된다.
pub fn write_adjustments(
    view: &AdjustmentView,
    items: &[CostItem],
    scope: &[&str],
    cond: &str,
    dir: &Path,
) -> AppResult<ExportResult> {
    let pending = view.additional.needs_check + view.refund.needs_check;
    if pending > 0 {
        return Err(AppError::new(
            "ADJUSTMENT_NEEDS_CHECK",
            format!(
                "금액 변경 확인이 필요한 내역이 {pending}건 있습니다. \
                 변경내역을 확인한 뒤 Excel을 만들어 주세요."
            ),
        ));
    }

    let mut name_scope: Vec<&str> = scope.to_vec();
    if !cond.trim().is_empty() {
        name_scope.push(cond);
    }
    let path = write::export_path(dir, "추가징수환불", &name_scope)?;

    let n = view.additional.rows.len() + view.refund.rows.len();
    write::write_book(
        &path,
        &[
            student_sheet("추가징수 명단", &view.additional, items),
            student_sheet("환불 명단", &view.refund, items),
            detail_sheet("상세 내역", view, items),
        ],
    )?;
    Ok(done(path, n))
}
