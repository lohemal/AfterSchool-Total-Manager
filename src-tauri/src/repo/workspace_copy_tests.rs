//! 작업공간 자료 가져오기 · 쪽 나누기 · 다중 정렬 시험 (v0.1.6).
//!
//! 가장 중요한 것 넷.
//!
//! 1. **학생·지원대상자를 복제하지 않는다** — 학년도 소속이라 이미 공유된다.
//! 2. **금액을 이어받지 않는다** — 새 부서의 기준금액에서 시작한다.
//! 3. **과거 작업공간이 바뀌지 않는다** — 그쪽 정산이 낡음이 되면 안 된다.
//! 4. **쪽 하나만 읽는다** — 전체를 읽어 화면에서 자르지 않는다.

use crate::db::Db;
use crate::model::{
    CostItem, DepartmentInput, EnrollmentFilter, EnrollmentInput, Fee, SortSpec, StudentInput,
    WorkspaceCopyInput, WorkspaceInput,
};
use crate::repo;

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

fn 기준금액() -> Vec<Fee> {
    vec![
        fee(강사료, 100_000),
        fee(수용비, 5_000),
        fee(교재비, 20_000),
        fee(재료비, 1_500),
    ]
}

fn sort(key: &str, dir: &str) -> SortSpec {
    SortSpec {
        key: key.to_string(),
        dir: dir.to_string(),
    }
}

struct F {
    db: Db,
    year: i64,
    items: Vec<CostItem>,
}

impl F {
    fn new() -> Self {
        let db = Db::open_memory().unwrap();
        let year = db
            .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        Self { db, year, items }
    }

    fn ws(&self, name: &str, from: &str, to: &str) -> i64 {
        self.db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    self.year,
                    &WorkspaceInput {
                        name: name.into(),
                        start_date: from.into(),
                        end_date: to.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn student(&self, grade: i64, class_no: &str, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no: class_no.into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn dept(&self, ws: i64, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class_name.into()),
                        teacher: Some("김강사".into()),
                        days: Some("월,수".into()),
                        note: None,
                        fees,
                        capacity: None,
                        allowed_grades: Vec::new(),
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, ws: i64, student: i64, dept: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    ws,
                    &EnrollmentInput {
                        student_id: student,
                        department_id: dept,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap()
    }

    /// 명령 계층이 하는 것과 똑같이 — 작업공간 만들기와 가져오기를 한 트랜잭션에서.
    fn ws_copy(
        &self,
        name: &str,
        from: &str,
        to: &str,
        copy: Option<WorkspaceCopyInput>,
    ) -> crate::error::AppResult<(i64, crate::model::WorkspaceCopyResult)> {
        self.db.write(|c| {
            let id = repo::year::create_workspace(
                c,
                self.year,
                &WorkspaceInput {
                    name: name.into(),
                    start_date: from.into(),
                    end_date: to.into(),
                    note: None,
                },
            )?;
            let r = match copy.as_ref() {
                None => crate::model::WorkspaceCopyResult {
                    workspace_id: id,
                    departments: 0,
                    enrollments: 0,
                    charges: 0,
                },
                Some(o) => repo::workspace_copy::run(c, id, o, &self.items)?,
            };
            Ok((id, r))
        })
    }

    fn 목록(&self, ws: i64) -> Vec<crate::model::Enrollment> {
        self.db
            .read(|c| repo::enrollment::list(c, ws, &self.items, &Default::default()))
            .unwrap()
    }

    fn 쪽(
        &self,
        ws: i64,
        f: &EnrollmentFilter,
        s: &[SortSpec],
        page: i64,
        size: i64,
    ) -> crate::model::EnrollmentPage {
        self.db
            .read(|c| repo::enrollment::list_page(c, ws, &self.items, f, s, page, size))
            .unwrap()
    }

    fn 한개(&self, sql: &str, ws: i64) -> i64 {
        self.db
            .read(|c| Ok(c.query_row(sql, rusqlite::params![ws], |r| r.get(0))?))
            .unwrap()
    }
}

/// 기준 작업공간: 부서 둘 · 수강중 둘 · 취소 하나 · 학생별 수정 하나.
fn 기준_작업공간(f: &F) -> (i64, i64, i64, i64, i64) {
    let ws = f.ws("1기", "2026-03-01", "2026-03-31");
    let 로봇 = f.dept(ws, "로봇과학", "B반", 기준금액());
    let 미술 = f.dept(ws, "미술", "A반", vec![fee(강사료, 40_000)]);

    let a = f.student(1, "가람", 1, "홍길동");
    let b = f.student(2, "나리", 3, "이두리");
    let c = f.student(3, "10", 2, "박세찬");

    let e1 = f.enroll(ws, a, 로봇);
    let e2 = f.enroll(ws, b, 미술);
    let e3 = f.enroll(ws, c, 로봇);

    // 학생별로 금액을 고쳐 둔다 — 이것이 새 기간으로 넘어가면 안 된다
    f.db.write(|c| {
        repo::enrollment::update_fees(
            c,
            e1,
            &[fee(강사료, 90_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 1_000)],
            "형제 할인",
            &f.items,
        )
    })
    .unwrap();

    // 한 명은 취소 — 가져오지 않아야 한다
    f.db.write(|c| {
        repo::enrollment::cancel(c, e3, Some(&[fee(강사료, 30_000)]), "중도 포기", &f.items)
    })
    .unwrap();

    // 차감 우선순위를 정해 둔다
    f.db.write(|c| repo::priority::dept_save(c, ws, &[미술, 로봇]))
        .unwrap();

    (ws, 로봇, 미술, e1, e2)
}

fn 가져오기(source: i64, dept: bool, enr: bool) -> WorkspaceCopyInput {
    WorkspaceCopyInput {
        source_workspace_id: source,
        departments: dept,
        enrollments: enr,
    }
}

// ─────────────────────────────────── 기본

#[test]
fn 가져오기_없이_빈_작업공간을_만든다() {
    let f = F::new();
    기준_작업공간(&f);
    let (새, r) = f.ws_copy("2기", "2026-04-01", "2026-04-30", None).unwrap();
    assert_eq!(r.departments, 0);
    assert_eq!(r.enrollments, 0);
    assert_eq!(f.한개("SELECT COUNT(*) FROM department WHERE workspace_id = ?1", 새), 0);
    assert_eq!(f.목록(새).len(), 0);
}

#[test]
fn 부서만_가져온다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let (새, r) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, false)))
        .unwrap();

    assert_eq!(r.departments, 2);
    assert_eq!(r.enrollments, 0, "수강은 가져오지 않는다");
    assert_eq!(f.목록(새).len(), 0);

    let 새부서 = f.db.read(|c| repo::department::list(c, 새, None)).unwrap();
    assert_eq!(새부서.len(), 2);
    let 로봇 = 새부서.iter().find(|d| d.name == "로봇과학").unwrap();
    assert_eq!(로봇.class_name, "B반");
    assert_eq!(로봇.teacher, "김강사");
    assert_eq!(로봇.days, "월,수");
    assert_eq!(로봇.total, 126_500, "기준금액이 그대로 온다");
}

