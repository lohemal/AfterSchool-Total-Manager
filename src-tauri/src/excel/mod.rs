//! Excel 업로드 / 내려받기.
//!
//! 업로드는 두 단계다 (설계안 10-1).
//!   1. `preview` — 파일을 읽고 줄마다 검사해 **정상 / 오류 / 경고**로 나눈다.
//!      정상 줄은 메모리에 보관하고 토큰만 화면에 준다.
//!   2. `commit`  — 사용자가 확인하면 그 토큰의 정상 줄만 한 트랜잭션으로 저장한다.
//!
//! 오류가 있는 줄 때문에 정상 줄까지 버리지 않는다 (§28).

pub mod admin;
pub mod read;
pub mod write;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::domain::{class_no, grade_matches, parse_grades};
use crate::error::{AppError, AppResult};
use crate::model::{CostItem, Fee};
use crate::repo;
use crate::repo::department::DepartmentRow;
use crate::repo::eligibility::EligibilityRow;
use crate::repo::enrollment::EnrollmentRow;
use crate::repo::student::StudentRow;

// ─────────────────────────────────────────────── 화면과 주고받는 자료

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowIssue {
    /// Excel 화면에 보이는 행 번호
    pub row: usize,
    pub cells: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub token: String,
    pub kind: String,
    pub file_name: String,
    pub headers: Vec<String>,
    pub total: usize,
    pub ok_count: usize,
    /// 저장되지 않는 줄
    pub errors: Vec<RowIssue>,
    /// 저장은 되지만 확인이 필요한 줄 (이름 불일치, 대상학년 불일치 등)
    pub warnings: Vec<RowIssue>,
    /// 정상 줄 미리보기 (앞 20줄)
    pub preview: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub name: String,
    pub rows: usize,
}

/// 확인 대기 중인 업로드 자료. 토큰으로 꺼내 쓰고 지운다.
#[derive(Debug)]
pub enum Staged {
    Students(Vec<StudentRow>),
    Eligibility {
        program: String,
        rows: Vec<EligibilityRow>,
    },
    Departments(Vec<DepartmentRow>),
    Enrollments(Vec<EnrollmentRow>),
}

#[derive(Default)]
pub struct Stage(pub Mutex<HashMap<String, Staged>>);

impl Stage {
    pub fn put(&self, staged: Staged) -> AppResult<String> {
        let token = uuid::Uuid::new_v4().to_string();
        let mut map = self
            .0
            .lock()
            .map_err(|_| AppError::new("LOCK", "업로드 자료에 접근하지 못했습니다."))?;
        // 확인하지 않고 떠난 자료가 쌓이지 않도록 오래된 것은 버린다.
        if map.len() > 8 {
            map.clear();
        }
        map.insert(token.clone(), staged);
        Ok(token)
    }

    pub fn take(&self, token: &str) -> AppResult<Staged> {
        let mut map = self
            .0
            .lock()
            .map_err(|_| AppError::new("LOCK", "업로드 자료에 접근하지 못했습니다."))?;
        map.remove(token).ok_or_else(|| {
            AppError::invalid("업로드 자료가 만료되었습니다. 파일을 다시 선택해 주세요.")
        })
    }
}

// ─────────────────────────────────────────────── 열 정의

const STUDENT_COLS: &[&str] = &["학년", "반", "번호", "이름", "비고"];
const STUDENT_SAMPLE: &[&str] = &["3", "1", "5", "홍길동", ""];

const ELIG_COLS: &[&str] = &["학년", "반", "번호", "이름", "적용 시작일", "적용 종료일", "비고"];
const ELIG_SAMPLE: &[&str] = &["3", "1", "5", "홍길동", "", "", "비우면 학년도 내내 적용"];

const DEPT_FIXED_COLS: &[&str] = &["부서명", "반명", "강사명", "요일"];
const DEPT_SAMPLE_FIXED: &[&str] = &["로봇과학", "A반", "김강사", "월,수"];

