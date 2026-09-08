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

/// 수익자 — 첫 장은 학생별 합계, 둘째 장은 학생 × 부서 상세 (요구사항 §2).
///
/// ## 왜 두 장인가
///
/// 화면이 학생별 합계로 바뀌었으므로 파일의 대표 장도 그것이어야 한다(v0.1.4).
/// 그렇다고 부서별 줄을 없앨 수는 없다 — **부서와 발생원인은 학생 한 줄에
/// 담기지 않는다.** 한 학생이 여러 부서를 듣고 원인이 섞일 수 있기 때문이다.
/// 그래서 지금까지 쓰던 부서별 형식을 둘째 장에 그대로 남긴다.
///
/// 두 장의 총액은 같다. 금액은 집계 서비스가 만든 값을 그대로 쓴다 — 여기서
/// 더하지 않는다.
pub fn write_self_pay(
    report: &crate::model::SelfPayReport,
    items: &[CostItem],
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let money = |v: i64| v.to_string();
    let fee_of = |fees: &[crate::model::Fee], code: &str| -> i64 {
        fees.iter()
            .find(|f| f.item_code == code)
            .map(|f| f.amount)
            .unwrap_or(0)
    };

    // ── 첫 장: 학생별 합계. 화면 목록과 같은 줄, 같은 차례다.
    let mut head1: Vec<String> = ["학년", "반", "번호", "이름"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    head1.extend(items.iter().map(|i| i.name.clone()));
    head1.push("합계".to_string());

    let money1: Vec<usize> = (4..4 + items.len() + 1).collect();
    let mut rows1: Vec<Vec<String>> = report
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
                row.push(money(fee_of(&r.fees, &it.code)));
            }
            row.push(money(r.total));
            row
        })
        .collect();
    if !report.rows.is_empty() {
        let mut foot = vec![
            "합계".to_string(),
            String::new(),
            String::new(),
            format!("학생 {}명", report.rows.len()),
        ];
        for it in items {
            foot.push(money(fee_of(&report.fees, &it.code)));
        }
        foot.push(money(report.total));
        rows1.push(foot);
    }

    // ── 둘째 장: 학생 × 부서. v0.1.3 까지 쓰던 형식 그대로다.
    let mut head2: Vec<String> = ["학년", "반", "번호", "이름", "부서"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    head2.extend(items.iter().map(|i| i.name.clone()));
    head2.push("합계".to_string());
    head2.push("발생원인".to_string());

    let money2: Vec<usize> = (5..5 + items.len() + 1).collect();
    let rows2: Vec<Vec<String>> = report
        .details
        .iter()
        .map(|r| {
            let mut row = vec![
                r.grade.to_string(),
                r.class_no.clone(),
                r.student_no.to_string(),
                r.name.clone(),
                r.dept_label.clone(),
            ];
            for it in items {
                row.push(money(fee_of(&r.fees, &it.code)));
            }
            row.push(money(r.total));
            row.push(origin_text(r).to_string());
            row
        })
        .collect();

    let path = write::export_path(dir, "수익자", scope)?;
    let n = report.rows.len();
    let w1 = width_plan(&[(3, W_NAME)], head1.len(), W_MONEY);
    let w2 = width_plan(
        &[(3, W_NAME), (4, W_DEPT), (head2.len() - 1, W_WIDE)],
        head2.len(),
        W_MONEY,
    );
    write::write_book(
        &path,
        &[
            write::SheetSpec {
                name: "학생별 합계",
                headers: head1,
                rows: rows1,
                money_cols: money1,
                widths: w1,
                bold_last_row: true,
            },
            write::SheetSpec {
                name: "부서별 상세",
                headers: head2,
                rows: rows2,
                money_cols: money2,
                widths: w2,
                bold_last_row: false,
            },
        ],
    )?;
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