#[test]
fn 수강생을_가져오려면_부서도_함께여야_한다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let err = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, false, true)))
        .unwrap_err();
    assert!(format!("{err:?}").contains("부서정보도 함께"), "{err:?}");
    // 작업공간 자체가 만들어지지 않는다
    let n = f.db.read(|c| repo::year::list_workspaces(c, f.year)).unwrap();
    assert_eq!(n.len(), 1, "되돌아가야 한다");
}

// ─────────────────────────────────── 학생 · 지원대상자는 복제하지 않는다

#[test]
fn 학생과_지원대상자는_복제하지_않는다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let 학생수 = f
        .db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM student", [], |r| r.get::<_, i64>(0))?))
        .unwrap();

    f.ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    let 뒤 = f
        .db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM student", [], |r| r.get::<_, i64>(0))?))
        .unwrap();
    assert_eq!(학생수, 뒤, "학생 행이 늘었다 — 복제하면 안 된다");
    assert_eq!(
        f.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM support_eligibility", [], |r| r
            .get::<_, i64>(0))?))
            .unwrap(),
        0
    );
}

#[test]
fn 새_작업공간에서도_같은_학년도_학생이_보인다() {
    let f = F::new();
    기준_작업공간(&f);
    let (새, _) = f.ws_copy("2기", "2026-04-01", "2026-04-30", None).unwrap();
    // 학생정보는 학년도 소속이라 작업공간과 무관하게 보인다
    let 학생 = f
        .db
        .read(|c| repo::student::list(c, f.year, &Default::default()))
        .unwrap();
    assert_eq!(학생.len(), 3);
    assert_eq!(f.목록(새).len(), 0, "다만 수강은 아직 없다");
}

// ─────────────────────────────────── 부서 · 매핑

#[test]
fn 새_부서는_새_행이고_옛_부서와_독립이다() {
    let f = F::new();
    let (옛, 옛로봇, _, _, _) = 기준_작업공간(&f);
    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    let 새로봇 = f
        .db
        .read(|c| repo::department::list(c, 새, None))
        .unwrap()
        .into_iter()
        .find(|d| d.name == "로봇과학")
        .unwrap();
    assert_ne!(새로봇.id, 옛로봇, "같은 행을 나눠 쓰면 안 된다");

    // 새 부서 금액을 고쳐도 옛 부서는 그대로
    f.db.write(|c| {
        repo::department::update(
            c,
            새로봇.id,
            &DepartmentInput {
                name: "로봇과학".into(),
                class_name: Some("B반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![fee(강사료, 200_000)],
                capacity: None,
                allowed_grades: Vec::new(),
            },
        )
    })
    .unwrap();
    let 옛금액 = f.db.read(|c| repo::department::fees_of(c, 옛로봇)).unwrap();
    assert_eq!(
        옛금액.iter().find(|x| x.item_code == 강사료).unwrap().amount,
        100_000,
        "과거 부서가 바뀌었다"
    );
}

#[test]
fn 수강이_새_부서를_가리킨다() {
    let f = F::new();
    let (옛, 옛로봇, 옛미술, _, _) = 기준_작업공간(&f);
    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    let 새부서: std::collections::HashMap<String, i64> = f
        .db
        .read(|c| repo::department::list(c, 새, None))
        .unwrap()
        .into_iter()
        .map(|d| (d.name.clone(), d.id))
        .collect();

    for e in f.목록(새) {
        assert_ne!(e.department_id, 옛로봇);
        assert_ne!(e.department_id, 옛미술);
        assert_eq!(
            e.department_id,
            새부서[&e.dept_name],
            "{} 가 새 부서를 가리키지 않는다",
            e.name
        );
    }
}

#[test]
fn 차감_우선순위도_새_부서_id_로_따라온다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, false)))
        .unwrap();

    // 화면 목록(dept_list)은 이용권 자격자가 수강 중인 부서만 보여 주므로
    // 여기서는 저장된 순서를 직접 본다.
    let 순서: Vec<String> = f
        .db
        .read(|c| {
            let mut st = c.prepare(
                "SELECT d.name || d.class_name FROM dept_priority p
                   JOIN department d ON d.id = p.department_id
                  WHERE p.workspace_id = ?1 ORDER BY p.sort_order",
            )?;
            let v = st
                .query_map(rusqlite::params![새], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(v)
        })
        .unwrap();
    // 기준에서 미술 → 로봇과학 순으로 정해 두었다
    assert_eq!(순서, vec!["미술A반".to_string(), "로봇과학B반".to_string()]);
    // 우선순위 줄이 가리키는 부서가 새 작업공간 것인지 DB 로 확인한다
    let 남의것 = f.한개(
        "SELECT COUNT(*) FROM dept_priority p JOIN department d ON d.id = p.department_id
           WHERE p.workspace_id = ?1 AND d.workspace_id <> ?1",
        새,
    );
    assert_eq!(남의것, 0, "옛 부서 id 가 남았다");
}

