//! Excel 쓰기 — rust_xlsxwriter.
//!
//! 규칙 (설계안 10-2)
//!   * 업로드 양식의 기본 열 너비는 **10**
//!   * 금액은 `#,##0` 서식 + 오른쪽 정렬, 그 밖의 값은 가운데 정렬
//!   * 첫 행은 헤더(연한 남색 배경, 굵게), 양식에는 둘째 행에 회색 예시 한 줄

use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook, Worksheet};

use crate::error::AppResult;

pub const DEFAULT_WIDTH: f64 = 10.0;

pub struct Styles {
    pub header: Format,
    pub text: Format,
    pub money: Format,
    pub sample: Format,
    /// 마지막 합계 행 — 굵게, 연한 남색 바탕
    pub total_text: Format,
    pub total_money: Format,
}

pub fn styles() -> Styles {
    let border = Color::RGB(0x00C8D4E3);
    Styles {
        header: Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x00E4EEFB))
            .set_font_color(Color::RGB(0x001B3A6B))
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
        text: Format::new()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
        money: Format::new()
            .set_num_format("#,##0")
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
        sample: Format::new()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_font_color(Color::RGB(0x00909AAB))
            .set_italic()
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
        total_text: Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x00F3F7FD))
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
        total_money: Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x00F3F7FD))
            .set_num_format("#,##0")
            .set_align(FormatAlign::Right)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(border),
    }
}

/// 한 장짜리 표를 쓴다.
///
/// * `money_cols` — 이 열들은 숫자로 저장하고 `#,##0` 서식을 준다.
/// * `sample` — 양식 파일의 예시 행. 자료 내려받기에서는 `None`.
pub fn write_sheet(
    path: &Path,
    sheet_name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    money_cols: &[usize],
    sample: Option<&[&str]>,
) -> AppResult<PathBuf> {
    let widths = vec![DEFAULT_WIDTH; headers.len()];
    write_sheet_sized(
        path, sheet_name, headers, rows, money_cols, sample, &widths, false,
    )
}

/// 열 너비를 따로 정하고, 마지막 줄을 합계 행으로 강조할 수 있는 판.
///
/// 행정자료는 열이 많고 이름·부서명이 잘리면 곤란하므로 너비를 열마다 준다.
/// **금액은 언제나 숫자 셀로 쓴다** — 문자열로 쓰면 Excel에서 합계가 안 된다.
#[allow(clippy::too_many_arguments)]
pub fn write_sheet_sized(
    path: &Path,
    sheet_name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    money_cols: &[usize],
    sample: Option<&[&str]>,
    widths: &[f64],
    bold_last_row: bool,
) -> AppResult<PathBuf> {

    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    let s = styles();
    fill_sheet(
        sheet,
        &s,
        sheet_name,
        headers,
        rows,
        money_cols,
        sample,
        widths,
        bold_last_row,
    )?;
    book.save(path)?;
    Ok(path.to_path_buf())
}

/// 여러 장으로 된 파일의 한 장.
///
/// 학생별 합계와 부서별 상세처럼 **같은 돈을 두 가지 굵기로** 보여 줄 때
/// 파일을 둘로 나누면 대조가 번거롭다. 그래서 한 파일에 시트로 담는다.
pub struct SheetSpec<'a> {
    pub name: &'a str,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub money_cols: Vec<usize>,
    pub widths: Vec<f64>,
    /// 마지막 줄을 합계 행으로 강조할지
    pub bold_last_row: bool,
}

/// 여러 장을 한 파일에 쓴다. 차례는 준 그대로다 — 첫 장이 대표 장이다.
pub fn write_book(path: &Path, sheets: &[SheetSpec]) -> AppResult<PathBuf> {
    let mut book = Workbook::new();
    let s = styles();
    for spec in sheets {
        let sheet = book.add_worksheet();
        let head: Vec<&str> = spec.headers.iter().map(|h| h.as_str()).collect();
        fill_sheet(
            sheet,
            &s,
            spec.name,
            &head,
            &spec.rows,
            &spec.money_cols,
            None,
            &spec.widths,
            spec.bold_last_row,
        )?;
    }
    book.save(path)?;
    Ok(path.to_path_buf())
}

/// 한 장을 채운다. `write_sheet_sized`와 `write_book`이 같은 몸통을 쓴다 —
/// 서식이 시트마다 달라지면 같은 파일 안에서 보기가 어긋난다.
#[allow(clippy::too_many_arguments)]
fn fill_sheet(
    sheet: &mut Worksheet,
    s: &Styles,
    sheet_name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    money_cols: &[usize],
    sample: Option<&[&str]>,
    widths: &[f64],
    bold_last_row: bool,
) -> AppResult<()> {
    sheet.set_name(sheet_name)?;

    for (c, h) in headers.iter().enumerate() {
        sheet.write_string_with_format(0, c as u16, *h, &s.header)?;
        let w = widths.get(c).copied().unwrap_or(DEFAULT_WIDTH);
        sheet.set_column_width(c as u16, w)?;
    }
    sheet.set_row_height(0, 22)?;

    let mut r = 1u32;
    if let Some(example) = sample {
        for (c, v) in example.iter().enumerate() {
            sheet.write_string_with_format(r, c as u16, *v, &s.sample)?;
        }
        r += 1;
    }

    let last = rows.len().saturating_sub(1);
    for (i, row) in rows.iter().enumerate() {
        let total_row = bold_last_row && i == last;
        for (c, v) in row.iter().enumerate() {
            if money_cols.contains(&c) {
                let n = v.replace(',', "").trim().parse::<f64>().unwrap_or(0.0);
                let f = if total_row { &s.total_money } else { &s.money };
                sheet.write_number_with_format(r, c as u16, n, f)?;
            } else {
                let f = if total_row { &s.total_text } else { &s.text };
                sheet.write_string_with_format(r, c as u16, v, f)?;
            }
        }
        r += 1;
    }

    freeze_header(sheet)?;
    Ok(())
}

fn freeze_header(sheet: &mut Worksheet) -> AppResult<()> {
    sheet.set_freeze_panes(1, 0)?;
    Ok(())
}

/// 저장 경로를 만든다. `exports/학생정보_2026학년도_2026년4월_20260906.xlsx`
pub fn export_path(dir: &Path, base: &str, scope: &[&str]) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d");
    let mut name = base.to_string();
    for s in scope.iter().filter(|s| !s.trim().is_empty()) {
        name.push('_');
        name.push_str(&sanitize(s));
    }
    name.push('_');
    name.push_str(&stamp.to_string());
    name.push_str(".xlsx");
    Ok(dir.join(name))
}

/// 파일 이름에 쓸 수 없는 글자를 지운다.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 파일이름에_못쓰는_글자를_지운다() {
        assert_eq!(sanitize("2026년 4월"), "2026년4월");
        assert_eq!(sanitize("1/2 학기"), "12학기");
    }
}
