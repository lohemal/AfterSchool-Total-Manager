//! Phase 1 통합 점검 — 실제 스키마를 올린 메모리 DB로 업무 흐름을 그대로 밟는다.
//!
//! 화면 없이도 "작업공간 → 학생 → 지원대상자 → 부서 → Excel"이 도는지 확인한다.

use crate::db::Db;
use crate::model::{DepartmentInput, EligibilityInput, Fee, StudentFilter, StudentInput, WorkspaceInput};
use crate::repo;

fn ws(name: &str, start: &str, end: &str) -> WorkspaceInput {
    WorkspaceInput {
        name: name.to_string(),
        start_date: start.to_string(),
        end_date: end.to_string(),
        note: None,
    }
}

fn student(grade: i64, class_no: i64, no: i64, name: &str) -> StudentInput {
    StudentInput {
        grade,
        class_no,
        student_no: no,
        name: name.to_string(),
        note: None,
    }
}

#[test]
fn 학년도를_만들면_지원정책_두_줄이_함께_생긴다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();

    db.read(|c| {
        let policies = repo::policy::list(c, year)?;
        assert_eq!(policies.len(), 2);
        // 한도·대상학년은 비워 둔다 — 시도마다 다르므로 프로그램이 정하지 않는다.
        assert_eq!(policies[0].annual_limit, 0);
        assert_eq!(policies[0].target_grades, "");
        assert!(policies[0].periods.is_empty());

        let y = repo::year::get_year(c, year)?;
        assert!(y.is_current);
        Ok(())
    })
    .unwrap();
}

#[test]
fn 작업공간은_언제나_시작일_순서로_보인다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();

    // 일부러 4월 → 3월 → 5월 차례로 만든다. 만든 순서와 무관해야 한다.
    for (n, s, e) in [
        ("4월", "2026-04-01", "2026-04-30"),
        ("3월", "2026-03-01", "2026-03-31"),
        ("5월", "2026-05-01", "2026-05-31"),
    ] {
        db.write(|c| repo::year::create_workspace(c, year, &ws(n, s, e))).unwrap();
    }

    let names: Vec<String> = db
        .read(|c| repo::year::list_workspaces(c, year))
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(names, vec!["3월", "4월", "5월"]);

    // 날짜를 고치면 순서도 그대로 따라간다 — 따로 맞출 곳이 없다
    let list = db.read(|c| repo::year::list_workspaces(c, year)).unwrap();
    let march = list.iter().find(|w| w.name == "3월").unwrap();
    db.write(|c| repo::year::update_workspace(c, march.id, &ws("3월(보강)", "2026-06-01", "2026-06-30")))
        .unwrap();
    let names: Vec<String> = db
        .read(|c| repo::year::list_workspaces(c, year))
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(names, vec!["4월", "5월", "3월(보강)"]);
}

#[test]
fn 선행_작업공간은_목록과_같은_순서로_정해진다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    for (n, s, e) in [
        ("3월", "2026-03-01", "2026-03-31"),
        ("4월", "2026-04-01", "2026-04-30"),
        ("여름특강", "2026-07-20", "2026-08-20"),
    ] {
        db.write(|c| repo::year::create_workspace(c, year, &ws(n, s, e))).unwrap();
    }

    let list = db.read(|c| repo::year::list_workspaces(c, year)).unwrap();
    let april = list.iter().find(|w| w.name == "4월").unwrap();
    let before: Vec<String> = db
        .read(|c| repo::year::workspaces_before(c, april))
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(before, vec!["3월"], "4월보다 앞선 것은 3월뿐");

    let summer = list.iter().find(|w| w.name == "여름특강").unwrap();
    let before: Vec<String> = db
        .read(|c| repo::year::workspaces_before(c, summer))
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(before, vec!["3월", "4월"]);

    // 첫 작업공간 앞에는 아무것도 없다
    let march = list.iter().find(|w| w.name == "3월").unwrap();
    assert!(db.read(|c| repo::year::workspaces_before(c, march)).unwrap().is_empty());
}

