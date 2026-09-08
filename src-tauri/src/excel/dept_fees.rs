//! 부서별 학생 금액 Excel 일괄 수정 (v0.1.4).
//!
//! 부서 하나를 고른 뒤 그 부서 수강생의 항목별 금액을 Excel로 한꺼번에 고친다.
//! [금액 수정 양식 받기] → 파일에서 금액 수정 → [금액 수정 파일 불러오기] →
//! 미리보기 확인 → [변경사항 반영].
//!
//! ## 지키는 것
//!
//! * **고른 부서 안에서만 학생을 찾는다.** 다른 부서는 건드리지 않는다.
//! * **학년 + 반 + 번호 + 이름이 모두 맞아야** 짝을 짓는다. 추측하지 않는다.
//!   (수강 업로드는 이름을 경고로만 보지만, 여기서는 금액을 고치는 일이므로
//!   더 엄하게 본다.)
//! * **반은 문자다.** `'01'`과 `'1'`은 서로 다른 반이고, 프로그램이 같은 값으로
//!   바꾸지 않는다. 양식의 반 칸은 문자 셀로 쓰므로 왕복해도 그대로다.
//! * **파일에 없는 학생은 건드리지 않는다.** 30명 가운데 한 줄만 있어도 된다.
//! * **네 금액이 모두 있어야 한다.** 빈 칸을 0원이나 '그대로'로 짐작하지 않고
//!   그 줄을 오류로 표시한다.
//! * **먼저 파일 전체를 본다.** 오류가 하나라도 있으면 아무것도 반영하지 않는다.
//! * **값이 같은 칸은 쓰지 않는다.** 그래서 같은 파일을 다시 올려도 자료판이
//!   올라가지 않고 정산이 낡음이 되지 않는다.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;

use crate::domain::class_no;
use crate::error::{AppError, AppResult};
use crate::excel::write::{self, DEFAULT_WIDTH};
use crate::excel::{done, read, ExportResult};
use crate::model::{CostItem, FeeChange, FeePreview, FeeRowIssue};
use crate::repo;
use crate::repo::enrollment::StudentFeeEdit;

/// 학생을 가리키는 고정 열. 뒤에 비용항목 열이 붙는다.
const FIXED_COLS: &[&str] = &["학년", "반", "번호", "이름"];

fn headers(items: &[CostItem]) -> Vec<String> {
    let mut v: Vec<String> = FIXED_COLS.iter().map(|s| s.to_string()).collect();
    v.extend(items.iter().map(|i| i.name.clone()));
    v
}

/// 고른 부서의 **지금 수강생과 지금 금액**을 양식으로 낸다.
///
/// 빈 양식이 아니라 현재 값이 채워진 파일이다 — 고칠 학생의 칸만 바꿔 다시
/// 올리면 된다.
///
/// 수강중인 학생만 담는다. 취소한 수강의 금액은 취소 창이나 수강생 명단에서
/// 고친다 — 한 학생·한 부서에 취소 건이 여럿 있을 수 있어 학년·반·번호·이름
/// 만으로는 어느 건인지 정해지지 않기 때문이다.
pub fn template(
    conn: &Connection,
    workspace_id: i64,
    department_id: i64,
    items: &[CostItem],
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let (dname, dclass) = repo::department::name_of(conn, department_id)?;
    let list = repo::enrollment::by_department(conn, workspace_id, department_id, items)?;

    let head = headers(items);
    let head_ref: Vec<&str> = head.iter().map(|s| s.as_str()).collect();
    let money: Vec<usize> = (FIXED_COLS.len()..FIXED_COLS.len() + items.len()).collect();

    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|e| {
            let mut row = vec![
                e.grade.to_string(),
                e.class_no.clone(),
                e.student_no.to_string(),
                e.name.clone(),
            ];
            for it in items {
                row.push(
                    e.fees
                        .iter()
                        .find(|f| f.item_code == it.code)
                        .map(|f| f.amount)
                        .unwrap_or(0)
                        .to_string(),
                );
            }
            row
        })
        .collect();

    let label = repo::enrollment::dept_label(&dname, &dclass);
    let mut name_scope: Vec<&str> = scope.to_vec();
    name_scope.push(&label);
    let path = write::export_path(dir, "금액수정양식", &name_scope)?;
    let widths = vec![DEFAULT_WIDTH; head.len()];
    let n = rows.len();
    write::write_sheet_sized(
        &path,
        "금액 수정",
        &head_ref,
        &rows,
        &money,
        None,
        &widths,
        false,
    )?;
    Ok(done(path, n))
}

