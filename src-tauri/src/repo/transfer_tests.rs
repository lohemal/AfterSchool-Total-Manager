//! 학생 전출 시험 (v0.1.8).
//!
//! 지키려는 것 넷.
//!
//! 1. **상태를 저장하지 않는다** — 전출일과 오늘로 그때그때 판정한다.
//! 2. **전출이 금액을 건드리지 않는다** — 수강료 확정은 취소가 한다.
//! 3. **수강이 남은 채로 전출시키지 않는다** — 자동 취소도 하지 않는다.
//! 4. **삭제는 전출이 아니다** — 자료가 딸린 학생은 지워지지 않는다.

use crate::db::Db;
use crate::domain::school_date::{fmt, today};
use crate::model::{CostItem, DepartmentInput, EnrollmentInput, Fee, StudentInput, WorkspaceInput};
use crate::repo;

use chrono::Duration;

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

/// 오늘에서 며칠 떨어진 날짜. 시험이 달력에 묶이지 않게 한다.
fn 날짜(offset: i64) -> String {
    fmt(today() + Duration::days(offset))
}

struct F {
    db: Db,
    year: i64,
    ws: i64,
    items: Vec<CostItem>,
}

impl F {
    fn new() -> Self {
        let db = Db::open_memory().unwrap();
        let year = db
            .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
            .unwrap();
        let items = db.read(repo::cost_items).unwrap();
        let ws = db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    year,
                    &WorkspaceInput {
                        name: "1기".into(),
                        // 기간을 오늘 둘레로 잡아 전출일 시나리오와 겹치게 한다
                        start_date: 날짜(-30),
                        end_date: 날짜(30),
                        note: None,
                    },
                )
            })
            .unwrap();
        Self {
            db,
            year,
            ws,
            items,
        }
    }

    fn student(&self, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade: 4,
                        class_no: "2".into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn dept(&self, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some("A반".into()),
                        teacher: Some("김강사".into()),
                        days: Some("화".into()),
                        note: None,
                        fees: vec![fee("INSTRUCTOR", 30_000)],
                        capacity: None,
                        allowed_grades: Vec::new(),
                        custom_sessions: 0,
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, s: i64, d: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
                    &EnrollmentInput {
                        student_id: s,
                        department_id: d,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap()
    }

    fn 상태(&self, id: i64) -> String {
        self.db
            .read(|c| repo::student::list(c, self.year, &Default::default()))
            .unwrap()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
            .enrolment
    }

    fn 한개(&self, sql: &str, p: i64) -> i64 {
        self.db
            .read(|c| Ok(c.query_row(sql, rusqlite::params![p], |r| r.get(0))?))
            .unwrap()
    }
}

// ─────────────────────────────────────────────── 상태 판정

#[test]
fn 전출일이_없으면_재학이다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    assert_eq!(f.상태(a), "ATTENDING");
}

#[test]
fn 미래_전출일은_전출_예정이다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(8))))
        .unwrap();
    assert_eq!(f.상태(a), "LEAVING");
}

#[test]
fn 오늘이나_지난_전출일은_전출이다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(0))))
        .unwrap();
    assert_eq!(f.상태(a), "LEFT", "전출일 당일이 전출이 아니다");

    let b = f.student(2, "이두리");
    f.db.write(|c| repo::student::transfer_set(c, b, Some(&날짜(-5))))
        .unwrap();
    assert_eq!(f.상태(b), "LEFT");
}

// ─────────────────────────────────────────────── 수강이 남은 채 전출

#[test]
fn 수강이_남았으면_전출로_넘기지_못하고_자동_취소도_하지_않는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    let e = f.enroll(a, d);

    let err = f
        .db
        .write(|c| repo::student::transfer_set(c, a, Some(&날짜(0))))
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("수강 취소"), "안내가 다르다: {msg}");
    assert!(msg.contains("로봇과학A반"), "어느 반인지 알려 주지 않는다: {msg}");

    // 자동으로 아무것도 고치지 않았다
    assert_eq!(f.상태(a), "ATTENDING", "막혔는데 상태가 바뀌었다");
    let status: String = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT status FROM enrollment WHERE id = ?1",
                rusqlite::params![e],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(status, "ACTIVE", "수강이 자동으로 취소되었다");
}

#[test]
fn 전출_예정은_수강이_있어도_등록된다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    f.enroll(a, d);

    // 아직 떠나지 않았으니 그날까지는 정상 수강이다
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(10))))
        .unwrap();
    assert_eq!(f.상태(a), "LEAVING");
}

#[test]
fn 취소하고_나면_전출할_수_있고_금액은_그대로_남는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    let e = f.enroll(a, d);

    f.db.write(|c| {
        repo::enrollment::cancel(c, e, Some(&[fee("INSTRUCTOR", 12_000)]), "전출", &f.items)
    })
    .unwrap();
    let 금액_전: i64 = f.한개(
        "SELECT COALESCE(SUM(amount),0) FROM charge WHERE enrollment_id = ?1",
        e,
    );
    assert_eq!(금액_전, 12_000, "취소 금액이 확정되지 않았다");

    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(0))))
        .unwrap();
    assert_eq!(f.상태(a), "LEFT");

    // 전출해도 지난 기록이 그대로다
    assert_eq!(f.한개("SELECT COUNT(*) FROM enrollment WHERE student_id = ?1", a), 1);
    assert_eq!(
        f.한개(
            "SELECT COALESCE(SUM(amount),0) FROM charge WHERE enrollment_id = ?1",
            e
        ),
        12_000,
        "전출이 금액을 바꿨다"
    );
    let status: String = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT status FROM enrollment WHERE id = ?1",
                rusqlite::params![e],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(status, "CANCELLED");
}

