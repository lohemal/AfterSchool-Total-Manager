//! 정산 상세내역 Excel — 수익자 · 방과후 이용권 · 자유수강권 공통 (v0.1.10).
//!
//! **이 파일은 계산하지 않는다.** `repo::settle_detail` 이 정산 스냅샷에서 펼친
//! 줄을 받아 쓰기만 한다. 세 종류는 `DetailSpec` (재원 묶음 · 파일 이름 ·
//! 시트 이름)만 다르고 쓰는 몸통은 하나다.
//!
//! ```text
//! 묶음이 둘(이용권)        학년|반|번호|이름|부서|   지원금   |   초과금   |총합계
//!                                          |강사료…|합계|강사료…|합계|
//! 묶음이 하나(수익자 등)   학년|반|번호|이름|부서|강사료|수용비|교재비|재료비|합계
//! ```
//!
//! 합계 행은 넣지 않는다. 자료 줄만 있어야 Excel 의 필터 · 피벗 · SUM 이 그대로
//! 맞는다. 금액은 모두 숫자 셀이다.

use std::path::Path;

use rusqlite::Connection;
use rust_xlsxwriter::{Workbook, Worksheet};

use crate::error::AppResult;
use crate::excel::write::{self, DEFAULT_WIDTH};
use crate::excel::{done, ExportResult};
use crate::model::CostItem;
use crate::repo;
use crate::repo::settle_detail::{DetailKind, DetailRow, DetailSpec};

const W_NAME: f64 = 11.0;
const W_DEPT: f64 = 18.0;
const W_MONEY: f64 = 12.0;

/// 학생 정보 다섯 칸. 세 종류가 모두 같다.
const LEAD: [&str; 5] = ["학년", "반", "번호", "이름", "부서"];

/// 화면의 [상세내역 받기].
///
/// 기존 Excel 다운로드와 **같은 규칙**으로 막는다 — 최신 유효 정산이 아니면
/// 파일을 만들지 않는다. 정산을 대신 만들지도 않는다. 읽기만 한다.
pub fn export(
    conn: &Connection,
    workspace_id: i64,
    kind: DetailKind,
    dir: &Path,
) -> AppResult<ExportResult> {
    repo::settle::require_fresh(conn, workspace_id)?;
    let items = repo::cost_items(conn)?;
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;
    let scope = [year.name.as_str(), ws.name.as_str()];

    let rows = repo::settle_detail::rows(conn, workspace_id, kind, &items)?;
    write_detail(&rows, &items, kind.spec(), &scope, dir)
}

pub fn write_detail(
    rows: &[DetailRow],
    items: &[CostItem],
    spec: &DetailSpec,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let path = write::export_path(dir, spec.base, scope)?;
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    fill(sheet, rows, items, spec)?;
    book.save(&path)?;
    Ok(done(path, rows.len()))
}

fn fill(
    sheet: &mut Worksheet,
    rows: &[DetailRow],
    items: &[CostItem],
    spec: &DetailSpec,
) -> AppResult<()> {
    let s = write::styles();
    sheet.set_name(spec.sheet)?;

    let grouped = spec.groups.len() > 1;
    let head_rows: u32 = if grouped { 2 } else { 1 };
    let per_group = items.len() as u16 + 1; // 항목들 + 합계
    let lead = LEAD.len() as u16;
    let last_col = lead + per_group * spec.groups.len() as u16 - 1 + u16::from(grouped);

    // ── 머리글
    for (c, h) in LEAD.iter().enumerate() {
        let c = c as u16;
        if grouped {
            sheet.merge_range(0, c, 1, c, h, &s.header)?;
        } else {
            sheet.write_string_with_format(0, c, *h, &s.header)?;
        }
    }
    let mut col = lead;
    for g in spec.groups {
        if grouped {
            sheet.merge_range(0, col, 0, col + per_group - 1, g.title, &s.header)?;
        }
        let sub = head_rows - 1;
        for (i, it) in items.iter().enumerate() {
            sheet.write_string_with_format(sub, col + i as u16, &it.name, &s.header)?;
        }
        sheet.write_string_with_format(sub, col + per_group - 1, "합계", &s.header)?;
        col += per_group;
    }
    if grouped {
        sheet.merge_range(0, last_col, 1, last_col, "총합계", &s.header)?;
    }
    for r in 0..head_rows {
        sheet.set_row_height(r, 22)?;
    }

    // ── 열 너비. 학년 · 반 · 번호는 좁아도 된다.
    for c in 0..=last_col {
        let w = match c {
            0..=2 => DEFAULT_WIDTH * 0.7,
            3 => W_NAME,
            4 => W_DEPT,
            _ => W_MONEY,
        };
        sheet.set_column_width(c, w)?;
    }

    // ── 자료
    for (i, row) in rows.iter().enumerate() {
        let r = head_rows + i as u32;
        sheet.write_string_with_format(r, 0, row.grade.to_string(), &s.text)?;
        sheet.write_string_with_format(r, 1, &row.class_no, &s.text)?;
        sheet.write_string_with_format(r, 2, row.student_no.to_string(), &s.text)?;
        sheet.write_string_with_format(r, 3, &row.name, &s.text)?;
        sheet.write_string_with_format(r, 4, &row.dept_label, &s.text)?;
        let mut c = lead;
        for g in &row.groups {
            for f in &g.fees {
                sheet.write_number_with_format(r, c, f.amount as f64, &s.money)?;
                c += 1;
            }
            sheet.write_number_with_format(r, c, g.total as f64, &s.money)?;
            c += 1;
        }
        if grouped {
            sheet.write_number_with_format(r, c, row.total as f64, &s.money)?;
        }
        sheet.set_row_height(r, 18)?;
    }

    // 머리글과 학생 다섯 칸을 고정하고, 마지막 머리글 줄에 필터를 건다
    sheet.set_freeze_panes(head_rows, lead)?;
    if !rows.is_empty() {
        sheet.autofilter(head_rows - 1, 0, head_rows - 1 + rows.len() as u32, last_col)?;
    }
    Ok(())
}
