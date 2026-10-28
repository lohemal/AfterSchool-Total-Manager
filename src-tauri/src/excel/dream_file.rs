//! 방과후드림 결제파일 읽기 — 시트 · 머리글 · 실제 결제행 · 셀 타입.
//!
//! ```text
//! 파일   .xlsx 만, XLSX 패키지로 열려야 한다
//! 시트   `정산내역` 하나뿐(다른 시트가 있으면 오류 — 어느 것을 쓸지 짐작하지 않는다)
//! 1행    A~R 머리글이 정확히 이 차례(앞뒤 공백만 떼고 비교). 이름으로 열을 옮겨 맞추지 않는다
//! 2행~   값이 하나라도 있는 행이 결제행. 서식만 있는 셀은 빈 셀이다.
//!        마지막 결제행 뒤의 빈 행은 몇 줄이든 무시, 결제행 사이의 빈 행은 오류
//! R 오른쪽에 값이 있으면 오류
//! ```
//!
//! 글자로 저장되어야 할 칸(기관코드 · 개인참조번호 · 생년월일 · 강좌명코드 · 항목구분코드 ·
//! 이체예정일 등)이 숫자 셀이면 글자로 바꿔 맞추지 않고 오류다 — 숫자가 되는 순간 앞자리
//! 0 이 사라졌을 수 있기 때문이다. 반대로 금액 칸이 글자면 그것도 오류다.

use crate::domain::dream::{self, HEADERS};
use crate::domain::dream_file::{code, check_common, DreamFile, DreamRow, Issue, Secret};
use crate::excel::cells::{self, Cell, OpenError};

/// 시트 이름(정확히 이것).
pub const SHEET: &str = "정산내역";
/// 받는 파일 크기 상한 — 결제파일은 수백 KB 다. 엉뚱한 큰 파일을 메모리에 올리지 않는다.
pub const MAX_BYTES: usize = 20 * 1024 * 1024;

const STRING_COLS: [usize; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 17];
/// 비어 있으면 안 되는 칸 — A~N. O·P·Q(원본 배분)와 R(이체예정일)은 비어 있어도 된다.
const REQUIRED: std::ops::Range<usize> = 0..14;

/// 파일 이름과 바이트를 받아 읽는다. 무엇이 틀렸든 오류를 모아 돌려준다(패닉 없음).
pub fn parse(file_name: &str, bytes: &[u8]) -> DreamFile {
    let mut out = DreamFile::default();
    let lower = file_name.to_lowercase();
    if !lower.ends_with(".xlsx") {
        out.file_issues.push(Issue::error(code::FILE_NOT_XLSX));
        return out;
    }
    if bytes.len() > MAX_BYTES {
        out.file_issues.push(Issue::error(code::FILE_TOO_LARGE));
        return out;
    }
    let sheet = match cells::read_sheet(bytes, SHEET) {
        Ok(s) => s,
        Err(OpenError::Corrupt) => {
            out.file_issues.push(Issue::error(code::WORKBOOK_CORRUPT));
            return out;
        }
        Err(OpenError::SheetMissing) => {
            out.file_issues.push(Issue::error(code::SHEET_MISSING).expected(SHEET));
            return out;
        }
    };
    out.sheet_name = Some(SHEET.to_string());
    let others: Vec<&String> = sheet.sheet_names.iter().filter(|n| n.as_str() != SHEET).collect();
    if !others.is_empty() {
        let names = others.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        out.file_issues.push(Issue::error(code::EXTRA_SHEET).found(names));
    }

    let rows = &sheet.rows;
    let cell = |r: usize, c: usize| rows.get(r).and_then(|row| row.get(c)).unwrap_or(&Cell::Empty);

    // ── R 오른쪽 값 (머리글 줄 포함)
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate().skip(HEADERS.len()) {
            if !v.is_blank() {
                out.file_issues.push(Issue::error(code::EXTRA_DATA_COLUMN).row(r as u32 + 1).found(col_letter(c)));
            }
        }
    }

    // ── 머리글. 틀리면 열을 믿을 수 없으므로 줄을 읽지 않는다.
    let mut header_ok = true;
    for (c, want) in HEADERS.iter().enumerate() {
        let got = match cell(0, c) {
            Cell::Text(s) => s.trim().to_string(),
            Cell::Empty => String::new(),
            other => format!("{other:?}"),
        };
        if got != *want {
            header_ok = false;
            out.file_issues.push(Issue::error(code::HEADER_MISMATCH).at(1, c).expected(*want).found(got));
        }
    }
    if !header_ok {
        return out;
    }

    // ── 실제 결제행. 값이 하나라도 있는 행만.
    let blank = |r: usize| (0..HEADERS.len()).all(|c| cell(r, c).is_blank());
    let data: Vec<usize> = (1..rows.len()).filter(|r| !blank(*r)).collect();
    let (Some(&first), Some(&last)) = (data.first(), data.last()) else {
        out.file_issues.push(Issue::error(code::EMPTY_DATA));
        return out;
    };
    for r in first..=last {
        if blank(r) {
            out.file_issues.push(Issue::error(code::INTERNAL_EMPTY_ROW).row(r as u32 + 1));
        }
    }

    for r in data {
        let row: Vec<&Cell> = (0..HEADERS.len()).map(|c| cell(r, c)).collect();
        out.rows.push(parse_row(r as u32 + 1, &row));
    }
    check_common(&mut out);
    out
}