// ─────────────────────────────────── 수강 · 금액

#[test]
fn 수강중만_가져오고_취소는_빼놓는다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let (새, r) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    let 옛수강중 = f.한개(
        "SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1 AND status = 'ACTIVE'",
        옛,
    );
    assert_eq!(옛수강중, 2);
    assert_eq!(r.enrollments, 2, "수강중인 수만큼");
    assert_eq!(f.목록(새).len(), 2);
    assert_eq!(
        f.한개("SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1 AND status = 'CANCELLED'", 새),
        0,
        "취소는 가져오지 않는다"
    );
    // 학생도 정확히 옮겨졌는가
    let 이름: Vec<String> = f.목록(새).into_iter().map(|e| e.name).collect();
    assert!(이름.contains(&"홍길동".to_string()));
    assert!(이름.contains(&"이두리".to_string()));
    assert!(!이름.contains(&"박세찬".to_string()), "취소자가 들어왔다");
}

#[test]
fn 금액은_새_부서_기준금액에서_시작한다() {
    let f = F::new();
    let (옛, _, _, e1, _) = 기준_작업공간(&f);
    // 기준 작업공간의 홍길동은 학생별로 고쳐 116,000 이다
    let 옛홍길동 = f.db.read(|c| repo::enrollment::get(c, e1, &f.items)).unwrap();
    assert_eq!(옛홍길동.total, 116_000);
    assert!(옛홍길동.has_override);

    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();
    let 새홍길동 = f
        .목록(새)
        .into_iter()
        .find(|e| e.name == "홍길동")
        .unwrap();

    assert_eq!(새홍길동.total, 126_500, "새 부서 기준금액이어야 한다");
    assert!(!새홍길동.has_override, "override 가 따라왔다");
    let 금 = |code: &str| {
        새홍길동
            .fees
            .iter()
            .find(|x| x.item_code == code)
            .unwrap()
            .amount
    };
    assert_eq!(금(강사료), 100_000);
    assert_eq!(금(수용비), 5_000);
    assert_eq!(금(교재비), 20_000);
    assert_eq!(금(재료비), 1_500);

    // 과거 금액은 그대로 남아 있다
    assert_eq!(
        f.db.read(|c| repo::enrollment::get(c, e1, &f.items)).unwrap().total,
        116_000
    );
}

#[test]
fn 취소자의_최종_금액도_넘어오지_않는다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();
    // 취소자 자체가 없으므로 그 금액도 없다
    assert!(f.목록(새).iter().all(|e| e.name != "박세찬"));
    assert_eq!(
        f.한개(
            "SELECT COUNT(*) FROM charge c JOIN enrollment e ON e.id = c.enrollment_id
              WHERE e.workspace_id = ?1 AND c.is_overridden = 1",
            새
        ),
        0,
        "override 칸이 생겼다"
    );
}

// ─────────────────────────────────── 가져오지 않는 것들

#[test]
fn 추가징수_환불_정산은_넘어오지_않는다() {
    let f = F::new();
    let (옛, _, _, e1, _) = 기준_작업공간(&f);

    // 기준 작업공간에 추가징수와 정산을 만들어 둔다
    f.db.write(|c| {
        repo::adjustment::create_additional(
            c,
            e1,
            &crate::model::AdjustmentInput {
                occurred_on: Some("2026-03-10".into()),
                note: None,
            },
            &f.items,
        )
    })
    .unwrap();
    f.db.write(|c| repo::settle::generate(c, 옛)).unwrap();
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1", 옛), 1);
    assert_eq!(f.한개("SELECT COUNT(*) FROM settlement WHERE workspace_id = ?1", 옛), 1);
    let 옛정산 = f.db.read(|c| repo::settle::status(c, 옛)).unwrap().state;
    assert_eq!(옛정산, "FRESH");

    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    for (표, sql) in [
        ("추가징수", "SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1"),
        ("정산", "SELECT COUNT(*) FROM settlement WHERE workspace_id = ?1"),
        (
            "배분",
            "SELECT COUNT(*) FROM settlement_alloc a JOIN settlement s ON s.id = a.settlement_id
              WHERE s.workspace_id = ?1",
        ),
        (
            "지원금 스냅샷",
            "SELECT COUNT(*) FROM settlement_budget b JOIN settlement s ON s.id = b.settlement_id
              WHERE s.workspace_id = ?1",
        ),
    ] {
        assert_eq!(f.한개(sql, 새), 0, "{표} 가 넘어왔다");
    }

    // 과거 작업공간은 그대로 — 정산이 낡음이 되지 않는다
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1", 옛), 1);
    assert_eq!(
        f.db.read(|c| repo::settle::status(c, 옛)).unwrap().state,
        "FRESH",
        "과거 정산이 낡음이 되었다"
    );
}