const ENROLL_COLS: &[&str] = &["부서명", "반명", "학년", "반", "번호", "이름"];
const ENROLL_SAMPLE: &[&str] = &["로봇과학", "A반", "3", "1", "5", "홍길동"];

fn dept_headers(items: &[CostItem]) -> Vec<String> {
    let mut v: Vec<String> = DEPT_FIXED_COLS.iter().map(|s| s.to_string()).collect();
    v.extend(items.iter().map(|i| i.name.clone()));
    v
}

// ─────────────────────────────────────────────── 업로드 양식

pub fn template(kind: &str, items: &[CostItem], dir: &Path) -> AppResult<ExportResult> {
    let (name, headers, sample): (&str, Vec<String>, Vec<String>) = match kind {
        "students" => (
            "학생정보",
            STUDENT_COLS.iter().map(|s| s.to_string()).collect(),
            STUDENT_SAMPLE.iter().map(|s| s.to_string()).collect(),
        ),
        "eligibility" => (
            "지원대상자",
            ELIG_COLS.iter().map(|s| s.to_string()).collect(),
            ELIG_SAMPLE.iter().map(|s| s.to_string()).collect(),
        ),
        "departments" => {
            let mut sample: Vec<String> = DEPT_SAMPLE_FIXED.iter().map(|s| s.to_string()).collect();
            sample.extend(items.iter().map(|_| "0".to_string()));
            ("부서정보", dept_headers(items), sample)
        }
        "enrollments" => (
            "수강정보",
            ENROLL_COLS.iter().map(|s| s.to_string()).collect(),
            ENROLL_SAMPLE.iter().map(|s| s.to_string()).collect(),
        ),
        _ => return Err(AppError::invalid("알 수 없는 업로드 양식입니다.")),
    };

    let path = write::export_path(dir, &format!("{name}_업로드양식"), &[])?;
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();
    let samp: Vec<&str> = sample.iter().map(|s| s.as_str()).collect();
    write::write_sheet(&path, name, &head, &[], &[], Some(&samp))?;
    Ok(done(path, 0))
}

pub(crate) fn done(path: PathBuf, rows: usize) -> ExportResult {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    ExportResult {
        path: path.to_string_lossy().to_string(),
        name,
        rows,
    }
}

// ─────────────────────────────────────────────── 업로드 검사

struct Collector {
    errors: Vec<RowIssue>,
    warnings: Vec<RowIssue>,
    preview: Vec<Vec<String>>,
    total: usize,
}

impl Collector {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            preview: Vec::new(),
            total: 0,
        }
    }
    fn error(&mut self, row: usize, cells: &[String], reason: impl Into<String>) {
        self.errors.push(RowIssue {
            row,
            cells: cells.to_vec(),
            reason: reason.into(),
        });
    }
    fn warn(&mut self, row: usize, cells: &[String], reason: impl Into<String>) {
        self.warnings.push(RowIssue {
            row,
            cells: cells.to_vec(),
            reason: reason.into(),
        });
    }
    fn keep(&mut self, cells: Vec<String>) {
        if self.preview.len() < 20 {
            self.preview.push(cells);
        }
    }
}