/// 파일을 읽어 무엇이 바뀌는지 만든다. **DB는 건드리지 않는다.**
pub fn preview(
    conn: &Connection,
    workspace_id: i64,
    department_id: i64,
    items: &[CostItem],
    path: &Path,
) -> AppResult<(FeePreview, Vec<StudentFeeEdit>)> {
    let (dname, dclass) = repo::department::name_of(conn, department_id)?;
    let dept_label = repo::enrollment::dept_label(&dname, &dclass);
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let sheet = read::read_first_sheet(path)?;
    let c_grade = sheet.require("학년")?;
    let c_class = sheet.require("반")?;
    let c_no = sheet.require("번호")?;
    let c_name = sheet.require("이름")?;
    // 네 금액 열이 모두 있어야 한다 — 하나가 없으면 그 항목을 짐작해야 한다.
    let mut c_items: Vec<(usize, &CostItem)> = Vec::new();
    for it in items {
        c_items.push((sheet.require(&it.name)?, it));
    }

    // 이 부서의 수강중인 학생만 대상이다.
    let list = repo::enrollment::by_department(conn, workspace_id, department_id, items)?;
    let mut by_key: HashMap<(i64, String, i64, String), &crate::model::Enrollment> = HashMap::new();
    for e in &list {
        by_key.insert(
            (e.grade, e.class_no.clone(), e.student_no, e.name.clone()),
            e,
        );
    }

    let mut errors: Vec<FeeRowIssue> = Vec::new();
    let mut unmatched: Vec<FeeRowIssue> = Vec::new();
    let mut changes: Vec<FeeChange> = Vec::new();
    let mut edits: Vec<StudentFeeEdit> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut unchanged = 0i64;
    let mut total = 0i64;

    for (line, cells) in &sheet.rows {
        total += 1;
        let raw_grade = sheet.cell(cells, Some(c_grade)).trim().to_string();
        let raw_class = class_no::normalize(sheet.cell(cells, Some(c_class)));
        let raw_no = sheet.cell(cells, Some(c_no)).trim().to_string();
        let name = sheet.cell(cells, Some(c_name)).trim().to_string();
        let label = format!("{raw_grade}학년 {raw_class}반 {raw_no}번 {name}");

        let (Some(grade), Some(student_no)) =
            (read::parse_int(&raw_grade), read::parse_int(&raw_no))
        else {
            errors.push(issue(*line, &label, "학년과 번호는 숫자여야 합니다."));
            continue;
        };
        if raw_class.is_empty() {
            errors.push(issue(*line, &label, "반이 비어 있습니다."));
            continue;
        }
        if name.is_empty() {
            errors.push(issue(*line, &label, "이름이 비어 있습니다."));
            continue;
        }

        // 금액 네 칸 — 비어 있으면 짐작하지 않고 오류로 둔다.
        let mut fees: Vec<crate::model::Fee> = Vec::new();
        let mut bad: Option<String> = None;
        for (col, it) in &c_items {
            let raw = sheet.cell(cells, Some(*col)).trim();
            if raw.is_empty() {
                bad = Some(format!(
                    "{}이(가) 비어 있습니다. 0원이면 0을 적어 주세요.",
                    it.name
                ));
                break;
            }
            match read::parse_amount(raw) {
                Some(v) if v >= 0 => fees.push(crate::model::Fee {
                    item_code: it.code.clone(),
                    amount: v,
                }),
                Some(_) => {
                    bad = Some(format!("{}은(는) 0원 이상이어야 합니다.", it.name));
                    break;
                }
                None => {
                    bad = Some(format!("{}을(를) 숫자로 읽지 못했습니다: {raw}", it.name));
                    break;
                }
            }
        }
        if let Some(msg) = bad {
            errors.push(issue(*line, &label, &msg));
            continue;
        }

        let key = (grade, raw_class.clone(), student_no, name.clone());
        let Some(e) = by_key.get(&key) else {
            unmatched.push(issue(
                *line,
                &label,
                &format!(
                    "{dept_label} 수강생 가운데 학년·반·번호·이름이 모두 맞는 학생이 없습니다."
                ),
            ));
            continue;
        };
        if !seen.insert(e.id) {
            errors.push(issue(*line, &label, "같은 학생이 파일 안에 두 번 있습니다."));
            continue;
        }

        // 실제로 달라진 칸만 모은다 — 같은 값을 다시 쓰면 자료판이 올라간다.
        let mut moved = false;
        for f in &fees {
            let before = e
                .fees
                .iter()
                .find(|x| x.item_code == f.item_code)
                .map(|x| x.amount)
                .unwrap_or(0);
            if before == f.amount {
                continue;
            }
            moved = true;
            let item_name = items
                .iter()
                .find(|i| i.code == f.item_code)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            changes.push(FeeChange {
                enrollment_id: e.id,
                student_label: repo::enrollment::student_label(
                    e.grade,
                    &e.class_no,
                    e.student_no,
                    &e.name,
                ),
                item_code: f.item_code.clone(),
                item_name,
                before,
                after: f.amount,
            });
        }
        if moved {
            edits.push(StudentFeeEdit {
                enrollment_id: e.id,
                fees,
            });
        } else {
            unchanged += 1;
        }
    }

    let students = edits.len() as i64;
    Ok((
        FeePreview {
            token: String::new(), // 명령 계층이 채운다 (오류가 없을 때만)
            file_name,
            dept_label,
            total,
            students,
            cells: changes.len() as i64,
            unchanged,
            unmatched,
            errors,
            changes,
        },
        edits,
    ))
}