#[test]
fn 시작일이_같으면_종료일과_id로_순서가_갈린다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    db.write(|c| repo::year::create_workspace(c, year, &ws("긴쪽", "2026-03-01", "2026-05-31")))
        .unwrap();
    db.write(|c| repo::year::create_workspace(c, year, &ws("짧은쪽", "2026-03-01", "2026-03-31")))
        .unwrap();

    let names: Vec<String> = db
        .read(|c| repo::year::list_workspaces(c, year))
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    assert_eq!(names, vec!["짧은쪽", "긴쪽"], "종료일이 이른 쪽이 앞");
}

#[test]
fn 기간이_겹치면_알려_주되_막지는_않는다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let first = db
        .write(|c| repo::year::create_workspace(c, year, &ws("1학기", "2026-03-01", "2026-08-31")))
        .unwrap();

    // 여름특강은 1학기와 겹친다 — 알려 주기만 한다
    let hits = db
        .read(|c| repo::year::overlapping_workspaces(c, year, None, "2026-07-20", "2026-08-20"))
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "1학기");

    // 그래도 저장은 된다
    db.write(|c| repo::year::create_workspace(c, year, &ws("여름특강", "2026-07-20", "2026-08-20")))
        .unwrap();
    assert_eq!(db.read(|c| repo::year::list_workspaces(c, year)).unwrap().len(), 2);

    // 겹치지 않으면 아무것도 나오지 않는다
    let hits = db
        .read(|c| repo::year::overlapping_workspaces(c, year, None, "2026-09-01", "2026-09-30"))
        .unwrap();
    assert!(hits.is_empty());

    // 자기 자신은 겹침으로 세지 않는다 (수정할 때)
    let hits = db
        .read(|c| {
            repo::year::overlapping_workspaces(c, year, Some(first), "2026-03-01", "2026-06-30")
        })
        .unwrap();
    assert!(hits.iter().all(|w| w.id != first));

    // 하루만 스쳐도 겹침이다
    let hits = db
        .read(|c| repo::year::overlapping_workspaces(c, year, None, "2026-08-31", "2026-09-30"))
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn 학생_업로드는_기존_학생을_지우지_않고_갱신한다() {
    use repo::student::StudentRow;
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();

    let rows = vec![
        StudentRow { grade: 3, class_no: 1, student_no: 1, name: "김하나".into(), note: String::new() },
        StudentRow { grade: 3, class_no: 1, student_no: 2, name: "이두리".into(), note: String::new() },
    ];
    let (added, updated) = db.write(|c| repo::student::upsert_bulk(c, year, &rows)).unwrap();
    assert_eq!((added, updated), (2, 0));

    // 같은 학년·반·번호는 이름만 갱신된다 (개명·오타 수정)
    let rows = vec![StudentRow {
        grade: 3,
        class_no: 1,
        student_no: 2,
        name: "이두리(수정)".into(),
        note: String::new(),
    }];
    let (added, updated) = db.write(|c| repo::student::upsert_bulk(c, year, &rows)).unwrap();
    assert_eq!((added, updated), (0, 1));
    assert_eq!(db.read(|c| repo::student::count(c, year)).unwrap(), 2);
}