pub fn preview_students(path: &Path) -> AppResult<(ImportPreview, Staged)> {
    let sheet = read::read_first_sheet(path)?;
    let c_grade = sheet.require("학년")?;
    let c_class = sheet.require("반")?;
    let c_no = sheet.require("번호")?;
    let c_name = sheet.require("이름")?;
    let c_note = sheet.col("비고");

    let mut col = Collector::new();
    // 반은 문자다. '1'과 '가'가 모두 같은 자리에 들어온다.
    let mut seen: HashSet<(i64, String, i64)> = HashSet::new();
    let mut rows = Vec::new();

    for (line, cells) in &sheet.rows {
        col.total += 1;
        let grade = read::parse_int(sheet.cell(cells, Some(c_grade)));
        // read::text 를 거친 값이라 엑셀이 숫자로 넣은 1도 "1"로 들어온다.
        let class_no = class_no::normalize(sheet.cell(cells, Some(c_class)));
        let student_no = read::parse_int(sheet.cell(cells, Some(c_no)));
        let name = sheet.cell(cells, Some(c_name)).trim().to_string();

        let (Some(grade), Some(student_no)) = (grade, student_no) else {
            col.error(*line, cells, "학년과 번호는 숫자여야 합니다.");
            continue;
        };
        let class_no = match class_no::check(&class_no) {
            Ok(v) => v,
            Err(msg) => {
                col.error(*line, cells, &msg);
                continue;
            }
        };
        if !(1..=9).contains(&grade) || !(1..=99).contains(&student_no) {
            col.error(*line, cells, "학년(1~9)·번호(1~99) 범위를 벗어났습니다.");
            continue;
        }
        if name.is_empty() {
            col.error(*line, cells, "이름이 비어 있습니다.");
            continue;
        }
        if !seen.insert((grade, class_no.clone(), student_no)) {
            col.error(*line, cells, "같은 학년·반·번호가 파일 안에 두 번 있습니다.");
            continue;
        }

        col.keep(cells.clone());
        rows.push(StudentRow {
            grade,
            class_no,
            student_no,
            name,
            note: sheet.cell(cells, c_note).to_string(),
        });
    }

    Ok(finish(path, "students", &sheet.headers, col, rows.len(), Staged::Students(rows)))
}

pub fn preview_eligibility(
    conn: &Connection,
    year_id: i64,
    program: &str,
    path: &Path,
) -> AppResult<(ImportPreview, Staged)> {
    let sheet = read::read_first_sheet(path)?;
    let c_grade = sheet.require("학년")?;
    let c_class = sheet.require("반")?;
    let c_no = sheet.require("번호")?;
    let c_name = sheet.col("이름");
    let c_from = sheet.col("적용 시작일");
    let c_to = sheet.col("적용 종료일");
    let c_note = sheet.col("비고");

    let targets = repo::eligibility::target_grades(conn, year_id, program)?;
    let mut col = Collector::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut rows = Vec::new();

    for (line, cells) in &sheet.rows {
        col.total += 1;
        let grade = read::parse_int(sheet.cell(cells, Some(c_grade)));
        let class_no = class_no::normalize(sheet.cell(cells, Some(c_class)));
        let student_no = read::parse_int(sheet.cell(cells, Some(c_no)));
        let (Some(grade), Some(student_no)) = (grade, student_no) else {
            col.error(*line, cells, "학년과 번호는 숫자여야 합니다.");
            continue;
        };
        if class_no.is_empty() {
            col.error(*line, cells, "반이 비어 있습니다.");
            continue;
        }

        let found = repo::student::find_by_key(conn, year_id, grade, &class_no, student_no)?;
        let Some((student_id, real_name)) = found else {
            col.error(
                *line,
                cells,
                format!("{grade}학년 {class_no}반 {student_no}번 학생이 학생정보에 없습니다."),
            );
            continue;
        };
        if !seen.insert(student_id) {
            col.error(*line, cells, "같은 학생이 파일 안에 두 번 있습니다.");
            continue;
        }

        // 날짜는 비워도 된다 — 비우면 학년도 내내 유효하다.
        let raw_from = sheet.cell(cells, c_from);
        let raw_to = sheet.cell(cells, c_to);
        let valid_from = if raw_from.trim().is_empty() {
            None
        } else {
            match read::parse_date(raw_from) {
                Some(d) => Some(d),
                None => {
                    col.error(*line, cells, "적용 시작일 형식이 올바르지 않습니다. (예: 2026-03-01)");
                    continue;
                }
            }
        };
        let valid_to = if raw_to.trim().is_empty() {
            None
        } else {
            match read::parse_date(raw_to) {
                Some(d) => Some(d),
                None => {
                    col.error(*line, cells, "적용 종료일 형식이 올바르지 않습니다. (예: 2026-08-31)");
                    continue;
                }
            }
        };
        if let (Some(f), Some(t)) = (&valid_from, &valid_to) {
            if f > t {
                col.error(*line, cells, "적용 종료일이 시작일보다 빠릅니다.");
                continue;
            }
        }

        // 아래 둘은 경고다 — 저장은 하되 사람이 확인하게 한다 (설계안 0-3).
        let typed = sheet.cell(cells, c_name).trim().to_string();
        if !typed.is_empty() && typed != real_name {
            col.warn(
                *line,
                cells,
                format!("이름이 학생정보와 다릅니다. (학생정보: {real_name})"),
            );
        }
        if !grade_matches(&targets, grade) {
            col.warn(
                *line,
                cells,
                format!("대상학년({})이 아닙니다.", grade_text(&targets)),
            );
        }

        col.keep(cells.clone());
        rows.push(EligibilityRow {
            student_id,
            valid_from,
            valid_to,
            note: sheet.cell(cells, c_note).to_string(),
        });
    }

    let n = rows.len();
    Ok(finish(
        path,
        "eligibility",
        &sheet.headers,
        col,
        n,
        Staged::Eligibility {
            program: program.to_string(),
            rows,
        },
    ))
}

