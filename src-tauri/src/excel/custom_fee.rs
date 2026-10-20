//! 맞춤형 강사료 Excel (v0.1.8).
//!
//! **여기서는 날짜도 금액도 다시 셈하지 않는다.** `repo::custom_fee::report`
//! 가 낸 값을 그대로 적는다. 화면과 파일이 각자 계산하면 같은 반의 강사료가
//! 두 숫자로 갈린다.
//!
//! 화면 필터를 따르지 않는다 — 강사료 지급 근거로 올라가는 자료라 반이
//! 조용히 빠지면 안 된다.

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;
use crate::excel::write::{self, SheetSpec};
use crate::excel::ExportResult;
use crate::model::{CustomFeeReport, SessionKind};
use crate::repo;

fn kind_text(k: SessionKind) -> &'static str {
    match k {
        SessionKind::Normal => "정상",
        SessionKind::Closed => "휴강",
        SessionKind::Extra => "보강",
    }
}

fn dept_sheet(rep: &CustomFeeReport) -> SheetSpec<'static> {
    SheetSpec {
        name: "부서별 강사료",
        headers: [
            "부서명",
            "반명",
            "강사명",
            "요일",
            "기본 차시",
            "예정 수업일",
            "휴강",
            "추가/보강",
            "인정 수업일",
            "총 인정 차시",
            "차시당 단가",
            "강사료",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows: rep
            .rows
            .iter()
            .map(|r| {
                vec![
                    r.name.clone(),
                    r.class_name.clone(),
                    r.teacher.clone(),
                    r.days.clone(),
                    r.base_sessions.to_string(),
                    r.planned_days.to_string(),
                    r.closed_days.to_string(),
                    r.extra_days.to_string(),
                    r.counted_days.to_string(),
                    r.total_sessions.to_string(),
                    r.rate.to_string(),
                    r.fee.to_string(),
                ]
            })
            .collect(),
        // 단가와 강사료만 돈이다. 차시와 날짜 수는 사람이 세는 수이므로
        // 천 단위 쉼표를 붙이지 않는다.
        money_cols: vec![10, 11],
        widths: vec![
            16.0, 8.0, 12.0, 10.0, 10.0, 12.0, 8.0, 12.0, 12.0, 12.0, 12.0, 14.0,
        ],
        bold_last_row: false,
    }
}

fn session_sheet(rep: &CustomFeeReport) -> SheetSpec<'static> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for r in &rep.rows {
        for s in &r.sessions_list {
            rows.push(vec![
                r.name.clone(),
                r.class_name.clone(),
                r.teacher.clone(),
                s.date.clone(),
                s.weekday.clone(),
                kind_text(s.kind).to_string(),
                s.sessions.to_string(),
                s.reason.clone(),
            ]);
        }
    }
    SheetSpec {
        name: "수업일 상세",
        headers: [
            "부서명", "반명", "강사명", "날짜", "요일", "구분", "차시", "사유/메모",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows,
        money_cols: Vec::new(),
        widths: vec![16.0, 8.0, 12.0, 12.0, 8.0, 8.0, 8.0, 28.0],
        bold_last_row: false,
    }
}

/// 휴업일과 검토가 필요한 기록을 한 장에 함께 담는다.
///
/// 왜 그 날짜가 강사료에서 빠졌는지 파일만 보고도 알 수 있어야 한다.
fn note_sheet(rep: &CustomFeeReport) -> SheetSpec<'static> {
    let mut rows: Vec<Vec<String>> = rep
        .closures
        .iter()
        .map(|c| {
            vec![
                "공통 휴업일".to_string(),
                c.date.clone(),
                c.weekday.clone(),
                String::new(),
                c.reason.clone(),
            ]
        })
        .collect();
    for o in &rep.orphans {
        rows.push(vec![
            "검토 필요".to_string(),
            o.date.clone(),
            String::new(),
            o.department_label.clone(),
            format!("{} (적어 둔 값 {}차시 · {})", o.why, o.sessions, o.reason),
        ]);
    }
    if rows.is_empty() {
        rows.push(vec![
            "—".to_string(),
            String::new(),
            String::new(),
            String::new(),
            "등록된 휴업일이 없고 검토할 기록도 없습니다.".to_string(),
        ]);
    }
    SheetSpec {
        name: "휴업일·검토",
        headers: ["구분", "날짜", "요일", "부서", "내용"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rows,
        money_cols: Vec::new(),
        widths: vec![12.0, 12.0, 8.0, 16.0, 46.0],
        bold_last_row: false,
    }
}

pub fn export_custom_fee(
    conn: &Connection,
    workspace_id: i64,
    dir: &Path,
) -> AppResult<ExportResult> {
    // 맞춤형이 아닌 부서는 담지 않는다 — 강사료가 0인 줄이 지급 자료를 덮는다.
    let rep = repo::custom_fee::report(conn, workspace_id, false)?;
    let sheets = [dept_sheet(&rep), session_sheet(&rep), note_sheet(&rep)];
    let scope = [rep.year_name.as_str(), rep.workspace_name.as_str()];
    let path = write::export_path(dir, "맞춤형강사료", &scope)?;
    let saved = write::write_book(&path, &sheets)?;
    Ok(ExportResult {
        path: saved.to_string_lossy().to_string(),
        name: saved
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        rows: rep.rows.len(),
    })
}