#[test]
fn 지원유형은_저장하지_않고_자격에서_만들어진다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let s1 = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "김하나"))).unwrap();
    db.write(|c| repo::student::create(c, year, &student(3, 1, 2, "이두리"))).unwrap();

    db.write(|c| {
        repo::eligibility::create(
            c,
            year,
            &EligibilityInput {
                student_id: s1,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();
    db.write(|c| {
        repo::eligibility::create(
            c,
            year,
            &EligibilityInput {
                student_id: s1,
                program: "FREE_VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();

    let list = db
        .read(|c| repo::student::list(c, year, &StudentFilter::default()))
        .unwrap();
    let hana = list.iter().find(|s| s.id == s1).unwrap();
    assert_eq!(hana.programs, vec!["FREE_VOUCHER", "VOUCHER"]);
    assert!(list.iter().find(|s| s.name == "이두리").unwrap().programs.is_empty());

    // 자격을 지우면 지원유형도 곧바로 사라진다 — 따로 갱신할 곳이 없다
    let rows = db.read(|c| repo::eligibility::list(c, year, "VOUCHER", None)).unwrap();
    db.write(|c| repo::eligibility::delete_many(c, &[rows[0].id])).unwrap();
    let list = db
        .read(|c| repo::student::list(c, year, &StudentFilter::default()))
        .unwrap();
    assert_eq!(list.iter().find(|s| s.id == s1).unwrap().programs, vec!["FREE_VOUCHER"]);
}

#[test]
fn 자격은_작업공간_기간과_겹칠_때만_유효하다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let april = db
        .write(|c| repo::year::create_workspace(c, year, &ws("4월", "2026-04-01", "2026-04-30")))
        .unwrap();
    let may = db
        .write(|c| repo::year::create_workspace(c, year, &ws("5월", "2026-05-01", "2026-05-31")))
        .unwrap();
    let sid = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "김하나"))).unwrap();

    // 4월 15일에 지원이 끊긴 학생
    db.write(|c| {
        repo::eligibility::create(
            c,
            year,
            &EligibilityInput {
                student_id: sid,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: Some("2026-04-15".into()),
                note: None,
            },
        )
    })
    .unwrap();

    let in_april = db
        .read(|c| {
            repo::student::list(
                c,
                year,
                &StudentFilter { workspace_id: Some(april), ..Default::default() },
            )
        })
        .unwrap();
    assert_eq!(in_april[0].programs, vec!["VOUCHER"]);

    let in_may = db
        .read(|c| {
            repo::student::list(
                c,
                year,
                &StudentFilter { workspace_id: Some(may), ..Default::default() },
            )
        })
        .unwrap();
    assert!(in_may[0].programs.is_empty());
}

#[test]
fn 같은_학생의_자격기간이_겹치면_막는다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let sid = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "김하나"))).unwrap();

    let mk = |from: Option<&str>, to: Option<&str>| EligibilityInput {
        student_id: sid,
        program: "VOUCHER".into(),
        valid_from: from.map(|s| s.to_string()),
        valid_to: to.map(|s| s.to_string()),
        note: None,
    };

    db.write(|c| repo::eligibility::create(c, year, &mk(None, Some("2026-08-31")))).unwrap();
    // 겹치는 기간은 막는다
    assert!(db
        .write(|c| repo::eligibility::create(c, year, &mk(Some("2026-08-01"), None)))
        .is_err());
    // 겹치지 않으면 된다 (지원 중지 후 재선정)
    db.write(|c| repo::eligibility::create(c, year, &mk(Some("2026-09-01"), None))).unwrap();
}

#[test]
fn 대상학년이_아닌_대상자는_지우지_않고_표시만_한다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let s3 = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "삼학년"))).unwrap();
    let s4 = db.write(|c| repo::student::create(c, year, &student(4, 1, 1, "사학년"))).unwrap();

    for sid in [s3, s4] {
        db.write(|c| {
            repo::eligibility::create(
                c,
                year,
                &EligibilityInput {
                    student_id: sid,
                    program: "VOUCHER".into(),
                    valid_from: None,
                    valid_to: None,
                    note: None,
                },
            )
        })
        .unwrap();
    }

    // 대상학년을 3학년으로 정한다
    db.write(|c| repo::policy::save(c, year, "VOUCHER", 500_000, false, "3", &[]))
        .unwrap();

    let rows = db.read(|c| repo::eligibility::list(c, year, "VOUCHER", None)).unwrap();
    assert_eq!(rows.len(), 2, "정책을 바꿨다고 명단을 지우지 않는다");
    assert_eq!(rows.iter().filter(|r| r.grade_mismatch).count(), 1);
    assert_eq!(db.read(|c| repo::eligibility::mismatch_count(c, year, "VOUCHER")).unwrap(), 1);
}

#[test]
fn 부서_수강료는_항목별_행으로_저장된다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let wsid = db
        .write(|c| repo::year::create_workspace(c, year, &ws("4월", "2026-04-01", "2026-04-30")))
        .unwrap();

    let input = DepartmentInput {
        name: "로봇과학".into(),
        class_name: Some("A반".into()),
        teacher: Some("김강사".into()),
        days: Some("월,수".into()),
        note: None,
        fees: vec![
            Fee { item_code: "INSTRUCTOR".into(), amount: 40_000 },
            Fee { item_code: "TEXTBOOK".into(), amount: 30_000 },
        ],
    };
    db.write(|c| repo::department::create(c, wsid, &input)).unwrap();

    let list = db.read(|c| repo::department::list(c, wsid, None)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].total, 70_000);
    // 합계는 저장하지 않고 늘 더해서 낸다
    assert_eq!(list[0].fees.iter().map(|f| f.amount).sum::<i64>(), 70_000);

    // 같은 부서명·반명은 막는다
    assert!(db.write(|c| repo::department::create(c, wsid, &input)).is_err());
}