fn grade_text(targets: &[i64]) -> String {
    if targets.is_empty() {
        "전 학년".to_string()
    } else {
        targets
            .iter()
            .map(|g| format!("{g}학년"))
            .collect::<Vec<_>>()
            .join("·")
    }
}

pub fn preview_departments(
    items: &[CostItem],
    path: &Path,
) -> AppResult<(ImportPreview, Staged)> {
    let sheet = read::read_first_sheet(path)?;
    let c_name = sheet.require("부서명")?;
    let c_class = sheet.col("반명");
    let c_teacher = sheet.col("강사명");
    let c_days = sheet.col("요일");
    let fee_cols: Vec<(String, Option<usize>)> = items
        .iter()
        .map(|i| (i.code.clone(), sheet.col(&i.name)))
        .collect();

    let mut col = Collector::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut rows = Vec::new();

    for (line, cells) in &sheet.rows {
        col.total += 1;
        let name = sheet.cell(cells, Some(c_name)).trim().to_string();
        if name.is_empty() {
            col.error(*line, cells, "부서명이 비어 있습니다.");
            continue;
        }
        let class_name = sheet.cell(cells, c_class).trim().to_string();
        if !seen.insert((name.clone(), class_name.clone())) {
            col.error(*line, cells, "같은 부서명·반명이 파일 안에 두 번 있습니다.");
            continue;
        }

        let mut fees = Vec::new();
        let mut bad: Option<String> = None;
        for (code, c) in &fee_cols {
            let raw = sheet.cell(cells, *c);
            match read::parse_amount(raw) {
                Some(v) if v >= 0 => fees.push(Fee {
                    item_code: code.clone(),
                    amount: v,
                }),
                Some(_) => bad = Some(format!("금액이 음수입니다: {raw}")),
                None => bad = Some(format!("금액을 숫자로 읽지 못했습니다: {raw}")),
            }
        }
        if let Some(reason) = bad {
            col.error(*line, cells, reason);
            continue;
        }

        col.keep(cells.clone());
        rows.push(DepartmentRow {
            name,
            class_name,
            teacher: sheet.cell(cells, c_teacher).trim().to_string(),
            days: sheet.cell(cells, c_days).trim().to_string(),
            fees,
        });
    }

    let n = rows.len();
    Ok(finish(path, "departments", &sheet.headers, col, n, Staged::Departments(rows)))
}

