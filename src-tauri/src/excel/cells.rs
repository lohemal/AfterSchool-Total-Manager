//! XLSX 시트를 **셀 타입 그대로** 읽는다 — 어떤 업무 규칙도 모른다.
//!
//! `read.rs` 는 업로드 양식을 글자로 바꿔 읽는다(숫자 1 과 글자 '1' 을 같게 본다).
//! 방과후드림 결제파일처럼 **글자로 저장되어야 할 코드가 숫자로 바뀌었는지**를 가려야
//! 하는 곳에서는 그렇게 읽으면 안 된다. 그래서 타입을 지닌 채로 돌려준다.
//!
//! * XLSX 만 연다(`xls` · `ods` · `csv` 는 calamine 이 열 수 있어도 받지 않는다).
//! * 파일 바이트를 한 번만 읽어 메모리에서 연다 — 원본을 다른 곳에 복사하지 않는다.
//! * 서식만 있고 값이 없는 셀은 `Empty` 다. 시트가 선언한 범위(`A1:R621` 등)는 믿지
//!   않으므로 여기서는 셀을 있는 그대로 줄 뿐이다.

use std::io::Cursor;

use calamine::{Data, Reader, Xlsx};

/// 셀 하나. 글자 · 숫자를 섞지 않는다.
#[derive(Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64),
    /// 참/거짓 · 날짜 · 오류 셀 등 — 무엇이었는지만 남긴다
    Other(&'static str),
}

impl std::fmt::Debug for Cell {
    /// 값을 찍지 않는다 — 개인정보가 든 셀이 로그로 새지 않게.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Empty => write!(f, "Empty"),
            Cell::Text(s) => write!(f, "Text({} chars)", s.chars().count()),
            Cell::Number(_) => write!(f, "Number"),
            Cell::Other(k) => write!(f, "Other({k})"),
        }
    }
}

impl Cell {
    /// 값이 없는가 — 비어 있거나 공백뿐인 글자도 값이 없는 것으로 본다.
    pub fn is_blank(&self) -> bool {
        match self {
            Cell::Empty => true,
            Cell::Text(s) => s.trim().is_empty(),
            _ => false,
        }
    }
}

/// 읽은 시트. `rows[r][c]` 는 Excel 의 (r+1)행 · (c+1)열이다.
pub struct SheetCells {
    pub sheet_names: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// XLSX 패키지로 열 수 없다(손상 · 다른 형식)
    Corrupt,
    /// 그 이름의 시트가 없다
    SheetMissing,
}

fn cell_of(d: &Data) -> Cell {
    match d {
        Data::Empty => Cell::Empty,
        Data::String(s) => Cell::Text(s.clone()),
        Data::Float(f) => Cell::Number(*f),
        Data::Int(i) => Cell::Number(*i as f64),
        Data::Bool(_) => Cell::Other("bool"),
        Data::DateTime(_) | Data::DateTimeIso(_) => Cell::Other("date"),
        Data::DurationIso(_) => Cell::Other("duration"),
        Data::Error(_) => Cell::Other("error"),
    }
}

/// 시트 이름 목록만.
pub fn sheet_names(bytes: &[u8]) -> Result<Vec<String>, OpenError> {
    let wb: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(|_| OpenError::Corrupt)?;
    Ok(wb.sheet_names().to_vec())
}

/// 이름이 **정확히** 같은 시트 하나를 읽는다. 비슷한 이름으로 짐작하지 않는다.
pub fn read_sheet(bytes: &[u8], name: &str) -> Result<SheetCells, OpenError> {
    let mut wb: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(|_| OpenError::Corrupt)?;
    let sheet_names = wb.sheet_names().to_vec();
    if !sheet_names.iter().any(|n| n == name) {
        return Err(OpenError::SheetMissing);
    }
    let range = wb.worksheet_range(name).map_err(|_| OpenError::Corrupt)?;

    let mut rows: Vec<Vec<Cell>> = Vec::new();
    if let (Some((r0, c0)), Some((r1, c1))) = (range.start(), range.end()) {
        // 범위가 1행 · A열부터 시작하지 않을 수 있으므로 절대 위치로 옮겨 담는다
        rows = vec![vec![Cell::Empty; c1 as usize + 1]; r1 as usize + 1];
        for r in r0..=r1 {
            for c in c0..=c1 {
                if let Some(d) = range.get_value((r, c)) {
                    rows[r as usize][c as usize] = cell_of(d);
                }
            }
        }
    }
    Ok(SheetCells { sheet_names, rows })
}
