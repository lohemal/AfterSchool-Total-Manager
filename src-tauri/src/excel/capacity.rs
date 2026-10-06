//! 부서별 수강현황 Excel (v0.1.7).
//!
//! ## 여기서는 셈하지 않는다
//!
//! 남은 자리도 충원율도 참여율도 **`repo::capacity` 가 낸 값을 그대로 적는다.**
//! 화면과 파일의 숫자가 다르면 어느 쪽을 믿어야 하는지 알 수 없게 되고, 같은
//! 계산을 두 곳에 두면 한쪽만 고쳐지는 날이 온다.
//!
//! ## 화면 필터를 따르지 않는다
//!
//! 이 파일은 **공식 통계 자료**다. 담당자가 화면에서 '월요일'만 보고 있었다는
//! 이유로 결재에 올라갈 통계에서 다른 요일이 빠지면 그대로 틀린 자료가 된다.
//! 그래서 늘 작업공간 전체를 낸다 (수익자·이용권·품의와 같은 원칙이다).

use std::path::Path;

use rusqlite::Connection;

use crate::domain::allowed_grade_text;
use crate::error::AppResult;
use crate::excel::write::{self, SheetSpec};
use crate::excel::ExportResult;
use crate::model::{CapacityStatus, DeptCapacityRow};
use crate::repo;

/// 화면에 쓰는 것과 같은 말. 색이 아니라 **글로** 상태를 알린다.
pub fn status_text(status: CapacityStatus) -> &'static str {
    match status {
        CapacityStatus::Unset => "정원 미설정",
        CapacityStatus::Open => "모집 가능",
        CapacityStatus::Full => "정원 도달",
        CapacityStatus::Over => "정원 초과",
    }
}

fn num(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "-".into())
}

fn pct(v: Option<f64>) -> String {
    v.map(|n| format!("{n:.1}%")).unwrap_or_else(|| "-".into())
}

fn dept_sheet(rows: &[DeptCapacityRow]) -> SheetSpec<'static> {
    SheetSpec {
        name: "부서별 현황",
        headers: [
            "부서명",
            "반명",
            "강사명",
            "요일",
            "대상 학년",
            "정원",
            "현재 수강",
            "남은 자리",
            "충원율",
            "상태",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows: rows
            .iter()
            .map(|r| {
                vec![
                    r.name.clone(),
                    r.class_name.clone(),
                    r.teacher.clone(),
                    r.days.clone(),
                    allowed_grade_text(&r.allowed_grades),
                    num(r.capacity),
                    r.current_count.to_string(),
                    num(r.remaining),
                    pct(r.fill_rate),
                    status_text(r.status).to_string(),
                ]
            })
            .collect(),
        // 정원·수강·남은 자리는 금액이 아니다. 천 단위 쉼표를 붙이면 사람 수가
        // 돈처럼 보인다.
        money_cols: Vec::new(),
        widths: vec![16.0, 8.0, 12.0, 10.0, 14.0, 8.0, 10.0, 10.0, 10.0, 12.0],
        bold_last_row: false,
    }
}

fn grade_sheet(stats: &crate::model::CapacityStats) -> SheetSpec<'static> {
    SheetSpec {
        name: "학년별 현황",
        headers: [
            "학년",
            "수강 학생 수",
            "수강 건수",
            "학년 전체 학생 수",
            "참여율",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows: stats
            .grades
            .iter()
            .map(|g| {
                vec![
                    format!("{}학년", g.grade),
                    g.students.to_string(),
                    g.enrollments.to_string(),
                    g.total_students.to_string(),
                    pct(g.join_rate),
                ]
            })
            .collect(),
        money_cols: Vec::new(),
        widths: vec![10.0, 14.0, 12.0, 18.0, 10.0],
        bold_last_row: false,
    }
}

fn weekday_sheet(stats: &crate::model::CapacityStats) -> SheetSpec<'static> {
    let mut rows: Vec<Vec<String>> = stats
        .weekdays
        .iter()
        .map(|w| {
            vec![
                w.day.clone(),
                w.classes.to_string(),
                w.enrollments.to_string(),
                w.classes_with_capacity.to_string(),
                w.total_capacity.to_string(),
                w.open_seats.to_string(),
            ]
        })
        .collect();

    // 합계 줄을 두지 않는 까닭을 파일 안에 적는다. 파일만 따로 받아 본 사람이
    // 세로로 더해 보고 전체 수강 건수와 다르다며 놀라지 않도록.
    rows.push(vec![String::new(); 6]);
    rows.push(vec![
        format!(
            "※ 여러 요일에 운영하는 반은 각 요일에 모두 셉니다. 그래서 세로 합계는 \
             전체 수강 건수({}건)보다 클 수 있습니다.",
            stats.summary.total_enrollments
        ),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ]);

    SheetSpec {
        name: "요일별 현황",
        headers: [
            "요일",
            "운영 반 수",
            "수강 건수",
            "정원 설정 반 수",
            "설정된 총 정원",
            "남은 자리",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows,
        money_cols: Vec::new(),
        widths: vec![10.0, 12.0, 12.0, 16.0, 16.0, 12.0],
        bold_last_row: false,
    }
}

pub fn export_capacity(
    conn: &Connection,
    workspace_id: i64,
    scope: &[&str],
    dir: &Path,
) -> AppResult<ExportResult> {
    let stats = repo::capacity::stats(conn, workspace_id)?;
    let sheets = [
        dept_sheet(&stats.rows),
        grade_sheet(&stats),
        weekday_sheet(&stats),
    ];
    let path = write::export_path(dir, "부서별 수강현황", scope)?;
    let saved = write::write_book(&path, &sheets)?;
    Ok(ExportResult {
        path: saved.to_string_lossy().to_string(),
        name: saved
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        rows: stats.rows.len(),
    })
}