fn col_letter(c: usize) -> String {
    // A..Z, AA.. (R 오른쪽 몇 칸만 쓰이므로 두 글자면 충분하다)
    let mut n = c + 1;
    let mut s = String::new();
    while n > 0 {
        let m = (n - 1) % 26;
        s.insert(0, (b'A' + m as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// 정수 원 · 0 이상. 숫자 셀만.
fn money(v: &Cell) -> Result<Option<i64>, ()> {
    match v {
        Cell::Empty => Ok(None),
        Cell::Number(f) if f.fract() == 0.0 && *f >= 0.0 && *f <= 1e12 => Ok(Some(*f as i64)),
        _ => Err(()),
    }
}

fn text(v: &Cell) -> String {
    match v {
        Cell::Text(s) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// 화면에 보일 글자. 숫자는 그대로(202609, 35000), 글자는 원래대로.
fn shown(v: &Cell) -> String {
    match v {
        Cell::Empty => String::new(),
        Cell::Text(s) => s.clone(),
        Cell::Number(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        Cell::Number(f) => f.to_string(),
        Cell::Other(k) => format!("({k})"),
    }
}

fn parse_row(source_row: u32, cells: &[&Cell]) -> DreamRow {
    let mut r = DreamRow {
        source_row,
        ..Default::default()
    };
    let issue = |code: &'static str, c: usize| Issue::error(code).at(source_row, c);

    // 화면용 18칸 — C · H 는 여기서 가린다. 원래 값은 아래 Secret 에만.
    r.display = cells.iter().map(|c| shown(c)).collect();
    r.display[2] = dream::mask_ref(&r.display[2]);
    r.display[7] = dream::mask_birth(&r.display[7]);

    // 1) 필수 · 타입. 타입이 틀린 칸은 값 검사를 하지 않는다.
    let mut bad = [false; 18];
    for c in REQUIRED {
        if cells[c].is_blank() {
            r.issues.push(issue(code::REQUIRED_VALUE_MISSING, c));
            bad[c] = true;
        }
    }
    for c in 0..18 {
        if bad[c] || cells[c].is_blank() {
            continue;
        }
        let ok = if STRING_COLS.contains(&c) {
            matches!(cells[c], Cell::Text(_))
        } else {
            matches!(cells[c], Cell::Number(_))
        };
        if !ok {
            let want = if STRING_COLS.contains(&c) { "TEXT" } else { "NUMBER" };
            r.issues.push(issue(code::INVALID_CELL_TYPE, c).expected(want));
            bad[c] = true;
        }
    }

    // 2) 값
    if !bad[0] {
        let v = text(cells[0]);
        if v.len() == 4 && v.bytes().all(|b| b.is_ascii_digit()) {
            r.accounting_year = Some(v);
        } else {
            r.issues.push(issue(code::INVALID_ACCOUNTING_YEAR, 0));
        }
    }
    if !bad[1] {
        match dream::check_org_code(&text(cells[1])) {
            Ok(v) => r.org_code = Some(v),
            Err(_) => r.issues.push(issue(code::INVALID_ORG_CODE, 1)),
        }
    }
    if !bad[2] {
        match dream::check_ref(&text(cells[2])) {
            Ok(v) => r.personal_ref = Some(Secret::new(v)),
            Err(_) => r.issues.push(issue(code::INVALID_PERSONAL_REF, 2)),
        }
    }
    if !bad[3] {
        r.grade_raw = text(cells[3]);
        r.grade_candidate = dream::grade_candidate(&r.grade_raw);
        if r.grade_candidate.is_none() {
            r.issues.push(issue(code::INVALID_GRADE, 3));
        }
    }
    if !bad[4] {
        r.class_raw = text(cells[4]);
        r.class_candidate = dream::class_candidate(&r.class_raw);
        if r.class_raw.chars().count() > 20 {
            r.issues.push(issue(code::INVALID_CLASS, 4));
        }
    }
    if !bad[5] {
        r.student_no_raw = text(cells[5]);
        r.student_no = dream::student_no_of(&r.student_no_raw);
        if r.student_no.is_none() {
            r.issues.push(issue(code::INVALID_STUDENT_NO, 5));
        }
    }
    if !bad[6] {
        r.student_name = text(cells[6]);
    }
    if !bad[7] {
        match dream::check_birth(&text(cells[7])) {
            Ok(v) => r.birth = Some(Secret::new(v)),
            Err(_) => r.issues.push(issue(code::INVALID_BIRTHDATE, 7)),
        }
    }
    if !bad[8] {
        // 숫자 셀 202609 — 정수 여섯 자리 YYYYMM
        let ok = match cells[8] {
            Cell::Number(f) if f.fract() == 0.0 && (100_000.0..1_000_000.0).contains(f) => {
                dream::check_billing_month(&format!("{}", *f as i64)).ok()
            }
            _ => None,
        };
        match ok {
            Some(v) => r.billing_month = Some(v),
            None => r.issues.push(issue(code::INVALID_BILLING_MONTH, 8).found(shown(cells[8]))),
        }
    }
    if !bad[9] {
        r.course_name = text(cells[9]);
    }
    if !bad[10] {
        match dream::check_course_code(&text(cells[10])) {
            Ok(v) => r.course_code = Some(v),
            Err(_) => r.issues.push(issue(code::INVALID_COURSE_CODE, 10)),
        }
    }
    if !bad[11] && !bad[12] {
        r.item_name = text(cells[11]);
        r.dream_item_code = text(cells[12]);
        r.item_code = dream::item_of(&r.item_name, &r.dream_item_code);
        if r.item_code.is_none() {
            r.issues.push(
                Issue::error(code::INVALID_ITEM_PAIR)
                    .at(source_row, 11)
                    .found(format!("{} / {}", r.item_name, r.dream_item_code)),
            );
        }
    }
    if !bad[13] {
        match money(cells[13]) {
            Ok(v) => r.amount_n = v,
            Err(_) => r.issues.push(issue(code::INVALID_AMOUNT, 13).found(shown(cells[13]))),
        }
    }
    let mut before = [None; 3];
    let mut before_ok = true;
    for (i, c) in [14usize, 15, 16].into_iter().enumerate() {
        if bad[c] {
            before_ok = false;
            continue;
        }
        match money(cells[c]) {
            Ok(v) => before[i] = v,
            Err(_) => {
                before_ok = false;
                r.issues.push(issue(code::INVALID_EXISTING_ALLOCATION, c).found(shown(cells[c])));
            }
        }
    }
    [r.personal_before, r.voucher_before, r.free_voucher_before] = before;
    if before_ok {
        if let Some(n) = r.amount_n {
            if n == 0 && before.iter().any(|v| v.is_some_and(|x| x != 0)) {
                // 0원 줄에 금액이 배분되어 있다 — 이상한 원본이다
                r.issues.push(Issue::error(code::INVALID_EXISTING_ALLOCATION).at(source_row, 13).found("0"));
            } else if let [Some(o), Some(p), Some(q)] = before {
                if o + p + q != n {
                    // 참고만 — 이 값들은 나중에 새로 계산해 덮어쓴다
                    r.issues.push(Issue::info(code::EXISTING_SUM_MISMATCH).at(source_row, 14));
                }
            }
        }
    }
    if !bad[17] && !cells[17].is_blank() {
        match dream::check_transfer_date(&text(cells[17])) {
            Ok(v) => r.transfer_date = Some(v),
            Err(_) => r.issues.push(issue(code::INVALID_TRANSFER_DATE, 17).found(text(cells[17]))),
        }
    }
    r
}

#[cfg(test)]
#[path = "dream_file_tests.rs"]
mod tests;