/// 수강 데이터 업로드 검사.
///
/// 걸러 내는 것 (요구사항 §28)
///   · 학생정보에 없는 학생 / 부서정보에 없는 부서  → 오류
///   · 같은 학생·부서가 파일 안에 두 번                → 오류
///   · 이미 수강 중인 학생·부서                        → 오류
///   · 필수값 누락                                     → 오류
///   · 이름이 학생정보와 다름                          → 경고 (저장은 된다)
pub fn preview_enrollments(
    conn: &Connection,
    year_id: i64,
    workspace_id: i64,
    path: &Path,
) -> AppResult<(ImportPreview, Staged)> {
    let sheet = read::read_first_sheet(path)?;
    let c_dept = sheet.require("부서명")?;
    let c_class_name = sheet.col("반명");
    let c_grade = sheet.require("학년")?;
    let c_class = sheet.require("반")?;
    let c_no = sheet.require("번호")?;
    let c_name = sheet.col("이름");

    let mut col = Collector::new();
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    let mut rows = Vec::new();

    for (line, cells) in &sheet.rows {
        col.total += 1;

        let dept_name = sheet.cell(cells, Some(c_dept)).trim().to_string();
        if dept_name.is_empty() {
            col.error(*line, cells, "부서명이 비어 있습니다.");
            continue;
        }
        let class_name = sheet.cell(cells, c_class_name).trim().to_string();
        let Some(department_id) =
            repo::department::find_by_name(conn, workspace_id, &dept_name, &class_name)?
        else {
            col.error(
                *line,
                cells,
                format!(
                    "'{}' 부서가 이 작업공간의 부서정보에 없습니다.",
                    dept_label_of(&dept_name, &class_name)
                ),
            );
            continue;
        };

        let grade = read::parse_int(sheet.cell(cells, Some(c_grade)));
        let class_no = class_no::normalize(sheet.cell(cells, Some(c_class)));
        let student_no = read::parse_int(sheet.cell(cells, Some(c_no)));
        let (Some(grade), Some(student_no)) = (grade, student_no) else {
            col.error(*line, cells, "학년과 번호는 숫자여야 합니다.");
            continue;
        };
        if class_no.is_empty() {
            col.error(*line, cells, "반이 비어 있습니다.");
            continue;
        }

        let Some((student_id, real_name)) =
            repo::student::find_by_key(conn, year_id, grade, &class_no, student_no)?
        else {
            col.error(
                *line,
                cells,
                format!("{grade}학년 {class_no}반 {student_no}번 학생이 학생정보에 없습니다."),
            );
            continue;
        };

        if !seen.insert((student_id, department_id)) {
            col.error(*line, cells, "같은 학생·부서가 파일 안에 두 번 있습니다.");
            continue;
        }
        if repo::enrollment::active_exists(conn, workspace_id, student_id, department_id)? {
            col.error(
                *line,
                cells,
                format!("{real_name} 학생은 이 부서를 이미 수강 중입니다."),
            );
            continue;
        }

        let typed = sheet.cell(cells, c_name).trim().to_string();
        if !typed.is_empty() && typed != real_name {
            col.warn(
                *line,
                cells,
                format!("이름이 학생정보와 다릅니다. (학생정보: {real_name})"),
            );
        }

        col.keep(cells.clone());
        rows.push(EnrollmentRow {
            student_id,
            department_id,
        });
    }

    let n = rows.len();
    Ok(finish(
        path,
        "enrollments",
        &sheet.headers,
        col,
        n,
        Staged::Enrollments(rows),
    ))
}

fn dept_label_of(name: &str, class_name: &str) -> String {
    if class_name.trim().is_empty() {
        name.to_string()
    } else {
        format!("{name}{class_name}")
    }
}

