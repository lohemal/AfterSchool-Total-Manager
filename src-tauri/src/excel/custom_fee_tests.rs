//! 맞춤형 강사료 Excel 시험 (v0.1.8).
//!
//! 지키려는 것 하나 — **파일의 숫자가 화면의 숫자와 같다.** Excel 쪽에서
//! 날짜도 금액도 다시 셈하지 않으므로, 집계 서비스가 낸 값이 그대로 적혀
//! 있어야 한다.

use std::path::PathBuf;

use crate::db::Db;
use crate::excel::{self, read};
use crate::model::{DepartmentInput, Fee, WorkspaceInput};
use crate::repo;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-custom-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn 세_장이_화면과_같은_숫자를_담는다() {
    let db = Db::open_memory().unwrap();
    let year = db
        .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
        .unwrap();
    let ws = db
        .write(|c| {
            repo::year::create_workspace(
                c,
                year,
                &WorkspaceInput {
                    name: "2026년 10월".into(),
                    start_date: "2026-10-01".into(),
                    end_date: "2026-10-31".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    let d = db
        .write(|c| {
            repo::department::create(
                c,
                ws,
                &DepartmentInput {
                    name: "미술".into(),
                    class_name: Some("A".into()),
                    teacher: Some("홍길동".into()),
                    days: Some("화".into()),
                    note: None,
                    fees: vec![Fee {
                        item_code: "INSTRUCTOR".into(),
                        amount: 30_000,
                    }],
                    capacity: None,
                    allowed_grades: Vec::new(),
                    custom_sessions: 2,
                },
            )
        })
        .unwrap();
    db.write(|c| repo::custom_fee::session_set(c, d, "2026-10-20", 0, "공휴일"))
        .unwrap();
    db.write(|c| repo::custom_fee::session_set(c, d, "2026-10-23", 1, "보강"))
        .unwrap();
    db.write(|c| repo::custom_fee::closure_add(c, ws, "2026-10-09", "한글날"))
        .unwrap();

    // 화면이 쓰는 것과 같은 집계
    let rep = db
        .read(|c| repo::custom_fee::report(c, ws, false))
        .unwrap();
    // 2 + 2 + 0(휴강) + 2 + 1(보강) = 7차시
    assert_eq!(rep.rows[0].total_sessions, 7);
    assert_eq!(rep.rows[0].fee, 280_000);

    let dir = tmp_dir("sheets");
    let out = db
        .read(|c| excel::export_custom_fee(c, ws, &dir))
        .unwrap();
    let path = PathBuf::from(&out.path);
    assert!(path.exists());
    assert!(out.name.contains("맞춤형강사료"), "{}", out.name);
    assert!(out.name.contains("2026년10월"), "{}", out.name);

    // ── 1장 부서별 강사료
    let s1 = read::read_sheet_at(&path, 0).unwrap();
    let c_name = s1.require("부서명").unwrap();
    let c_total = s1.require("총 인정 차시").unwrap();
    let c_rate = s1.require("차시당 단가").unwrap();
    let c_fee = s1.require("강사료").unwrap();
    let c_planned = s1.require("예정 수업일").unwrap();
    let c_closed = s1.require("휴강").unwrap();
    let c_extra = s1.require("추가/보강").unwrap();
    let 줄 = s1
        .rows
        .iter()
        .find(|(_, cells)| cells.get(c_name).map(|v| v.as_str()) == Some("미술"))
        .expect("미술 줄이 없다")
        .1
        .clone();
    assert_eq!(줄[c_planned], "4");
    assert_eq!(줄[c_closed], "1");
    assert_eq!(줄[c_extra], "1");
    assert_eq!(줄[c_total], "7", "화면과 총 차시가 다르다");
    assert_eq!(줄[c_rate], "40000");
    assert_eq!(줄[c_fee], "280000", "화면과 강사료가 다르다");
    assert_eq!(
        줄[c_fee],
        rep.rows[0].fee.to_string(),
        "Excel 이 금액을 다시 셈했다"
    );

    // ── 2장 수업일 상세
    let s2 = read::read_sheet_at(&path, 1).unwrap();
    let d_date = s2.require("날짜").unwrap();
    let d_kind = s2.require("구분").unwrap();
    let d_sessions = s2.require("차시").unwrap();
    let d_reason = s2.require("사유/메모").unwrap();
    let find = |date: &str| {
        s2.rows
            .iter()
            .find(|(_, c)| c.get(d_date).map(|v| v.as_str()) == Some(date))
            .unwrap_or_else(|| panic!("{date} 줄이 없다"))
            .1
            .clone()
    };
    assert_eq!(find("2026-10-20")[d_kind], "휴강");
    assert_eq!(find("2026-10-20")[d_sessions], "0");
    assert_eq!(find("2026-10-20")[d_reason], "공휴일");
    assert_eq!(find("2026-10-23")[d_kind], "보강");
    assert_eq!(find("2026-10-23")[d_sessions], "1");
    assert_eq!(find("2026-10-06")[d_kind], "정상");
    assert_eq!(find("2026-10-06")[d_sessions], "2");
    assert_eq!(s2.rows.len(), rep.rows[0].sessions_list.len());

    // ── 3장 휴업일·검토 — 왜 그날이 빠졌는지 파일만 보고 알 수 있어야 한다
    let s3 = read::read_sheet_at(&path, 2).unwrap();
    let n_date = s3.require("날짜").unwrap();
    assert!(
        s3.rows
            .iter()
            .any(|(_, c)| c.get(n_date).map(|v| v.as_str()) == Some("2026-10-09")),
        "공통 휴업일이 파일에 없다"
    );
}
