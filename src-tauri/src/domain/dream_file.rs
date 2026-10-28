//! 방과후드림 결제파일 — 읽은 결과의 모양과 파일 수준 규칙 (DB 를 모르는 순수 코드).
//!
//! 파일을 읽는 일(`excel::dream_file`)과 여기의 규칙은 나뉜다. 여기서는
//!
//! * 한 줄이 무엇을 지니는가(`DreamRow`) — 원래 값과 비교용 후보를 **둘 다** 지닌다.
//! * 무엇이 잘못되었는가(`Issue`) — 안정된 코드 + 행 · 열. 사람이 읽는 문장은 화면이 만든다.
//! * 파일 전체의 공통값(회계년도 · 기관코드 · 수강월)과 작업공간 설정이 맞는가
//!
//! 를 정한다. 값을 고쳐 맞추지 않는다. 안전 규칙을 어기면 '확인 필요'로 낮추지 않고 오류다.

use std::collections::BTreeSet;

use serde::Serialize;
use zeroize::Zeroize;

/// 열 문자(A~R).
pub const LETTERS: [&str; 18] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// 참고 — 상태를 바꾸지 않는다
    Info,
    /// 사람이 골라야 한다(P3: 학생 후보가 여럿 등)
    Review,
    /// 막는다
    Error,
}

/// 문제 하나. `found` 에는 개인정보가 아닌 값만 넣는다(코드 · 월 · 머리글 등).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: &'static str,
    pub severity: Severity,
    /// Excel 행 번호(1부터). 파일 전체 문제면 None
    pub row: Option<u32>,
    /// 열 문자
    pub column: Option<&'static str>,
    pub expected: Option<String>,
    pub found: Option<String>,
}

impl Issue {
    pub fn error(code: &'static str) -> Self {
        Issue {
            code,
            severity: Severity::Error,
            row: None,
            column: None,
            expected: None,
            found: None,
        }
    }
    pub fn info(code: &'static str) -> Self {
        Issue {
            severity: Severity::Info,
            ..Issue::error(code)
        }
    }
    pub fn at(mut self, row: u32, col: usize) -> Self {
        self.row = Some(row);
        self.column = LETTERS.get(col).copied();
        self
    }
    pub fn row(mut self, row: u32) -> Self {
        self.row = Some(row);
        self
    }
    pub fn expected(mut self, v: impl Into<String>) -> Self {
        self.expected = Some(v.into());
        self
    }
    pub fn found(mut self, v: impl Into<String>) -> Self {
        self.found = Some(v.into());
        self
    }
}

