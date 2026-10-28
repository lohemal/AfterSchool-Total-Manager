//! 결제파일 검사 — 파일을 읽고, 선택한 작업공간의 설정과 맞대어, 화면용 결과를 만든다.
//!
//! **읽기만 한다.** DB 에 아무것도 쓰지 않고(처리 이력도 만들지 않는다), 원본 파일을
//! 다른 곳에 복사하지 않는다. 화면으로 가는 값에서 개인참조번호 · 생년월일은 이미 가려져
//! 있다 — 원래 값은 이 모듈 밖으로 나가지 않는다.

use rusqlite::Connection;
use serde::Serialize;

use crate::crypto;
use crate::domain::dream_file::{check_workspace, overall, Expect, Issue, Severity};
use crate::error::AppResult;
use crate::excel::dream_file;
use crate::repo;
use crate::repo::dream::{setting, workspace_setting};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub source_row: u32,
    /// A~R 18칸(C · H 는 가림)
    pub cells: Vec<String>,
    /// `OK` | `REVIEW` | `ERROR`
    pub status: &'static str,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCheck {
    pub file_name: String,
    pub file_size: u64,
    pub sha256: String,
    pub sheet_name: Option<String>,
    /// 실제 결제행 수(0원 행 포함)
    pub data_rows: usize,
    /// 전체 상태 — 파일 · 작업공간 · 줄 문제를 모두 본다
    pub status: &'static str,
    pub file_issues: Vec<Issue>,
    pub row_error_count: usize,
    pub row_info_count: usize,
    pub accounting_year: Option<String>,
    pub org_code: Option<String>,
    pub billing_month: Option<String>,
    /// 작업공간 쪽 기대값
    pub expected_year: i64,
    pub expected_org_code: Option<String>,
    pub expected_billing_month: Option<String>,
    pub rows: Vec<PreviewRow>,
}

fn expect_of(conn: &Connection, workspace_id: i64) -> AppResult<Expect> {
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;
    Ok(Expect {
        accounting_year: year.year,
        org_code: setting::org_code(conn)?,
        billing_month: workspace_setting::billing_month(conn, workspace_id)?,
    })
}

/// 너무 커서 읽지 않은 파일 — 오류 하나만 담아 돌려준다.
pub fn too_large(conn: &Connection, workspace_id: i64, file_name: &str, size: u64) -> AppResult<FileCheck> {
    let expect = expect_of(conn, workspace_id)?;
    log::info!("방과후드림 결제파일 검사: {file_name} · 너무 큼({size} bytes)");
    Ok(FileCheck {
        file_name: file_name.to_string(),
        file_size: size,
        sha256: String::new(),
        sheet_name: None,
        data_rows: 0,
        status: "ERROR",
        file_issues: vec![Issue::error(crate::domain::dream_file::code::FILE_TOO_LARGE)],
        row_error_count: 0,
        row_info_count: 0,
        accounting_year: None,
        org_code: None,
        billing_month: None,
        expected_year: expect.accounting_year,
        expected_org_code: expect.org_code,
        expected_billing_month: expect.billing_month,
        rows: Vec::new(),
    })
}

/// 파일 바이트를 받아 검사한다. `file_name` 은 경로 없이 이름만.
pub fn check(conn: &Connection, workspace_id: i64, file_name: &str, bytes: &[u8]) -> AppResult<FileCheck> {
    let expect = expect_of(conn, workspace_id)?;
    let parsed = dream_file::parse(file_name, bytes);
    let mut file_issues = parsed.file_issues.clone();
    // 파일을 열지 못했거나 머리글이 틀렸으면 공통값이 없다 — 그때는 작업공간과 견주지 않는다
    let readable = parsed.sheet_name.is_some() && !parsed.rows.is_empty();
    if readable {
        file_issues.extend(check_workspace(&parsed, &expect));
    }

    let rows: Vec<PreviewRow> = parsed
        .rows
        .iter()
        .map(|r| PreviewRow {
            source_row: r.source_row,
            cells: r.display.clone(),
            status: overall(r.issues.iter()),
            issues: r.issues.clone(),
        })
        .collect();
    let row_error_count = rows.iter().filter(|r| r.status == "ERROR").count();
    let row_info_count = rows
        .iter()
        .filter(|r| r.issues.iter().any(|i| i.severity == Severity::Info))
        .count();
    let status = overall(file_issues.iter().chain(rows.iter().flat_map(|r| r.issues.iter())));

    // 진단 로그 — 파일 이름 · 건수 · 코드만. 이름 · 번호 · 생년월일은 적지 않는다.
    let mut codes: Vec<&str> = file_issues.iter().map(|i| i.code).collect();
    codes.sort_unstable();
    codes.dedup();
    log::info!(
        "방과후드림 결제파일 검사: {file_name} · 결제행 {} · 오류 줄 {row_error_count} · 파일 문제 {:?} · {status}",
        rows.len(),
        codes
    );

    Ok(FileCheck {
        file_name: file_name.to_string(),
        file_size: bytes.len() as u64,
        sha256: crypto::sha256_hex(bytes),
        sheet_name: parsed.sheet_name.clone(),
        data_rows: rows.len(),
        status,
        file_issues,
        row_error_count,
        row_info_count,
        accounting_year: parsed.accounting_year.clone(),
        org_code: parsed.org_code.clone(),
        billing_month: parsed.billing_month.clone(),
        expected_year: expect.accounting_year,
        expected_org_code: expect.org_code,
        expected_billing_month: expect.billing_month,
        rows,
    })
}