#[test]
fn 과거_변경이력을_새_작업공간으로_복제하지_않는다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let 옛이력 = f.한개("SELECT COUNT(*) FROM change_log WHERE workspace_id = ?1", 옛);
    assert!(옛이력 >= 3);

    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(옛, true, true)))
        .unwrap();

    // 가져온 일은 **한 줄**로만 남는다
    let 새이력 = f.한개("SELECT COUNT(*) FROM change_log WHERE workspace_id = ?1", 새);
    assert_eq!(새이력, 1, "수강마다 한 줄씩 남기면 이력이 덮인다");
    let kind: String = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT kind FROM change_log WHERE workspace_id = ?1",
                rusqlite::params![새],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(kind, "WS_IMPORT");
    // 과거 이력도 그대로
    assert_eq!(f.한개("SELECT COUNT(*) FROM change_log WHERE workspace_id = ?1", 옛), 옛이력);
}

#[test]
fn 중간에_실패하면_작업공간까지_되돌아간다() {
    let f = F::new();
    let (옛, _, _, _, _) = 기준_작업공간(&f);
    let 전 = f
        .db
        .read(|c| repo::year::list_workspaces(c, f.year))
        .unwrap()
        .len();

    // 다른 학년도에서 가져오려 하면 막힌다 — 그때 작업공간도 남지 않아야 한다
    let 다른해 = f
        .db
        .write(|c| repo::year::create_year(c, 2027, "2027학년도"))
        .unwrap();
    let 남의방 = f
        .db
        .write(|c| {
            repo::year::create_workspace(
                c,
                다른해,
                &WorkspaceInput {
                    name: "남의 기수".into(),
                    start_date: "2027-03-01".into(),
                    end_date: "2027-03-31".into(),
                    note: None,
                },
            )
        })
        .unwrap();

    let err = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(남의방, true, true)))
        .unwrap_err();
    assert!(format!("{err:?}").contains("같은 학년도"), "{err:?}");
    assert_eq!(
        f.db.read(|c| repo::year::list_workspaces(c, f.year)).unwrap().len(),
        전,
        "작업공간이 남았다"
    );
    let _ = 옛;
}

#[test]
fn 미리보기가_무엇이_넘어오는지_알려_준다() {
    let f = F::new();
    let (옛, _, _, e1, _) = 기준_작업공간(&f);
    f.db.write(|c| {
        repo::adjustment::create_additional(
            c,
            e1,
            &crate::model::AdjustmentInput {
                occurred_on: Some("2026-03-10".into()),
                note: None,
            },
            &f.items,
        )
    })
    .unwrap();
    f.db.write(|c| repo::settle::generate(c, 옛)).unwrap();

    let p = f.db.read(|c| repo::workspace_copy::preview(c, 옛)).unwrap();
    assert_eq!(p.source_name, "1기");
    assert_eq!(p.departments, 2);
    assert_eq!(p.active_enrollments, 2);
    assert_eq!(p.cancelled_enrollments, 1);
    assert_eq!(p.adjustments, 1);
    assert_eq!(p.settlements, 1);
    // 형제 할인으로 고친 두 칸 + 취소하며 확정한 강사료 한 칸
    assert_eq!(p.overridden_cells, 3);
    assert!(p.has_priority);
}

// ─────────────────────────────────── 쪽 나누기

/// 한 부서에 학생 `n` 명을 넣는다.
fn 많은_수강(f: &F, n: i64) -> (i64, i64) {
    let ws = f.ws("대량", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    for i in 0..n {
        // 번호는 1~99 까지만 쓸 수 있으므로 학년·반을 돌려 가며 겹치지 않게 만든다
        let no = (i % 99) + 1;
        let class_no = ((i / 99) % 3) + 1;
        let grade = (i % 6) + 1;
        let s = f.student(grade, &format!("{class_no}"), no, &format!("학생{i:04}"));
        f.enroll(ws, s, d);
    }
    (ws, d)
}

#[test]
fn 쪽_하나만_읽는다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);

    let p = f.쪽(ws, &Default::default(), &[], 1, 100);
    assert_eq!(p.total, 250, "건수는 전체를 센다");
    assert_eq!(p.rows.len(), 100, "줄은 한 쪽만 읽는다");
    assert_eq!(p.page, 1);
    assert_eq!(p.page_count, 3);
    assert_eq!(p.page_size, 100);

    let p3 = f.쪽(ws, &Default::default(), &[], 3, 100);
    assert_eq!(p3.rows.len(), 50, "마지막 쪽은 남은 만큼");
}

#[test]
fn 쪽_크기_50_100_200() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);
    for (size, first, count) in [(50, 50, 5), (100, 100, 3), (200, 200, 2)] {
        let p = f.쪽(ws, &Default::default(), &[], 1, size);
        assert_eq!(p.rows.len(), first, "{size}개씩");
        assert_eq!(p.page_count, count);
        assert_eq!(p.total, 250);
    }
}

