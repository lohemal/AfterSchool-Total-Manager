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
        assert_eq!(migrate::current_version(c)?, migrate::latest_version());
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
            "dream_workspace_setting",
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
    assert!(backups[0].contains("v8"), "{backups:?}"); // 008 → 최신(009 · 010)을 한 번에

    // 다시 열어도 다시 돌지 않는다
    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, migrate::latest_version());
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

// ─────────────────────────────────────────────── 011 (P6) — v0.1.11 DB(10)에서

/// v0.1.11 과 같은 10 단계 DB — 정산 + 방과후드림 자료(강좌 연결 · 부서별 항목 차례 ·
/// 결제 기준월 · 결과 파일까지 만든 작업)를 둔다. (작업공간, 부서)
fn v10_with_dream(path: &std::path::Path) -> (i64, i64) {
    let ws = v8_with_settlement(path);
    let c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    for sql in [
        include_str!("../../migrations/009_dream_payment.sql"),
        include_str!("../../migrations/010_dream_workspace_setting.sql"),
    ] {
        c.execute_batch(sql).unwrap();
    }
    c.pragma_update(None, "user_version", 10).unwrap();
    let dept: i64 = c.query_row("SELECT id FROM department WHERE workspace_id = ?1", [ws], |r| r.get(0)).unwrap();
    c.execute_batch(&format!(
        "INSERT INTO dream_course_map (workspace_id, department_id, course_code) VALUES ({ws}, {dept}, 'PGM0001');
         INSERT INTO dream_item_priority (department_id, item_code, sort_order) VALUES
           ({dept}, 'MATERIAL', 1), ({dept}, 'INSTRUCTOR', 2), ({dept}, 'OPERATION', 3), ({dept}, 'TEXTBOOK', 4);
         INSERT INTO dream_workspace_setting (workspace_id, billing_month) VALUES ({ws}, '202610');
         INSERT INTO dream_job (id, workspace_id, status, source_name, source_sha256, source_size, app_version)
           VALUES (1, {ws}, 'READY', '결제.xlsx', '{}', 100, '0.1.11');
         INSERT INTO dream_job_event (job_id, kind, detail) VALUES (1, 'RESULT_EXPORTED', 'date=20261030 sha={} rows=4 file=결과.xlsx');",
        "a".repeat(64),
        "b".repeat(64)
    ))
    .unwrap();
    (ws, dept)
}

fn dream_counts(c: &Connection) -> Vec<(String, i64)> {
    [
        "dream_course_map",
        "dream_item_priority",
        "dream_workspace_setting",
        "dream_job",
        "dream_job_event",
        "dream_applied_grade",
        "dream_item_order",
    ]
    .iter()
    .map(|t| {
        let n: i64 = c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap_or(-1);
        (t.to_string(), n)
    })
    .collect()
}

#[test]
fn 열면_011_이_적용되고_기존_방과후드림_자료와_정산은_그대로다() {
    let dir = tmp_dir("v10");
    let path = dir.join("old.db");
    let (ws, dept) = v10_with_dream(&path);
    let (before, dream_before) = {
        let c = Connection::open(&path).unwrap();
        assert_eq!(migrate::current_version(&c).unwrap(), 10);
        (fingerprint(&c, ws), dream_counts(&c))
    };
    assert_eq!(dream_before.iter().find(|x| x.0 == "dream_applied_grade").unwrap().1, -1, "011 전에는 표가 없다");

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, 11);
        assert_eq!(migrate::latest_version(), 11);
        assert_eq!(fingerprint(c, ws), before, "기존 표 · 자료판 · 정산 상태가 바뀌었다");
        assert_eq!(repo::settle::status(c, ws)?.state, "FRESH");
        let after = dream_counts(c);
        for (t, n) in &dream_before {
            let m = after.iter().find(|x| &x.0 == t).unwrap().1;
            if *n >= 0 {
                assert_eq!(m, *n, "{t} 자료가 바뀌었다");
            }
        }
        // 새 표는 비어 있다 — 적용 학년은 미설정, 공통 항목 차례도 옮겨 담지 않는다
        assert_eq!(after.iter().find(|x| x.0 == "dream_applied_grade").unwrap().1, 0);
        assert_eq!(after.iter().find(|x| x.0 == "dream_item_order").unwrap().1, 0);
        assert!(crate::repo::dream::workspace_setting::applied_grades(c, ws)?.is_empty());
        // 예전 부서별 차례가 하나뿐이니 그 차례를 그대로 쓴다(저장은 하지 않는다)
        let o = crate::repo::dream::priority::common_item_order(c, ws)?;
        assert_eq!((o.source, o.order[0].as_str()), ("LEGACY", "MATERIAL"));
        assert_eq!(crate::repo::dream::course::find(c, ws, "PGM0001")?, Some(dept));
        let ev: String = c.query_row("SELECT detail FROM dream_job_event WHERE kind = 'RESULT_EXPORTED'", [], |r| r.get(0))?;
        assert!(ev.starts_with("date=20261030"));
        let fk: i64 = c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        assert_eq!(fk, 0);
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        assert_eq!(ok, "ok");
        // 작업공간을 지우면 적용 학년도 함께 사라진다
        crate::repo::dream::workspace_setting::set_applied_grades(c, ws, &[3])?;
        Ok(())
    })
    .unwrap();
    db.write(|c| {
        c.execute("DELETE FROM workspace WHERE id = ?1", [ws])?;
        let n: i64 = c.query_row("SELECT COUNT(*) FROM dream_applied_grade", [], |r| r.get(0))?;
        assert_eq!(n, 0);
        Ok(())
    })
    .unwrap();
    drop(db);
    let backups: Vec<String> = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".db"))
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert!(backups[0].starts_with("before-migrate-v10-"), "{backups:?}");
}