fn finish(
    path: &Path,
    kind: &str,
    headers: &[String],
    col: Collector,
    ok_count: usize,
    staged: Staged,
) -> (ImportPreview, Staged) {
    (
        ImportPreview {
            token: String::new(), // 호출한 쪽에서 Stage에 넣고 채운다
            kind: kind.to_string(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            headers: headers.to_vec(),
            total: col.total,
            ok_count,
            errors: col.errors,
            warnings: col.warnings,
            preview: col.preview,
        },
        staged,
    )
}

// ─────────────────────────────────────────────── 저장

pub fn commit(
    conn: &Connection,
    staged: Staged,
    year_id: i64,
    workspace_id: Option<i64>,
) -> AppResult<ImportResult> {
    let (added, updated) = match staged {
        Staged::Students(rows) => repo::student::upsert_bulk(conn, year_id, &rows)?,
        Staged::Eligibility { program, rows } => {
            repo::eligibility::upsert_bulk(conn, year_id, &program, &rows)?
        }
        Staged::Departments(rows) => {
            let ws = workspace_id
                .ok_or_else(|| AppError::invalid("먼저 작업공간을 선택해 주세요."))?;
            repo::department::upsert_bulk(conn, ws, &rows)?
        }
        Staged::Enrollments(rows) => {
            let ws = workspace_id
                .ok_or_else(|| AppError::invalid("먼저 작업공간을 선택해 주세요."))?;
            let items = repo::cost_items(conn)?;
            // 수강을 만들면서 그 부서의 기준 수강료로 charge를 함께 만든다.
            let (added, skipped) = repo::enrollment::create_bulk(conn, ws, &rows, &items)?;
            (added, skipped)
        }
    };
    Ok(ImportResult { added, updated })
}

// ─────────────────────────────────────────────── 내려받기

pub fn export_students(
    conn: &Connection,
    year_id: i64,
    filter: &crate::model::StudentFilter,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let list = repo::student::list(conn, year_id, filter)?;
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|s| {
            vec![
                s.grade.to_string(),
                s.class_no.to_string(),
                s.student_no.to_string(),
                s.name.clone(),
                program_label(&s.programs),
                s.note.clone(),
            ]
        })
        .collect();
    let headers = ["학년", "반", "번호", "이름", "지원유형", "비고"];
    let path = write::export_path(dir, "학생정보", scope)?;
    let n = rows.len();
    write::write_sheet(&path, "학생정보", &headers, &rows, &[], None)?;
    Ok(done(path, n))
}

pub fn export_eligibility(
    conn: &Connection,
    year_id: i64,
    program: &str,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let list = repo::eligibility::list(conn, year_id, program, None)?;
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|e| {
            vec![
                e.grade.to_string(),
                e.class_no.to_string(),
                e.student_no.to_string(),
                e.name.clone(),
                e.valid_from.clone().unwrap_or_default(),
                e.valid_to.clone().unwrap_or_default(),
                e.note.clone(),
            ]
        })
        .collect();
    let title = if program == "VOUCHER" {
        "방과후이용권대상자"
    } else {
        "자유수강권대상자"
    };
    let path = write::export_path(dir, title, scope)?;
    let n = rows.len();
    write::write_sheet(&path, title, ELIG_COLS, &rows, &[], None)?;
    Ok(done(path, n))
}

pub fn export_departments(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let list = repo::department::list(conn, workspace_id, None)?;
    let mut headers = dept_headers(items);
    headers.push("합계".to_string());
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();
    let money_cols: Vec<usize> =
        (DEPT_FIXED_COLS.len()..DEPT_FIXED_COLS.len() + items.len() + 1).collect();

    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|d| {
            let map: HashMap<&str, i64> = d
                .fees
                .iter()
                .map(|f| (f.item_code.as_str(), f.amount))
                .collect();
            let mut row = vec![
                d.name.clone(),
                d.class_name.clone(),
                d.teacher.clone(),
                d.days.clone(),
            ];
            for i in items {
                row.push(map.get(i.code.as_str()).copied().unwrap_or(0).to_string());
            }
            row.push(d.total.to_string());
            row
        })
        .collect();

    let path = write::export_path(dir, "부서정보", scope)?;
    let n = rows.len();
    write::write_sheet(&path, "부서정보", &head, &rows, &money_cols, None)?;
    Ok(done(path, n))
}