#[test]
fn 경계_0건_1건_정확히_100건_101건() {
    let f = F::new();
    let 빈 = f.ws("빈", "2026-06-01", "2026-06-30");
    let p = f.쪽(빈, &Default::default(), &[], 1, 100);
    assert_eq!(p.total, 0);
    assert_eq!(p.rows.len(), 0);
    assert_eq!(p.page_count, 1, "0건이어도 1쪽이다");
    assert_eq!(p.page, 1);

    for (n, 쪽수, 마지막) in [(1, 1, 1), (100, 1, 100), (101, 2, 1)] {
        let g = F::new();
        let (ws, _) = 많은_수강(&g, n);
        let p = g.쪽(ws, &Default::default(), &[], 1, 100);
        assert_eq!(p.total, n);
        assert_eq!(p.page_count, 쪽수, "{n}건");
        let last = g.쪽(ws, &Default::default(), &[], 쪽수, 100);
        assert_eq!(last.rows.len() as i64, 마지막, "{n}건의 마지막 쪽");
    }
}

#[test]
fn 없는_쪽을_달라고_하면_마지막_쪽을_준다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);
    let p = f.쪽(ws, &Default::default(), &[], 99, 100);
    assert_eq!(p.page, 3, "빈 화면 대신 마지막 쪽");
    assert_eq!(p.rows.len(), 50);

    let p0 = f.쪽(ws, &Default::default(), &[], 0, 100);
    assert_eq!(p0.page, 1);
}

#[test]
fn 쪽이_겹치거나_빠지지_않는다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);
    let mut 본것: Vec<i64> = Vec::new();
    for page in 1..=3 {
        본것.extend(f.쪽(ws, &Default::default(), &[], page, 100).rows.iter().map(|e| e.id));
    }
    assert_eq!(본것.len(), 250);
    let mut 정렬 = 본것.clone();
    정렬.sort_unstable();
    정렬.dedup();
    assert_eq!(정렬.len(), 250, "같은 학생이 두 쪽에 나왔다");
}

#[test]
fn 필터가_먼저_걸리고_그_안에서_쪽을_나눈다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);
    let 일학년 = EnrollmentFilter {
        grade: Some(1),
        ..Default::default()
    };
    let 전체 = f.db.read(|c| repo::enrollment::list(c, ws, &f.items, &일학년)).unwrap().len() as i64;
    let p = f.쪽(ws, &일학년, &[], 1, 100);
    assert_eq!(p.total, 전체, "필터에 걸린 건수여야 한다");
    assert!(p.total < 250);
    assert!(p.rows.iter().all(|e| e.grade == 1));
}

// ─────────────────────────────────── 다중 정렬

#[test]
fn 오름차순과_내림차순() {
    let f = F::new();
    let ws = f.ws("정렬", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    for (g, c, n, name) in [(3, "1", 1, "다"), (1, "2", 5, "가"), (2, "10", 3, "나")] {
        let s = f.student(g, c, n, name);
        f.enroll(ws, s, d);
    }

    let asc = f.쪽(ws, &Default::default(), &[sort("grade", "ASC")], 1, 100);
    assert_eq!(asc.rows.iter().map(|e| e.grade).collect::<Vec<_>>(), vec![1, 2, 3]);

    let desc = f.쪽(ws, &Default::default(), &[sort("grade", "DESC")], 1, 100);
    assert_eq!(desc.rows.iter().map(|e| e.grade).collect::<Vec<_>>(), vec![3, 2, 1]);
}

#[test]
fn 반은_자연정렬을_그대로_쓴다() {
    let f = F::new();
    let ws = f.ws("반", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    for (i, c) in ["10", "2", "1", "가람", "01"].iter().enumerate() {
        let s = f.student(1, c, i as i64 + 1, "아무개");
        f.enroll(ws, s, d);
    }
    let p = f.쪽(ws, &Default::default(), &[sort("classNo", "ASC")], 1, 100);
    let 반: Vec<&str> = p.rows.iter().map(|e| e.class_no.as_str()).collect();
    // 숫자 반이 자연정렬로 먼저, 그 뒤 문자 반. `01` 과 `1` 은 서로 다른 값이다.
    assert_eq!(반, vec!["01", "1", "2", "10", "가람"]);
    assert_eq!(반.iter().filter(|c| **c == "1").count(), 1);
    assert_eq!(반.iter().filter(|c| **c == "01").count(), 1);
}

#[test]
fn 여러_조건이_우선순위대로_걸린다() {
    let f = F::new();
    let ws = f.ws("다중", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    // 같은 학년 안에서 반, 같은 반 안에서 번호
    for (g, c, n) in [(1, "2", 1), (1, "1", 9), (1, "1", 2), (2, "1", 1)] {
        let s = f.student(g, c, n, "아무개");
        f.enroll(ws, s, d);
    }
    let p = f.쪽(
        ws,
        &Default::default(),
        &[sort("grade", "ASC"), sort("classNo", "ASC"), sort("studentNo", "DESC")],
        1,
        100,
    );
    let 본 : Vec<(i64, &str, i64)> = p
        .rows
        .iter()
        .map(|e| (e.grade, e.class_no.as_str(), e.student_no))
        .collect();
    assert_eq!(본, vec![(1, "1", 9), (1, "1", 2), (1, "2", 1), (2, "1", 1)]);
}

#[test]
fn 다섯_개를_넘으면_앞의_다섯만_쓴다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 20);
    let 여섯 = vec![
        sort("grade", "ASC"),
        sort("classNo", "ASC"),
        sort("studentNo", "ASC"),
        sort("name", "ASC"),
        sort("dept", "ASC"),
        sort("status", "DESC"),
    ];
    // 여섯 번째를 무시해도 결과가 나와야 한다 (화면이 애초에 막지만 서버도 견딘다)
    let p = f.쪽(ws, &Default::default(), &여섯, 1, 100);
    assert_eq!(p.total, 20);
}

#[test]
fn 알_수_없는_정렬_이름은_무시한다() {
    let f = F::new();
    let (ws, _) = 많은_수강(&f, 10);
    // SQL 을 넣으려는 시도가 그대로 들어가면 안 된다
    let 수상한 = vec![sort("s.name; DROP TABLE student--", "ASC"), sort("grade", "ASC")];
    let p = f.쪽(ws, &Default::default(), &수상한, 1, 100);
    assert_eq!(p.total, 10, "질의가 깨지지 않는다");
    assert_eq!(
        f.한개("SELECT COUNT(*) FROM student WHERE 1 = ?1", 1),
        10,
        "학생 표가 살아 있다"
    );
    // 알아들은 조건(학년)만 걸린다
    let g: Vec<i64> = p.rows.iter().map(|e| e.grade).collect();
    let mut 정렬 = g.clone();
    정렬.sort_unstable();
    assert_eq!(g, 정렬);
}

#[test]
fn 정렬이_없으면_원래_차례를_쓴다() {
    let f = F::new();
    let ws = f.ws("기본", "2026-05-01", "2026-05-31");
    let 미술 = f.dept(ws, "미술", "A반", vec![fee(강사료, 10_000)]);
    let 로봇 = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    let a = f.student(1, "1", 1, "가");
    f.enroll(ws, a, 미술);
    f.enroll(ws, a, 로봇);

    let p = f.쪽(ws, &Default::default(), &[], 1, 100);
    // 수강생 명단의 기존 기본 차례는 부서 우선이다
    assert_eq!(
        p.rows.iter().map(|e| e.dept_name.as_str()).collect::<Vec<_>>(),
        vec!["로봇과학", "미술"]
    );
}

#[test]
fn 값이_같아도_차례가_흔들리지_않는다() {
    let f = F::new();
    let ws = f.ws("동률", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    for i in 1..=30 {
        let s = f.student(1, "1", i, "같은이름");
        f.enroll(ws, s, d);
    }
    // 모두 같은 학년·반·이름 — 오직 tie-breaker 가 차례를 정한다
    let 첫 = f.쪽(ws, &Default::default(), &[sort("name", "ASC")], 1, 100);
    for _ in 0..5 {
        let 또 = f.쪽(ws, &Default::default(), &[sort("name", "ASC")], 1, 100);
        assert_eq!(
            첫.rows.iter().map(|e| e.id).collect::<Vec<_>>(),
            또.rows.iter().map(|e| e.id).collect::<Vec<_>>(),
            "실행마다 차례가 달라진다"
        );
    }
}

#[test]
fn 금액으로도_정렬한다() {
    let f = F::new();
    let ws = f.ws("금액", "2026-05-01", "2026-05-31");
    let 싼곳 = f.dept(ws, "미술", "A반", vec![fee(강사료, 10_000)]);
    let 비싼곳 = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 90_000)]);
    let a = f.student(1, "1", 1, "가");
    let b = f.student(1, "1", 2, "나");
    f.enroll(ws, a, 싼곳);
    f.enroll(ws, b, 비싼곳);

    let p = f.쪽(ws, &Default::default(), &[sort("total", "DESC")], 1, 100);
    assert_eq!(p.rows[0].total, 90_000);
    assert_eq!(p.rows[1].total, 10_000);

    let i = f.쪽(ws, &Default::default(), &[sort(강사료, "ASC")], 1, 100);
    assert_eq!(i.rows[0].total, 10_000, "항목 하나로도 정렬된다");
}

