//! 009 승격 시험 — 방과후드림 표를 더해도 기존 업무 자료 · 자료판 · 정산 상태가
//! 그대로인가.
//!
//! 008 까지만 적용한 DB 에 학생 · 부서 · 수강 · 지원 · 정산(실제 엔진)을 만들어 두고,
//! 앱이 열 때 009 를 거치게 한다. 학생은 모두 가명이다.

use rusqlite::Connection;

use crate::db::{migrate, Db};
use crate::domain::Program;
use crate::model::{DepartmentInput, EligibilityInput, EnrollmentInput, Fee, GrantInput, StudentInput, WorkspaceInput};
use crate::repo;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-mig009-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 008 까지 적용하고 업무 자료와 정산을 만든다. (작업공간 id)
fn v8_with_settlement(path: &std::path::Path) -> i64 {
    let mut c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    migrate::run_up_to(&mut c, 8).unwrap();
    let year = repo::year::create_year(&c, 2026, "2026학년도").unwrap();
    let ws = repo::year::create_workspace(
        &c,
        year,
        &WorkspaceInput {
            name: "10월".into(),
            start_date: "2026-10-01".into(),
            end_date: "2026-10-31".into(),
            note: None,
        },
    )
    .unwrap();
    let items = repo::cost_items(&c).unwrap();
    for p in [Program::Voucher, Program::FreeVoucher] {
        repo::policy::save(&c, year, p.code(), 1_000_000, false, "", &[]).unwrap();
    }
    let dept = repo::department::create(
        &c,
        ws,
        &DepartmentInput {
            name: "항공드론".into(),
            class_name: Some("B".into()),
            teacher: None,
            days: None,
            note: None,
            fees: vec![
                Fee { item_code: "INSTRUCTOR".into(), amount: 35_000 },
                Fee { item_code: "OPERATION".into(), amount: 1_500 },
                Fee { item_code: "MATERIAL".into(), amount: 25_000 },
            ],
            capacity: None,
            allowed_grades: Vec::new(),
            custom_sessions: 0,
        },
    )
    .unwrap();
    for (no, name, voucher, free) in [(1, "가온", 0, 0), (2, "나래", 40_000, 0), (3, "다솜", 10_000, 30_000)] {
        let s = repo::student::create(
            &c,
            year,
            &StudentInput {
                grade: 3,
                class_no: "가람".into(),
                student_no: no,
                name: name.into(),
                note: None,
            },
        )
        .unwrap();
        repo::enrollment::create(
            &c,
            ws,
            &EnrollmentInput {
                student_id: s,
                department_id: dept,
                fees: Vec::new(),
                reason: None,
            },
            &items,
        )
        .unwrap();
        for (p, limit) in [(Program::Voucher, voucher), (Program::FreeVoucher, free)] {
            if limit > 0 {
                repo::eligibility::create(
                    &c,
                    year,
                    &EligibilityInput {
                        student_id: s,
                        program: p.code().into(),
                        valid_from: None,
                        valid_to: None,
                        note: None,
                    },
                )
                .unwrap();
                repo::policy::grant_save(
                    &c,
                    year,
                    &GrantInput {
                        student_id: s,
                        program: p.code().into(),
                        period_id: None,
                        amount: limit,
                        reason: Some("시험".into()),
                    },
                )
                .unwrap();
            }
        }
    }
    repo::settle::generate(&c, ws).unwrap();
    assert_eq!(repo::settle::status(&c, ws).unwrap().state, "FRESH");
    ws
}

/// 모든 표(방과후드림 표 제외)의 정렬한 내용 · 자료판 · 정산 상태.
fn fingerprint(c: &Connection, ws: i64) -> Vec<String> {
    let tables: Vec<String> = c
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'dream_%' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut out = Vec::new();
    for t in tables {
        let mut st = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        let mut lines = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            let mut line = String::new();
            for i in 0..n {
                line.push_str(&format!("{:?}|", r.get_ref(i).unwrap()));
            }
            lines.push(line);
        }
        lines.sort();
        out.push(format!("{t}={}", lines.join("\n")));
    }
    out.push(repo::settle::status(c, ws).unwrap().state);
    out
}

#[test]
fn 열면_009_가_한_번만_적용되고_기존_자료와_정산은_그대로다() {
    let dir = tmp_dir("keep");
    let path = dir.join("old.db");
    let ws = v8_with_settlement(&path);
    let before = {
        let c = Connection::open(&path).unwrap();
        assert_eq!(migrate::current_version(&c).unwrap(), 8);
        fingerprint(&c, ws)
    };

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, 9);
        assert_eq!(fingerprint(c, ws), before, "기존 표 · 자료판 · 정산 상태가 바뀌었다");
        assert_eq!(repo::settle::status(c, ws)?.state, "FRESH");
        for t in [
            "dream_key",
            "dream_student_identity",
            "dream_course_map",
            "dream_dept_priority",
            "dream_item_priority",
            "dream_job",
            "dream_job_row",
            "dream_job_event",
        ] {
            let n: i64 = c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?;
            assert_eq!(n, 0, "{t} 에 자료가 들어갔다");
        }
        let fk: i64 = c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        assert_eq!(fk, 0);
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        assert_eq!(ok, "ok");
        Ok(())
    })
    .unwrap();
    drop(db);

    // 승격 직전 백업이 하나 생겼다
    let backups: Vec<String> = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".db"))
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert!(backups[0].contains("v8"), "{backups:?}");

    // 다시 열어도 다시 돌지 않는다
    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, 9);
        assert_eq!(fingerprint(c, ws), before);
        Ok(())
    })
    .unwrap();
    let n = std::fs::read_dir(dir.join("backups")).unwrap().flatten().count();
    assert_eq!(n, 1, "두 번째로 열 때 승격 백업이 또 생겼다");
}

#[test]
fn 기존_자료판_트리거에_방과후드림_표가_이어지지_않았다() {
    let db = Db::open_memory().unwrap();
    db.read(|c| {
        let mut st = c.prepare("SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'trigger'")?;
        let rows: Vec<(String, String, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (name, table, sql) in rows {
            if table.starts_with("dream_") {
                // 방과후드림 표의 트리거는 일관성 검사뿐 — 자료판을 올리지 않는다
                assert!(name.starts_with("dream_"), "{name}");
                assert!(!sql.contains("data_version"), "{name} 이 자료판을 올린다");
            } else {
                assert!(!sql.contains("dream_"), "{name} 이 방과후드림 표를 건드린다");
            }
        }
        Ok(())
    })
    .unwrap();
}
