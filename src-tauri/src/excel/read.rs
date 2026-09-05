//! Excel 읽기 — calamine.
//!
//! 파싱 실패는 모두 `Result`로 올라온다. `panic!`을 쓰지 않으므로 이상한 파일을
//! 열어도 앱이 죽지 않는다 (§28).

use std::collections::HashMap;
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use crate::error::{AppError, AppResult};

/// 시트 한 장을 문자열 표로 읽는다. 첫 행은 헤더로 본다.
pub struct Sheet {
    pub headers: Vec<String>,
    /// (엑셀 기준 행 번호, 셀 값들)
    pub rows: Vec<(usize, Vec<String>)>,
    index: HashMap<String, usize>,
}

impl Sheet {
    /// 헤더 이름으로 열 위치를 찾는다. 공백과 괄호 부분은 무시한다
    /// (`학 년`, `학년(필수)` 모두 `학년`으로 본다).
    pub fn col(&self, name: &str) -> Option<usize> {
        self.index.get(&normalize(name)).copied()
    }

    pub fn require(&self, name: &str) -> AppResult<usize> {
        self.col(name).ok_or_else(|| {
            AppError::invalid(format!(
                "'{name}' 열을 찾지 못했습니다. [업로드 양식 받기]로 받은 파일을 사용해 주세요."
            ))
        })
    }

    pub fn cell<'a>(&self, row: &'a [String], col: Option<usize>) -> &'a str {
        match col.and_then(|c| row.get(c)) {
            Some(v) => v.as_str(),
            None => "",
        }
    }
}

/// 헤더 이름 정규화 — 공백·괄호 안의 내용·대소문자를 무시한다.
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '（' => depth += 1,
            ')' | ']' | '）' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            c if c.is_whitespace() => {}
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}

pub fn read_first_sheet(path: &Path) -> AppResult<Sheet> {
    let mut wb = open_workbook_auto(path).map_err(|e| {
        AppError::new("EXCEL_READ", "Excel 파일을 열지 못했습니다. 파일이 열려 있지 않은지 확인해 주세요.")
            .detail(e.to_string())
    })?;

    let name = wb
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| AppError::invalid("Excel 파일에 시트가 없습니다."))?;

    let range = wb
        .worksheet_range(&name)
        .map_err(|e| AppError::new("EXCEL_READ", "시트를 읽지 못했습니다.").detail(e.to_string()))?;

    let mut iter = range.rows();
    let header_row = iter
        .next()
        .ok_or_else(|| AppError::invalid("Excel 파일이 비어 있습니다."))?;

    let headers: Vec<String> = header_row.iter().map(text).collect();
    let mut index = HashMap::new();
    for (i, h) in headers.iter().enumerate() {
        let key = normalize(h);
        if !key.is_empty() {
            index.entry(key).or_insert(i);
        }
    }

    let mut rows = Vec::new();
    for (offset, r) in iter.enumerate() {
        let values: Vec<String> = r.iter().map(text).collect();
        if values.iter().all(|v| v.trim().is_empty()) {
            continue; // 빈 줄은 건너뛴다
        }
        rows.push((offset + 2, values)); // 엑셀 화면의 행 번호(헤더가 1행)
    }

    Ok(Sheet { headers, rows, index })
}

/// 셀 하나를 문자열로. 숫자는 소수점 없이 읽는다(학년·번호·금액 모두 정수).
pub fn text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if (f.fract()).abs() < f64::EPSILON {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) => s.trim().to_string(),
        Data::DurationIso(s) => s.trim().to_string(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

/// `'1,250'`, `' 40000 '` 같은 값을 정수로 읽는다.
pub fn parse_amount(raw: &str) -> Option<i64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '원')
        .collect();
    if cleaned.is_empty() {
        return Some(0);
    }
    cleaned.parse::<i64>().ok().or_else(|| {
        cleaned
            .parse::<f64>()
            .ok()
            .filter(|f| f.fract().abs() < f64::EPSILON)
            .map(|f| f as i64)
    })
}

/// `'3학년'`, `' 3 '` 같은 값을 정수로 읽는다.
pub fn parse_int(raw: &str) -> Option<i64> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok()
}

/// `'2026-03-01'`, `'2026.3.1'`, `'2026/03/01'`을 모두 받아 준다.
pub fn parse_date(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let norm = t.replace(['.', '/'], "-");
    let parts: Vec<&str> = norm.split('-').filter(|p| !p.is_empty()).collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(y, m, d).map(|x| x.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 헤더_정규화는_공백과_괄호를_무시한다() {
        assert_eq!(normalize("학 년"), normalize("학년"));
        assert_eq!(normalize("학년(필수)"), normalize("학년"));
        assert_eq!(normalize(" 강사료 "), normalize("강사료"));
    }

    #[test]
    fn 금액_파싱() {
        assert_eq!(parse_amount("40,000"), Some(40_000));
        assert_eq!(parse_amount(" 40000 원"), Some(40_000));
        assert_eq!(parse_amount(""), Some(0));
        assert_eq!(parse_amount("삼만원"), None);
        assert_eq!(parse_amount("-5"), Some(-5));
    }

    #[test]
    fn 정수_파싱() {
        assert_eq!(parse_int("3학년"), Some(3));
        assert_eq!(parse_int(" 12 "), Some(12));
        assert_eq!(parse_int("없음"), None);
    }

    #[test]
    fn 날짜_파싱() {
        assert_eq!(parse_date("2026-03-01").as_deref(), Some("2026-03-01"));
        assert_eq!(parse_date("2026.3.1").as_deref(), Some("2026-03-01"));
        assert_eq!(parse_date("2026/03/01").as_deref(), Some("2026-03-01"));
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("2026-13-01"), None);
    }
}