#[test]
fn 필터와_정렬을_함께_쓴다() {
    let f = F::new();
    let ws = f.ws("함께", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    for (g, n) in [(1, 3), (1, 1), (2, 2)] {
        let s = f.student(g, "1", n, "아무개");
        f.enroll(ws, s, d);
    }
    let p = f.쪽(
        ws,
        &EnrollmentFilter {
            grade: Some(1),
            ..Default::default()
        },
        &[sort("studentNo", "DESC")],
        1,
        100,
    );
    assert_eq!(p.total, 2);
    assert_eq!(p.rows.iter().map(|e| e.student_no).collect::<Vec<_>>(), vec![3, 1]);
}

#[test]
fn 지원유형_필터도_쪽_건수에_맞게_걸린다() {
    let f = F::new();
    let ws = f.ws("자격", "2026-05-01", "2026-05-31");
    let d = f.dept(ws, "로봇과학", "A반", vec![fee(강사료, 10_000)]);
    let 있는이 = f.student(1, "1", 1, "가");
    let 없는이 = f.student(1, "1", 2, "나");
    f.enroll(ws, 있는이, d);
    f.enroll(ws, 없는이, d);
    f.db.write(|c| {
        repo::eligibility::create(
            c,
            f.year,
            &crate::model::EligibilityInput {
                student_id: 있는이,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();

    let 이용권 = EnrollmentFilter {
        program: Some("VOUCHER".into()),
        ..Default::default()
    };
    let p = f.쪽(ws, &이용권, &[], 1, 100);
    assert_eq!(p.total, 1, "건수가 거른 뒤 값이어야 한다");
    assert_eq!(p.rows.len(), 1);
    assert_eq!(p.rows[0].name, "가");

    let 없음 = f.쪽(
        ws,
        &EnrollmentFilter {
            program: Some("NONE".into()),
            ..Default::default()
        },
        &[],
        1,
        100,
    );
    assert_eq!(없음.total, 1);
    assert_eq!(없음.rows[0].name, "나");

    // 예전처럼 Rust 에서 거른 결과와도 같아야 한다
    assert_eq!(
        f.db.read(|c| repo::enrollment::list(c, ws, &f.items, &이용권)).unwrap().len(),
        1
    );
}

// ─────────────────────────────────── Excel 은 쪽과 무관하다

#[test]
fn Excel은_현재_쪽이_아니라_필터_전체를_낸다() {
    use crate::excel::read;

    let f = F::new();
    let (ws, _) = 많은_수강(&f, 250);
    let dir = std::env::temp_dir().join("afterschool-page-excel");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // 화면은 3쪽을 보고 있어도
    let 쪽 = f.쪽(ws, &Default::default(), &[], 3, 100);
    assert_eq!(쪽.rows.len(), 50);

    // Excel 은 필터에 걸린 250건을 모두 낸다
    let made = f
        .db
        .read(|c| {
            crate::excel::export_enrollments(c, ws, &f.items, &Default::default(), &["2026학년도"], &dir)
        })
        .unwrap();
    let sheet = read::read_first_sheet(&std::path::PathBuf::from(&made.path)).unwrap();
    assert_eq!(sheet.rows.len(), 250, "현재 쪽만 나왔다");
    assert_eq!(made.rows, 250);
}

/// 실제 신고(v0.1.6 후보)를 그대로 옮긴 재현 시험.
///
/// 학생 셋이 **서로 다른 방향으로** 금액을 고쳐 두었을 때, 새 작업공간의 금액이
/// 셋 다 새 부서의 기준금액이어야 한다. 올린 것·내린 것·0원으로 만든 것을 모두
/// 넣은 까닭은, 한 방향만 시험하면 "우연히 기준금액과 같았다"를 통과로 볼 수
/// 있기 때문이다.
///
/// 저장된 행(`charge`)만 보지 않고 **화면이 읽는 길**(`list_page`)로도 확인한다.
/// 신고가 'DB 는 맞는데 화면이 옛 금액을 보여 준다' 일 수도 있어서다.
#[test]
fn 학생별_수정금액_셋은_어느_것도_새_작업공간으로_넘어가지_않는다() {
    let f = F::new();
    let 기준 = f.ws("1기", "2026-03-01", "2026-03-31");
    let 로봇 = f.dept(기준, "로봇과학", "B반", 기준금액());

    let a = f.student(1, "1", 1, "학생가");
    let b = f.student(1, "1", 2, "학생나");
    let c = f.student(1, "1", 3, "학생다");
    let ea = f.enroll(기준, a, 로봇);
    let eb = f.enroll(기준, b, 로봇);
    let ec = f.enroll(기준, c, 로봇);

    // 가: 올림 · 나: 내림 · 다: 0원
    let 고침 = |e: i64, 강사: i64| {
        f.db.write(|c| {
            repo::enrollment::update_fees(
                c,
                e,
                &[
                    fee(강사료, 강사),
                    fee(수용비, 5_000),
                    fee(교재비, 20_000),
                    fee(재료비, 1_500),
                ],
                "시험",
                &f.items,
            )
        })
        .unwrap();
    };
    고침(ea, 500_000);
    고침(eb, 30_000);
    고침(ec, 0);

    // 새 기간에는 기준금액 자체가 달라졌다 — 넘어온 금액과 헷갈리지 않는 값으로.
    f.db.write(|c| {
        repo::department::update(
            c,
            로봇,
            &DepartmentInput {
                name: "로봇과학".into(),
                class_name: Some("B반".into()),
                teacher: Some("김강사".into()),
                days: Some("월,수".into()),
                note: None,
                fees: vec![
                    fee(강사료, 77_000),
                    fee(수용비, 5_000),
                    fee(교재비, 20_000),
                    fee(재료비, 1_500),
                ],
                capacity: None,
                allowed_grades: Vec::new(),
            },
        )
    })
    .unwrap();

    let (새, r) = f
        .ws_copy(
            "2기",
            "2026-04-01",
            "2026-04-30",
            Some(가져오기(기준, true, true)),
        )
        .unwrap();
    assert_eq!(r.enrollments, 3);

    // ── 저장된 행
    let 넘어온_수정 = f.한개(
        "SELECT COUNT(*) FROM charge ch JOIN enrollment e ON e.id = ch.enrollment_id
          WHERE e.workspace_id = ?1 AND ch.is_overridden = 1",
        새,
    );
    assert_eq!(넘어온_수정, 0, "override 표시가 넘어왔다");

    let 기준과_다른_칸 = f.한개(
        "SELECT COUNT(*) FROM charge ch
           JOIN enrollment e ON e.id = ch.enrollment_id
           LEFT JOIN department_fee df
                  ON df.department_id = e.department_id AND df.item_code = ch.item_code
          WHERE e.workspace_id = ?1 AND ch.amount <> COALESCE(df.amount, 0)",
        새,
    );
    assert_eq!(기준과_다른_칸, 0, "새 부서 기준금액과 다른 금액이 있다");

    // ── 화면이 읽는 길
    let page = f.쪽(새, &EnrollmentFilter::default(), &[], 1, 100);
    assert_eq!(page.total, 3);
    for row in &page.rows {
        let 강사 = row
            .fees
            .iter()
            .find(|x| x.item_code == 강사료)
            .map(|x| x.amount)
            .unwrap_or(-1);
        assert_eq!(강사, 77_000, "{} 의 강사료가 새 기준금액이 아니다", row.name);
        assert!(!row.has_override, "{} 에 수정 표시가 남았다", row.name);
    }

    // ── 기준 작업공간은 그대로다
    let 옛것 = f.쪽(기준, &EnrollmentFilter::default(), &[], 1, 100);
    let mut 옛_강사: Vec<i64> = 옛것
        .rows
        .iter()
        .map(|r| {
            r.fees
                .iter()
                .find(|x| x.item_code == 강사료)
                .map(|x| x.amount)
                .unwrap_or(-1)
        })
        .collect();
    옛_강사.sort();
    assert_eq!(옛_강사, vec![0, 30_000, 500_000], "기준 작업공간이 바뀌었다");
}

/// 정원과 수강 가능 학년도 새 작업공간으로 함께 온다 (v0.1.7).
///
/// 학년은 **새 부서 id 로 다시 만들어야** 한다. 옛 id 를 가리키는 줄이 하나라도
/// 남으면 기준 작업공간의 부서를 지웠을 때 새 작업공간의 대상 학년이 함께
/// 사라진다.
#[test]
fn 정원과_수강_가능_학년도_가져온다() {
    let f = F::new();
    let 기준 = f.ws("1기", "2026-03-01", "2026-03-31");
    let 로봇 = f.db
        .write(|c| {
            repo::department::create(
                c,
                기준,
                &DepartmentInput {
                    name: "로봇과학".into(),
                    class_name: Some("A반".into()),
                    teacher: Some("김강사".into()),
                    days: Some("월,수".into()),
                    note: None,
                    fees: 기준금액(),
                    capacity: Some(20),
                    allowed_grades: vec![1, 2, 4],
                },
            )
        })
        .unwrap();
    // 정원을 정하지 않은 반도 그대로 미설정으로 와야 한다
    f.dept(기준, "미술", "A반", vec![fee(강사료, 40_000)]);

    let a = f.student(1, "1", 1, "홍길동");
    f.enroll(기준, a, 로봇);

    let (새, out) = f
        .ws_copy(
            "2기",
            "2026-04-01",
            "2026-04-30",
            Some(가져오기(기준, true, true)),
        )
        .unwrap();
    assert_eq!(out.departments, 2);

    let 새목록 = f
        .db
        .read(|c| repo::department::list(c, 새, None))
        .unwrap();
    let 새로봇 = 새목록
        .iter()
        .find(|d| d.name == "로봇과학")
        .expect("로봇과학이 오지 않았다");
    assert_eq!(새로봇.capacity, Some(20));
    assert_eq!(
        새로봇.allowed_grades,
        vec![1, 2, 4],
        "1·2·4 가 그대로 오지 않았다"
    );

    let 새미술 = 새목록.iter().find(|d| d.name == "미술").unwrap();
    assert_eq!(새미술.capacity, None, "미설정이 0 으로 바뀌었다");
    assert!(새미술.allowed_grades.is_empty());

    // 옛 부서 id 를 가리키는 학년 줄이 새 작업공간에 없어야 한다
    let 옛참조 = f.한개(
        "SELECT COUNT(*) FROM department_allowed_grade g
           JOIN department d ON d.id = g.department_id
          WHERE d.workspace_id = ?1 AND g.department_id = (
                SELECT id FROM department WHERE workspace_id = 1 AND name = '로봇과학')",
        새,
    );
    assert_eq!(옛참조, 0);
    let 새학년줄 = f.한개(
        "SELECT COUNT(*) FROM department_allowed_grade g
           JOIN department d ON d.id = g.department_id
          WHERE d.workspace_id = ?1",
        새,
    );
    assert_eq!(새학년줄, 3, "학년 줄이 3개가 아니다");

    // 기준 작업공간은 그대로다
    let 기준목록 = f
        .db
        .read(|c| repo::department::list(c, 기준, None))
        .unwrap();
    let 기준로봇 = 기준목록.iter().find(|d| d.name == "로봇과학").unwrap();
    assert_eq!(기준로봇.capacity, Some(20));
    assert_eq!(기준로봇.allowed_grades, vec![1, 2, 4]);

    // 가져온 수강의 금액은 v0.1.6 규칙 그대로 — 새 부서 기준금액에서 시작
    let 넘어온_수정 = f.한개(
        "SELECT COUNT(*) FROM charge ch JOIN enrollment e ON e.id = ch.enrollment_id
          WHERE e.workspace_id = ?1 AND ch.is_overridden = 1",
        새,
    );
    assert_eq!(넘어온_수정, 0);
}

/// 기준 부서를 지워도 새 작업공간의 대상 학년은 남는다 — 참조가 끊겨 있다는 증거.
#[test]
fn 기준_부서를_지워도_새_작업공간의_학년은_남는다() {
    let f = F::new();
    let 기준 = f.ws("1기", "2026-03-01", "2026-03-31");
    let 로봇 = f.db
        .write(|c| {
            repo::department::create(
                c,
                기준,
                &DepartmentInput {
                    name: "로봇과학".into(),
                    class_name: Some("A반".into()),
                    teacher: None,
                    days: Some("월".into()),
                    note: None,
                    fees: 기준금액(),
                    capacity: Some(20),
                    allowed_grades: vec![3, 5],
                },
            )
        })
        .unwrap();

    let (새, _) = f
        .ws_copy("2기", "2026-04-01", "2026-04-30", Some(가져오기(기준, true, false)))
        .unwrap();

    f.db.write(|c| repo::department::delete_many(c, &[로봇]))
        .unwrap();

    let 새목록 = f.db.read(|c| repo::department::list(c, 새, None)).unwrap();
    assert_eq!(새목록.len(), 1);
    assert_eq!(새목록[0].allowed_grades, vec![3, 5]);

    // 외래키가 깨지지 않았다
    let 위반 = f.한개(
        "SELECT COUNT(*) FROM pragma_foreign_key_check WHERE ?1 = ?1",
        새,
    );
    assert_eq!(위반, 0);
}