/// 오류 코드. 화면은 이 코드를 사람이 읽는 문장으로 바꾼다.
pub mod code {
    // 파일 수준
    pub const FILE_NOT_XLSX: &str = "FILE_NOT_XLSX";
    pub const FILE_TOO_LARGE: &str = "FILE_TOO_LARGE";
    pub const FILE_READ_FAILED: &str = "FILE_READ_FAILED";
    pub const WORKBOOK_CORRUPT: &str = "WORKBOOK_CORRUPT";
    pub const SHEET_MISSING: &str = "SHEET_MISSING";
    pub const EXTRA_SHEET: &str = "EXTRA_SHEET";
    pub const HEADER_MISMATCH: &str = "HEADER_MISMATCH";
    pub const EXTRA_DATA_COLUMN: &str = "EXTRA_DATA_COLUMN";
    pub const EMPTY_DATA: &str = "EMPTY_DATA";
    pub const INTERNAL_EMPTY_ROW: &str = "INTERNAL_EMPTY_ROW";
    pub const MIXED_ACCOUNTING_YEAR: &str = "MIXED_ACCOUNTING_YEAR";
    pub const MIXED_ORG_CODE: &str = "MIXED_ORG_CODE";
    pub const MIXED_BILLING_MONTH: &str = "MIXED_BILLING_MONTH";
    pub const ACCOUNTING_YEAR_MISMATCH: &str = "ACCOUNTING_YEAR_MISMATCH";
    pub const ORG_CODE_NOT_SET: &str = "ORG_CODE_NOT_SET";
    pub const ORG_CODE_MISMATCH: &str = "ORG_CODE_MISMATCH";
    pub const BILLING_MONTH_NOT_SET: &str = "BILLING_MONTH_NOT_SET";
    pub const BILLING_MONTH_MISMATCH: &str = "BILLING_MONTH_MISMATCH";
    // 행 수준
    pub const REQUIRED_VALUE_MISSING: &str = "REQUIRED_VALUE_MISSING";
    pub const INVALID_CELL_TYPE: &str = "INVALID_CELL_TYPE";
    pub const INVALID_ACCOUNTING_YEAR: &str = "INVALID_ACCOUNTING_YEAR";
    pub const INVALID_ORG_CODE: &str = "INVALID_ORG_CODE";
    pub const INVALID_PERSONAL_REF: &str = "INVALID_PERSONAL_REF";
    pub const INVALID_GRADE: &str = "INVALID_GRADE";
    pub const INVALID_CLASS: &str = "INVALID_CLASS";
    pub const INVALID_STUDENT_NO: &str = "INVALID_STUDENT_NO";
    pub const INVALID_BIRTHDATE: &str = "INVALID_BIRTHDATE";
    pub const INVALID_BILLING_MONTH: &str = "INVALID_BILLING_MONTH";
    pub const INVALID_COURSE_CODE: &str = "INVALID_COURSE_CODE";
    pub const INVALID_ITEM_PAIR: &str = "INVALID_ITEM_PAIR";
    pub const INVALID_AMOUNT: &str = "INVALID_AMOUNT";
    pub const INVALID_EXISTING_ALLOCATION: &str = "INVALID_EXISTING_ALLOCATION";
    pub const INVALID_TRANSFER_DATE: &str = "INVALID_TRANSFER_DATE";
    // 참고
    pub const EXISTING_SUM_MISMATCH: &str = "EXISTING_SUM_MISMATCH";
}

/// 개인정보 값 — `Debug` 로 찍히지 않고, 사라질 때 지워진다.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(v: String) -> Self {
        Secret(v)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret(******)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// 결제파일 한 줄. 값이 형식에 맞지 않으면 그 칸은 None 이고 `issues` 에 까닭이 있다.
///
/// `display` 는 화면에 보일 18칸이다 — 개인참조번호(C) · 생년월일(H)은 이미 가려져 있다.
/// 원래 값은 `personal_ref` · `birth` 에만 있고, 이 둘은 로그에 찍히지 않는다.
#[derive(Debug, Clone, Default)]
pub struct DreamRow {
    pub source_row: u32,
    pub display: Vec<String>,
    pub accounting_year: Option<String>,
    pub org_code: Option<String>,
    pub personal_ref: Option<Secret>,
    pub grade_raw: String,
    pub grade_candidate: Option<i64>,
    pub class_raw: String,
    /// 끝의 `반` 을 한 번 뗀 값(있을 때만). 원래 값은 `class_raw`
    pub class_candidate: Option<String>,
    pub student_no_raw: String,
    pub student_no: Option<i64>,
    pub student_name: String,
    pub birth: Option<Secret>,
    pub billing_month: Option<String>,
    pub course_name: String,
    pub course_code: Option<String>,
    pub item_name: String,
    /// 방과후드림 항목구분코드(`01` 등)
    pub dream_item_code: String,
    /// 이름 · 코드가 둘 다 맞을 때의 비용항목 코드
    pub item_code: Option<&'static str>,
    pub amount_n: Option<i64>,
    /// 원본의 O(개인부담금) · P(이용권) · Q(자유수강권). 비어 있으면 None. **정답으로 믿지 않는다.**
    pub personal_before: Option<i64>,
    pub voucher_before: Option<i64>,
    pub free_voucher_before: Option<i64>,
    pub transfer_date: Option<String>,
    pub issues: Vec<Issue>,
}