/// 수강생 명단 내려받기. 화면에 걸린 필터를 그대로 넘겨 받는다.
pub fn export_enrollments(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
    filter: &crate::model::EnrollmentFilter,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let list = repo::enrollment::list(conn, workspace_id, items, filter)?;

    let mut headers: Vec<String> = vec![
        "부서".into(),
        "학년".into(),
        "반".into(),
        "번호".into(),
        "이름".into(),
        "지원유형".into(),
    ];
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("합계".into());
    headers.push("상태".into());
    headers.push("변경사유".into());
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();

    let money_from = 6;
    let money_cols: Vec<usize> = (money_from..money_from + items.len() + 1).collect();

    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|e| {
            let mut row = vec![
                e.dept_label.clone(),
                e.grade.to_string(),
                e.class_no.to_string(),
                e.student_no.to_string(),
                e.name.clone(),
                program_label(&e.programs),
            ];
            for it in items {
                let amount = e
                    .fees
                    .iter()
                    .find(|f| f.item_code == it.code)
                    .map(|f| f.amount)
                    .unwrap_or(0);
                row.push(amount.to_string());
            }
            row.push(e.total.to_string());
            row.push(if e.status == "ACTIVE" { "수강중".into() } else { "취소".into() });
            row.push(e.change_reason.clone());
            row
        })
        .collect();

    let path = write::export_path(dir, "수강생명단", scope)?;
    let n = rows.len();
    write::write_sheet(&path, "수강생명단", &head, &rows, &money_cols, None)?;
    Ok(done(path, n))
}

/// 오류 목록만 따로 내려받는다 — 사용자가 그 파일을 보고 원본을 고칠 수 있게.
pub fn export_issues(issues: &[RowIssue], headers: &[String], dir: &Path) -> AppResult<ExportResult> {
    let mut head: Vec<String> = vec!["행".to_string()];
    head.extend(headers.iter().cloned());
    head.push("사유".to_string());
    let head_ref: Vec<&str> = head.iter().map(|s| s.as_str()).collect();

    let rows: Vec<Vec<String>> = issues
        .iter()
        .map(|i| {
            let mut r = vec![i.row.to_string()];
            for c in 0..headers.len() {
                r.push(i.cells.get(c).cloned().unwrap_or_default());
            }
            r.push(i.reason.clone());
            r
        })
        .collect();

    let path = write::export_path(dir, "업로드오류", &[])?;
    let n = rows.len();
    write::write_sheet(&path, "업로드오류", &head_ref, &rows, &[], None)?;
    Ok(done(path, n))
}

fn program_label(programs: &[String]) -> String {
    let v = programs.iter().any(|p| p == "VOUCHER");
    let f = programs.iter().any(|p| p == "FREE_VOUCHER");
    match (v, f) {
        (true, true) => "방과후 이용권 + 자유수강권".to_string(),
        (true, false) => "방과후 이용권".to_string(),
        (false, true) => "자유수강권".to_string(),
        (false, false) => "일반".to_string(),
    }
}

/// 대상학년 문구를 화면에서도 쓰려고 밖으로 낸다.
pub fn target_grade_text(target_grades: &str) -> String {
    grade_text(&parse_grades(target_grades))
}

#[cfg(test)]
mod roundtrip_tests;

#[cfg(test)]
#[path = "class_no_tests.rs"]
mod class_no_tests;