fn issue(line: usize, label: &str, message: &str) -> FeeRowIssue {
    FeeRowIssue {
        line,
        label: label.to_string(),
        message: message.to_string(),
    }
}

/// 미리보기에서 확인한 변경만 쓴다.
///
/// `repo::enrollment::save_student_fees`가 값이 같은 건을 건너뛰므로, 실제로
/// 달라진 것이 없으면 `charge`도 `enrollment`도 손대지 않는다 — 자료판 트리거가
/// 발동하지 않아 정산이 낡음이 되지 않는다.
///
/// `Db::write`가 감싸므로 중간에 실패하면 전부 되돌아간다.
pub fn apply(
    conn: &Connection,
    workspace_id: i64,
    department_id: i64,
    edits: &[StudentFeeEdit],
    reason: &str,
    items: &[CostItem],
) -> AppResult<crate::model::FeeApplyResult> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::invalid("변경사유를 입력해 주세요."));
    }

    // 토큰을 만든 뒤 화면에서 부서를 바꿨거나 자료가 달라졌을 수 있다.
    // 고른 부서의 수강생이 아닌 건은 절대 쓰지 않는다.
    let allowed: HashSet<i64> =
        repo::enrollment::by_department(conn, workspace_id, department_id, items)?
            .iter()
            .map(|e| e.id)
            .collect();
    for e in edits {
        if !allowed.contains(&e.enrollment_id) {
            return Err(AppError::invalid(
                "고른 부서의 수강생이 아닌 자료가 있습니다. 파일을 다시 불러와 주세요.",
            ));
        }
    }

    // 바뀌는 칸 수는 지금 값과 견주어 여기서 센다 — 미리보기 뒤에 누가 금액을
    // 고쳤을 수도 있으므로, 보고하는 숫자는 실제로 쓴 것이어야 한다.
    let mut cells = 0i64;
    for e in edits {
        let before = repo::enrollment::get(conn, e.enrollment_id, items)?;
        for f in &e.fees {
            let b = before
                .fees
                .iter()
                .find(|x| x.item_code == f.item_code)
                .map(|x| x.amount)
                .unwrap_or(0);
            if b != f.amount {
                cells += 1;
            }
        }
    }

    let students = repo::enrollment::save_student_fees(conn, edits, reason, items)?;
    Ok(crate::model::FeeApplyResult { students, cells })
}