impl DreamRow {
    pub fn has_error(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

/// 읽은 파일 전체.
#[derive(Debug, Clone, Default)]
pub struct DreamFile {
    pub sheet_name: Option<String>,
    /// 실제 결제행만(빈 행 제외)
    pub rows: Vec<DreamRow>,
    pub file_issues: Vec<Issue>,
    /// 모든 줄이 같은 값일 때만 Some
    pub accounting_year: Option<String>,
    pub org_code: Option<String>,
    pub billing_month: Option<String>,
}

/// 줄마다 하나여야 하는 값들을 모은다. 섞여 있으면 파일 오류다.
pub fn check_common(file: &mut DreamFile) {
    fn one(values: BTreeSet<String>, mixed: &'static str, issues: &mut Vec<Issue>) -> Option<String> {
        match values.len() {
            0 => None,
            1 => values.into_iter().next(),
            _ => {
                // 회계년도 · 기관코드 · 수강월은 개인정보가 아니므로 보여 준다
                issues.push(Issue::error(mixed).found(values.into_iter().collect::<Vec<_>>().join(", ")));
                None
            }
        }
    }
    let years: BTreeSet<String> = file.rows.iter().filter_map(|r| r.accounting_year.clone()).collect();
    let orgs: BTreeSet<String> = file.rows.iter().filter_map(|r| r.org_code.clone()).collect();
    let months: BTreeSet<String> = file.rows.iter().filter_map(|r| r.billing_month.clone()).collect();
    file.accounting_year = one(years, code::MIXED_ACCOUNTING_YEAR, &mut file.file_issues);
    file.org_code = one(orgs, code::MIXED_ORG_CODE, &mut file.file_issues);
    file.billing_month = one(months, code::MIXED_BILLING_MONTH, &mut file.file_issues);
}

/// 선택한 작업공간 쪽의 기대값.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    /// 작업공간이 속한 학년도(예: 2026). 달력 연도로 짐작하지 않는다.
    pub accounting_year: i64,
    pub org_code: Option<String>,
    pub billing_month: Option<String>,
}

/// 파일 공통값을 작업공간 설정과 맞대어 본다. 고쳐 맞추지 않는다.
pub fn check_workspace(file: &DreamFile, expect: &Expect) -> Vec<Issue> {
    let mut out = Vec::new();
    if let Some(y) = &file.accounting_year {
        let want = expect.accounting_year.to_string();
        if *y != want {
            out.push(Issue::error(code::ACCOUNTING_YEAR_MISMATCH).expected(want).found(y.clone()));
        }
    }
    match (&expect.org_code, &file.org_code) {
        (None, _) => out.push(Issue::error(code::ORG_CODE_NOT_SET)),
        (Some(want), Some(got)) if want != got => {
            out.push(Issue::error(code::ORG_CODE_MISMATCH).expected(want.clone()).found(got.clone()))
        }
        _ => {}
    }
    match (&expect.billing_month, &file.billing_month) {
        (None, _) => out.push(Issue::error(code::BILLING_MONTH_NOT_SET)),
        (Some(want), Some(got)) if want != got => {
            out.push(Issue::error(code::BILLING_MONTH_MISMATCH).expected(want.clone()).found(got.clone()))
        }
        _ => {}
    }
    out
}

/// 전체 상태 — 오류가 하나라도 있으면 ERROR, 사람이 고를 것이 있으면 REVIEW.
pub fn overall<'a>(issues: impl Iterator<Item = &'a Issue>) -> &'static str {
    match issues.map(|i| i.severity).max() {
        Some(Severity::Error) => "ERROR",
        Some(Severity::Review) => "REVIEW",
        _ => "OK",
    }
}