/// 학생별 징수 내역 (행정자료, v0.1.3).
///
/// **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`)를 그대로 쓴다.
/// 지원금이 어느 재원에서 나가는지는 수익자·이용권·자유수강권 화면에서 본다.
///
/// ## 화면 필터를 그대로 반영한다
///
/// 행정자료 가운데 수익자·이용권·자유수강권·품의는 필터를 무시하고 전체를 낸다 —
/// 학교 밖으로 나가는 문서라 일부만 담긴 파일이 만들어지면 안 되기 때문이다.
/// 이 자료는 다르다. 담당자가 "2학년만 뽑아 담임에게 확인" 하는 식으로 쓰므로
/// **필터를 그대로 반영한다.** 대신 전체 자료로 오해하지 않게 **파일 이름과 시트
/// 첫 줄에 적용된 조건을 적는다.**
///
/// 정산 최신을 요구하지 않는다 — 정산 전에 금액을 대조하는 자료이기 때문이다.
pub fn export_fee_report(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
    filter: &crate::model::EnrollmentFilter,
    scope: &[&str],
    cond: &str,
    dir: &Path,
) -> AppResult<ExportResult> {
    let list = repo::enrollment::list_by_student(conn, workspace_id, items, filter)?;

    let mut headers: Vec<String> = vec![
        "학년".into(),
        "반".into(),
        "번호".into(),
        "이름".into(),
        "지원유형".into(),
        "부서".into(),
    ];
    headers.extend(items.iter().map(|i| i.name.clone()));
    headers.push("합계".into());
    headers.push("수강상태".into());
    let head: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();

    let money_from = 6;
    let money_cols: Vec<usize> = (money_from..money_from + items.len() + 1).collect();

    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|e| {
            let mut row = vec![
                e.grade.to_string(),
                e.class_no.clone(),
                e.student_no.to_string(),
                e.name.clone(),
                program_label(&e.programs),
                e.dept_label.clone(),
            ];
            for it in items {
                let amount = e
                    .fees
                    .iter()
                    .find(|f| f.item_code == it.code)
                    .map(|f| f.amount)
                    .unwrap_or(0);
                row.push(amount.to_string());
            }
            row.push(e.total.to_string());
            row.push(status_label(&e.status));
            row
        })
        .collect();

    let n = rows.len();

    // 맨 아래에 합계 줄을 붙인다. 조건을 시트 **맨 위**에 얹으면 헤더가 한 줄
    // 밀려서, 이 파일을 다시 읽는 도구가 열을 못 찾는다. 그래서 아래에 둔다.
    let mut rows = rows;
    if n > 0 {
        let mut foot = vec![
            "합계".to_string(),
            String::new(),
            String::new(),
            format!("학생 {}명", students_of(&list)),
            format!("수강 {n}건"),
            if cond.trim().is_empty() {
                "조건 없음(전체)".to_string()
            } else {
                cond.trim().to_string()
            },
        ];
        for it in items {
            foot.push(
                list.iter()
                    .map(|e| {
                        e.fees
                            .iter()
                            .find(|f| f.item_code == it.code)
                            .map(|f| f.amount)
                            .unwrap_or(0)
                    })
                    .sum::<i64>()
                    .to_string(),
            );
        }
        foot.push(list.iter().map(|e| e.total).sum::<i64>().to_string());
        foot.push(String::new());
        rows.push(foot);
    }

    // 조건을 파일 이름에도 넣는다 — 파일만 보고도 전체가 아닌 것을 알아야 한다.
    let mut name_scope: Vec<&str> = scope.to_vec();
    if !cond.trim().is_empty() {
        name_scope.push(cond);
    }
    let path = write::export_path(dir, "학생별징수내역", &name_scope)?;
    let widths = vec![
        write::DEFAULT_WIDTH; head.len()
    ];
    write::write_sheet_sized(
        &path,
        "학생별징수내역",
        &head,
        &rows,
        &money_cols,
        None,
        &widths,
        true, // 마지막 줄을 합계로 강조
    )?;
    Ok(done(path, n))
}

/// 중복을 뺀 학생 수.
fn students_of(list: &[crate::model::Enrollment]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for e in list {
        seen.insert(e.student_id);
    }
    seen.len()
}

pub(crate) fn status_label(status: &str) -> String {
    if status == "ACTIVE" {
        "수강중".into()
    } else {
        "수강취소".into()
    }
}