// ─────────────────────────────────────────────── 신규 수강 대상

#[test]
fn 전출을_마친_학생은_새_수강에_넣을_수_없다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(-1))))
        .unwrap();

    let err = f
        .db
        .write(|c| {
            repo::enrollment::create(
                c,
                f.ws,
                &EnrollmentInput {
                    student_id: a,
                    department_id: d,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )
        })
        .unwrap_err();
    assert!(format!("{err:?}").contains("전출"), "{err:?}");
    assert_eq!(f.한개("SELECT COUNT(*) FROM enrollment WHERE student_id = ?1", a), 0);
}

#[test]
fn 전출_예정_학생은_전출일_전까지_새_수강에_넣을_수_있다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(10))))
        .unwrap();
    f.enroll(a, d); // 막히지 않아야 한다
    assert_eq!(f.한개("SELECT COUNT(*) FROM enrollment WHERE student_id = ?1", a), 1);
}

// ─────────────────────────────────────────────── 해제 · 이력

#[test]
fn 전출_정보를_지워도_수강과_금액은_그대로다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    let e = f.enroll(a, d);
    let 금액 = f.한개(
        "SELECT COALESCE(SUM(amount),0) FROM charge WHERE enrollment_id = ?1",
        e,
    );

    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(10))))
        .unwrap();
    f.db.write(|c| repo::student::transfer_set(c, a, None))
        .unwrap();

    assert_eq!(f.상태(a), "ATTENDING");
    assert_eq!(f.한개("SELECT COUNT(*) FROM enrollment WHERE student_id = ?1", a), 1);
    assert_eq!(
        f.한개(
            "SELECT COALESCE(SUM(amount),0) FROM charge WHERE enrollment_id = ?1",
            e
        ),
        금액
    );
}

#[test]
fn 전출_지정_변경_해제가_서로_다른_줄로_남는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(10))))
        .unwrap();
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(12))))
        .unwrap();
    f.db.write(|c| repo::student::transfer_set(c, a, None))
        .unwrap();

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, None, Some(a), None, 100))
        .unwrap();
    let kinds: Vec<&str> = logs.iter().map(|l| l.kind.as_str()).collect();
    assert!(kinds.contains(&"STUDENT_TRANSFER_SET"), "{kinds:?}");
    assert!(kinds.contains(&"STUDENT_TRANSFER_EDIT"), "{kinds:?}");
    assert!(kinds.contains(&"STUDENT_TRANSFER_CLEAR"), "{kinds:?}");

    let clear = logs
        .iter()
        .find(|l| l.kind == "STUDENT_TRANSFER_CLEAR")
        .unwrap();
    assert_eq!(clear.after_value, "재학");
}

#[test]
fn 같은_전출일을_다시_저장해도_이력이_쌓이지_않는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = 날짜(10);
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&d)))
        .unwrap();
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&d)))
        .unwrap();
    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, None, Some(a), None, 100))
        .unwrap();
    assert_eq!(logs.len(), 1, "바뀐 것이 없는데 이력이 쌓였다");
}

// ─────────────────────────────────────────────── 삭제 정책

#[test]
fn 이력이_없는_오입력_학생은_지워진다() {
    let f = F::new();
    let a = f.student(1, "오타");
    f.db.write(|c| repo::student::delete_many(c, &[a])).unwrap();
    assert_eq!(f.한개("SELECT COUNT(*) FROM student WHERE id = ?1", a), 0);
}

#[test]
fn 수강_기록이_있는_학생은_지워지지_않는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    let e = f.enroll(a, d);
    // 취소해도 기록은 남으므로 여전히 막힌다
    f.db.write(|c| repo::enrollment::cancel(c, e, None, "전출", &f.items))
        .unwrap();

    let err = f
        .db
        .write(|c| repo::student::delete_many(c, &[a]))
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("수강 기록"), "{msg}");
    assert!(msg.contains("전출 처리"), "전출을 안내하지 않는다: {msg}");

    // 막혔으니 수강과 금액이 그대로다
    assert_eq!(f.한개("SELECT COUNT(*) FROM student WHERE id = ?1", a), 1);
    assert_eq!(f.한개("SELECT COUNT(*) FROM enrollment WHERE student_id = ?1", a), 1);
}

#[test]
fn 전출_처리는_정산_자료판을_올리지_않는다() {
    let f = F::new();
    let a = f.student(1, "홍길동");
    let d = f.dept("로봇과학");
    f.enroll(a, d);
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    let before = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(before.state, "FRESH");

    let 학년도판 = |f: &F| f.한개("SELECT data_version FROM academic_year WHERE id = ?1", f.year);
    let v0 = 학년도판(&f);

    // 수강이 있는 학생이라도 전출 **예정**은 등록된다
    f.db.write(|c| repo::student::transfer_set(c, a, Some(&날짜(10))))
        .unwrap();

    assert_eq!(학년도판(&f), v0, "전출일을 넣었는데 학년도 자료판이 올랐다");
    let after = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(after.state, "FRESH", "전출 때문에 정산이 낡음이 되었다");
}