#[test]
fn 자료_버전은_트리거로_올라간다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let wsid = db
        .write(|c| repo::year::create_workspace(c, year, &ws("4월", "2026-04-01", "2026-04-30")))
        .unwrap();
    let sid = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "김하나"))).unwrap();

    let ws_v0 = db.read(|c| repo::year::get_workspace(c, wsid)).unwrap().data_version;
    let yr_v0 = db.read(|c| repo::year::get_year(c, year)).unwrap().data_version;

    // 부서를 만들면 그 작업공간의 버전만 오른다
    db.write(|c| {
        repo::department::create(
            c,
            wsid,
            &DepartmentInput {
                name: "미술".into(),
                class_name: None,
                teacher: None,
                days: None,
                note: None,
                fees: vec![Fee { item_code: "INSTRUCTOR".into(), amount: 25_000 }],
            },
        )
    })
    .unwrap();
    let ws_v1 = db.read(|c| repo::year::get_workspace(c, wsid)).unwrap().data_version;
    assert!(ws_v1 > ws_v0);

    // 이름을 고치는 것은 금액에 영향이 없으므로 학년도 버전이 오르지 않는다
    db.write(|c| repo::student::update(c, sid, &student(3, 1, 1, "김하나2"))).unwrap();
    assert_eq!(db.read(|c| repo::year::get_year(c, year)).unwrap().data_version, yr_v0);

    // 학년이 바뀌면 대상학년 판정이 달라지므로 오른다
    db.write(|c| repo::student::update(c, sid, &student(4, 1, 1, "김하나2"))).unwrap();
    assert!(db.read(|c| repo::year::get_year(c, year)).unwrap().data_version > yr_v0);
}

#[test]
fn 지원기간_저장은_겹침을_막는다() {
    use crate::model::Period;
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();

    let p = |name: &str, s: &str, e: &str, limit: i64| Period {
        id: None,
        name: name.to_string(),
        start_date: s.to_string(),
        end_date: e.to_string(),
        limit_amount: limit,
        seq: 0,
    };

    // 1학기 250,000 / 2학기 250,000, 연간 500,000, 이월 허용
    db.write(|c| {
        repo::policy::save(
            c,
            year,
            "VOUCHER",
            500_000,
            true,
            "3",
            &[
                p("1학기", "2026-03-01", "2026-08-31", 250_000),
                p("2학기", "2026-09-01", "2027-02-28", 250_000),
            ],
        )
    })
    .unwrap();

    let pol = db.read(|c| repo::policy::get(c, year, "VOUCHER")).unwrap();
    assert_eq!(pol.periods.len(), 2);
    assert_eq!(pol.periods[0].seq, 1);
    assert!(pol.carryover);
    assert_eq!(repo::policy::limit_notice(&pol), None);

    // 겹치는 기간은 막는다
    assert!(db
        .write(|c| {
            repo::policy::save(
                c,
                year,
                "VOUCHER",
                500_000,
                true,
                "3",
                &[
                    p("1학기", "2026-03-01", "2026-09-30", 250_000),
                    p("2학기", "2026-09-01", "2027-02-28", 250_000),
                ],
            )
        })
        .is_err());
}

#[test]
fn 학생을_지우면_자격도_함께_사라진다() {
    let db = Db::open_memory().unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let sid = db.write(|c| repo::student::create(c, year, &student(3, 1, 1, "김하나"))).unwrap();
    db.write(|c| {
        repo::eligibility::create(
            c,
            year,
            &EligibilityInput {
                student_id: sid,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();

    db.write(|c| repo::student::delete_many(c, &[sid])).unwrap();
    assert!(db
        .read(|c| repo::eligibility::list(c, year, "VOUCHER", None))
        .unwrap()
        .is_empty());
}
