//! 맞춤형 강사료 시험 (v0.1.8).
//!
//! 달력에 묶어 둔 시험이다 — **2026년 10월**을 쓴다. 10월 1일은 목요일이고
//! 화요일은 6·13·20·27 넷, 월요일은 5·12·19·26 넷, 수요일은 7·14·21·28 넷이다.
//! 날짜를 손으로 적어 두어야 사람이 결과를 검산할 수 있다.

use crate::db::Db;
use crate::model::{
    CostItem, CustomFeeReport, DepartmentInput, Fee, SessionKind, WorkspaceInput,
};
use crate::repo;

const 시작: &str = "2026-10-01";
const 끝: &str = "2026-10-31";

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

struct F {
    db: Db,
    year: i64,
    ws: i64,
    #[allow(dead_code)]
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
                        name: "2026년 10월".into(),
                        start_date: 시작.into(),
                        end_date: 끝.into(),
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

    fn dept(&self, name: &str, days: &str, sessions: i64) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some("A".into()),
                        teacher: Some("홍길동".into()),
                        days: Some(days.into()),
                        note: None,
                        fees: vec![fee("INSTRUCTOR", 30_000)],
                        capacity: None,
                        allowed_grades: Vec::new(),
                        custom_sessions: sessions,
                    },
                )
            })
            .unwrap()
    }

    fn rep(&self) -> CustomFeeReport {
        self.db
            .read(|c| repo::custom_fee::report(c, self.ws, false))
            .unwrap()
    }

    fn row(&self, name: &str) -> crate::model::CustomFeeRow {
        self.rep()
            .rows
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} 을 찾지 못했다"))
    }

    fn 조정(&self, dept: i64, date: &str, sessions: i64, why: &str) {
        self.db
            .write(|c| repo::custom_fee::session_set(c, dept, date, sessions, why))
            .unwrap();
    }

    fn 휴업일(&self, date: &str, why: &str) {
        self.db
            .write(|c| repo::custom_fee::closure_add(c, self.ws, date, why))
            .unwrap();
    }
}

// ─────────────────────────────────────────────── 기본 계산

#[test]
fn 주_1회_2차시_넉달치가_320000원이다() {
    let f = F::new();
    f.dept("미술", "화", 2);
    let r = f.row("미술");

    // 10월의 화요일은 6·13·20·27 — '4주'라고 어림하지 않고 달력을 훑은 결과다
    assert_eq!(r.planned_days, 4);
    assert_eq!(
        r.sessions_list.iter().map(|s| s.date.clone()).collect::<Vec<_>>(),
        vec!["2026-10-06", "2026-10-13", "2026-10-20", "2026-10-27"]
    );
    assert_eq!(r.counted_days, 4);
    assert_eq!(r.total_sessions, 8);
    assert_eq!(r.rate, 40_000, "기본 단가가 40,000원이 아니다");
    assert_eq!(r.fee, 320_000);
}

#[test]
fn 주_1회_1차시면_160000원이다() {
    let f = F::new();
    f.dept("바둑", "화", 1);
    let r = f.row("바둑");
    assert_eq!(r.total_sessions, 4);
    assert_eq!(r.fee, 160_000);
}

#[test]
fn 맞춤형이_아닌_부서는_목록에_없다() {
    let f = F::new();
    f.dept("미술", "화", 2);
    f.dept("일반반", "화", 0);
    let rep = f.rep();
    assert_eq!(rep.rows.len(), 1);
    assert_eq!(rep.rows[0].name, "미술");

    // 보고 싶으면 따로 켠다 — 그래도 합계에는 끼지 않는다
    let all = f
        .db
        .read(|c| repo::custom_fee::report(c, f.ws, true))
        .unwrap();
    assert_eq!(all.rows.len(), 2);
    assert_eq!(all.summary.classes, 1, "맞춤형 아닌 반이 합계에 들어갔다");
    assert_eq!(all.summary.total_fee, 320_000);
}

// ─────────────────────────────────────────────── 휴강 · 보강 · 차시 조정

#[test]
fn 한_번_휴강하면_240000원이다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 0, "공휴일");

    let r = f.row("미술");
    assert_eq!(r.planned_days, 4);
    assert_eq!(r.closed_days, 1);
    assert_eq!(r.counted_days, 3);
    assert_eq!(r.total_sessions, 6);
    assert_eq!(r.fee, 240_000);

    // 왜 빠졌는지 남아 있다 — 줄을 지우지 않고 0차시로 둔다
    let 그날 = r.sessions_list.iter().find(|s| s.date == "2026-10-20").unwrap();
    assert_eq!(그날.kind, SessionKind::Closed);
    assert_eq!(그날.sessions, 0);
    assert_eq!(그날.reason, "공휴일");
}

#[test]
fn 휴강한_만큼_보강하면_다시_320000원이다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 0, "공휴일");
    // 10-16 은 금요일 — 운영 요일이 아니어도 보강이 된다
    f.조정(d, "2026-10-16", 2, "10/20 보강");

    let r = f.row("미술");
    assert_eq!(r.extra_days, 1);
    assert_eq!(r.counted_days, 4);
    assert_eq!(r.total_sessions, 8);
    assert_eq!(r.fee, 320_000);

    let 보강 = r.sessions_list.iter().find(|s| s.date == "2026-10-16").unwrap();
    assert_eq!(보강.kind, SessionKind::Extra);
    assert_eq!(보강.weekday, "금");

    // 날짜 차례는 이른 것부터 — 보강이 끝에 붙지 않는다
    let dates: Vec<String> = r.sessions_list.iter().map(|s| s.date.clone()).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted);
}

#[test]
fn 날짜별_차시를_고치면_그만큼만_더해진다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 1, "단축수업");

    let r = f.row("미술");
    assert_eq!(r.total_sessions, 2 + 2 + 1 + 2, "2·2·1·2 = 7차시여야 한다");
    assert_eq!(r.total_sessions, 7);
    assert_eq!(r.fee, 280_000);
    assert_eq!(r.closed_days, 0, "1차시인데 휴강으로 셌다");
    assert_eq!(r.counted_days, 4);
}

#[test]
fn 조정을_지우면_기본_차시로_돌아간다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 0, "공휴일");
    assert_eq!(f.row("미술").total_sessions, 6);

    f.db.write(|c| repo::custom_fee::session_clear(c, d, "2026-10-20"))
        .unwrap();
    assert_eq!(f.row("미술").total_sessions, 8, "기본으로 돌아오지 않았다");
}

// ─────────────────────────────────────────────── 복수 요일

#[test]
fn 복수_요일은_모든_운영일을_센다() {
    let f = F::new();
    let d = f.dept("로봇", "월,수", 1);
    let r = f.row("로봇");

    // 10월 월요일 5·12·19·26 + 수요일 7·14·21·28 = 8일
    assert_eq!(r.planned_days, 8);
    assert_eq!(r.total_sessions, 8);
    assert_eq!(r.fee, 320_000);

    // 수요일 하나만 쉬면 그 날짜만 빠진다
    f.조정(d, "2026-10-14", 0, "강사 사정");
    let r2 = f.row("로봇");
    assert_eq!(r2.planned_days, 8);
    assert_eq!(r2.closed_days, 1);
    assert_eq!(r2.total_sessions, 7);
    assert!(
        r2.sessions_list
            .iter()
            .any(|s| s.date == "2026-10-21" && s.sessions == 1),
        "다른 수요일까지 쉬었다"
    );
}

// ─────────────────────────────────────────────── 공통 휴업일

#[test]
fn 공통_휴업일은_그날_모든_반에서_빠지고_지우면_돌아온다() {
    let f = F::new();
    f.dept("미술", "화", 2);
    f.dept("로봇", "화", 1);

    f.휴업일("2026-10-20", "재량휴업일");
    assert_eq!(f.row("미술").total_sessions, 6);
    assert_eq!(f.row("로봇").total_sessions, 3);
    let 그날 = f
        .row("미술")
        .sessions_list
        .into_iter()
        .find(|s| s.date == "2026-10-20")
        .unwrap();
    assert!(그날.from_closure, "공통 휴업일 표시가 없다");
    assert_eq!(그날.reason, "재량휴업일");

    // 지우면 둘 다 기본 예정일로 돌아온다
    let id = f.rep().closures[0].id;
    f.db.write(|c| repo::custom_fee::closure_delete(c, id))
        .unwrap();
    assert_eq!(f.row("미술").total_sessions, 8);
    assert_eq!(f.row("로봇").total_sessions, 4);
}

#[test]
fn 부서_조정이_공통_휴업일을_덮는다() {
    let f = F::new();
    let 미술 = f.dept("미술", "화", 2);
    f.dept("로봇", "화", 2);
    f.휴업일("2026-10-20", "재량휴업일");

    // 학교는 쉬었지만 이 반만 수업했다 — 사람이 적어 넣은 값이 앞선다
    f.조정(미술, "2026-10-20", 2, "정상 운영");

    assert_eq!(f.row("미술").total_sessions, 8, "부서 조정이 무시되었다");
    assert_eq!(f.row("로봇").total_sessions, 6, "다른 반까지 살아났다");
}

#[test]
fn 부서별_휴강은_그_반에만_걸린다() {
    let f = F::new();
    let 미술 = f.dept("미술", "화", 2);
    f.dept("로봇", "화", 2);
    f.조정(미술, "2026-10-20", 0, "강사 사정");

    assert_eq!(f.row("미술").total_sessions, 6);
    assert_eq!(f.row("로봇").total_sessions, 8, "다른 반이 함께 쉬었다");
}

#[test]
fn 기간_밖_휴업일은_넣을_수_없다() {
    let f = F::new();
    let err = f
        .db
        .write(|c| repo::custom_fee::closure_add(c, f.ws, "2026-11-05", "범위 밖"))
        .unwrap_err();
    assert!(format!("{err:?}").contains("작업공간 기간"), "{err:?}");
}

// ─────────────────────────────────────────────── 기본 차시 변경

#[test]
fn 기본_차시를_바꾸면_조정하지_않은_날만_따라_바뀐다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-27", 3, "보충");
    assert_eq!(f.row("미술").total_sessions, 2 + 2 + 2 + 3);

    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "미술".into(),
                class_name: Some("A".into()),
                teacher: Some("홍길동".into()),
                days: Some("화".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)],
                capacity: None,
                allowed_grades: Vec::new(),
                custom_sessions: 1,
            },
        )
    })
    .unwrap();

    let r = f.row("미술");
    assert_eq!(r.base_sessions, 1);
    // 조정하지 않은 셋은 1차시로, 적어 둔 27일은 3차시 그대로
    assert_eq!(r.total_sessions, 1 + 1 + 1 + 3);
    let 그날 = r.sessions_list.iter().find(|s| s.date == "2026-10-27").unwrap();
    assert_eq!(그날.sessions, 3, "손으로 적어 둔 차시가 덮였다");
}

// ─────────────────────────────────────────────── 고아 기록

#[test]
fn 요일을_바꿔도_적어_둔_기록을_지우지_않고_알린다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 0, "공휴일"); // 화요일

    // 화 → 목 으로 바꾼다. 10-20 은 더 이상 예정일이 아니다.
    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "미술".into(),
                class_name: Some("A".into()),
                teacher: Some("홍길동".into()),
                days: Some("목".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)],
                capacity: None,
                allowed_grades: Vec::new(),
                custom_sessions: 2,
            },
        )
    })
    .unwrap();

    let rep = f.rep();
    // 기록은 DB 에 그대로 있다
    let 남음: i64 = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM dept_session WHERE department_id = ?1",
                rusqlite::params![d],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(남음, 1, "조용히 지워졌다");

    // 그리고 검토하라고 알려 준다
    assert_eq!(rep.orphans.len(), 1, "{:?}", rep.orphans);
    assert_eq!(rep.orphans[0].date, "2026-10-20");
    assert!(rep.orphans[0].why.contains("운영 요일이 아닌"), "{:?}", rep.orphans[0]);

    // 목요일 1·8·15·22·29 다섯 번이 새 예정일이다
    let r = f.row("미술");
    assert_eq!(r.planned_days, 5);
    assert_eq!(r.total_sessions, 10);
}

// ─────────────────────────────────────────────── 단가

#[test]
fn 단가는_학년도마다_두고_고치면_강사료가_따라_바뀐다() {
    let f = F::new();
    f.dept("미술", "화", 2);
    assert_eq!(f.row("미술").fee, 320_000);

    f.db.write(|c| repo::custom_fee::rate_set(c, f.year, 45_000))
        .unwrap();
    assert_eq!(f.row("미술").rate, 45_000);
    assert_eq!(f.row("미술").fee, 360_000);

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, None, None, Some("YEAR_RATE_EDIT"), 10))
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].before_value.contains("40,000"));
    assert!(logs[0].after_value.contains("45,000"));
}

// ─────────────────────────────────────────────── 정산과 분리

#[test]
fn 맞춤형_변경은_학생_수강료_정산을_낡음으로_만들지_않는다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    let s = f
        .db
        .write(|c| {
            repo::student::create(
                c,
                f.year,
                &crate::model::StudentInput {
                    grade: 3,
                    class_no: "1".into(),
                    student_no: 1,
                    name: "홍길동".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    f.db.write(|c| {
        repo::enrollment::create(
            c,
            f.ws,
            &crate::model::EnrollmentInput {
                student_id: s,
                department_id: d,
                fees: Vec::new(),
                reason: None,
            },
            &f.items,
        )
    })
    .unwrap();

    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    assert_eq!(
        f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state,
        "FRESH"
    );

    let 자료판 = |f: &F| -> (i64, i64) {
        f.db.read(|c| {
            Ok((
                c.query_row(
                    "SELECT data_version FROM workspace WHERE id = ?1",
                    rusqlite::params![f.ws],
                    |r| r.get(0),
                )?,
                c.query_row(
                    "SELECT data_version FROM academic_year WHERE id = ?1",
                    rusqlite::params![f.year],
                    |r| r.get(0),
                )?,
            ))
        })
        .unwrap()
    };
    let v0 = 자료판(&f);

    // 맞춤형 쪽을 모두 건드려 본다
    f.휴업일("2026-10-13", "재량휴업일");
    f.조정(d, "2026-10-20", 0, "강사 사정");
    f.조정(d, "2026-10-16", 2, "보강");
    f.db.write(|c| repo::custom_fee::rate_set(c, f.year, 45_000))
        .unwrap();
    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "미술".into(),
                class_name: Some("A".into()),
                teacher: Some("홍길동".into()),
                days: Some("화".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)], // 금액은 그대로
                capacity: None,
                allowed_grades: Vec::new(),
                custom_sessions: 3, // 기본 차시만 바꾼다
            },
        )
    })
    .unwrap();

    assert_eq!(자료판(&f), v0, "맞춤형 변경이 자료판을 올렸다");
    assert_eq!(
        f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state,
        "FRESH",
        "맞춤형 변경 때문에 수강료 정산이 낡음이 되었다"
    );

    // 반대로 기준금액을 고치면 예전처럼 낡음이 된다
    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "미술".into(),
                class_name: Some("A".into()),
                teacher: Some("홍길동".into()),
                days: Some("화".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 35_000)],
                capacity: None,
                allowed_grades: Vec::new(),
                custom_sessions: 3,
            },
        )
    })
    .unwrap();
    assert_ne!(
        f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state,
        "FRESH",
        "기준금액을 고쳤는데 낡음이 되지 않았다"
    );
}

// ─────────────────────────────────────────────── 이력

#[test]
fn 맞춤형_변경이_이력에_남는다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.휴업일("2026-10-13", "한글날");
    f.조정(d, "2026-10-20", 0, "강사 사정");
    f.db.write(|c| repo::custom_fee::session_clear(c, d, "2026-10-20"))
        .unwrap();

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, None, 100))
        .unwrap();
    let kinds: Vec<&str> = logs.iter().map(|l| l.kind.as_str()).collect();
    assert!(kinds.contains(&"WS_CLOSURE_ADD"), "{kinds:?}");
    assert!(kinds.contains(&"DEPT_SESSION_EDIT"), "{kinds:?}");
    assert!(kinds.contains(&"DEPT_SESSION_CLEAR"), "{kinds:?}");
}

/// 보고서에 적을 **업무 예시**를 그대로 시험으로 고정한다.
///
/// ```text
/// 미술A · 2026-10-01 ~ 2026-10-31 · 화요일 · 기본 2차시
///   10/06 화  정상  2
///   10/13 화  정상  2
///   10/20 화  휴강  0   (공휴일)
///   10/23 금  보강  1   (10/20 보강)
///   10/27 화  정상  2
///   ─────────────────────────
///   인정 수업일 4회 · 총 인정 차시 7 · 40,000 × 7 = 280,000원
/// ```
#[test]
fn 보고용_업무_예시를_사람이_검산할_수_있다() {
    let f = F::new();
    let d = f.dept("미술", "화", 2);
    f.조정(d, "2026-10-20", 0, "공휴일");
    f.조정(d, "2026-10-23", 1, "10/20 보강");

    let r = f.row("미술");
    let 적힌대로: Vec<(String, String, i64)> = r
        .sessions_list
        .iter()
        .map(|s| (s.date.clone(), s.weekday.clone(), s.sessions))
        .collect();
    assert_eq!(
        적힌대로,
        vec![
            ("2026-10-06".to_string(), "화".to_string(), 2),
            ("2026-10-13".to_string(), "화".to_string(), 2),
            ("2026-10-20".to_string(), "화".to_string(), 0),
            ("2026-10-23".to_string(), "금".to_string(), 1),
            ("2026-10-27".to_string(), "화".to_string(), 2),
        ]
    );
    assert_eq!(r.planned_days, 4);
    assert_eq!(r.closed_days, 1);
    assert_eq!(r.extra_days, 1);
    assert_eq!(r.counted_days, 4);
    assert_eq!(r.total_sessions, 7);
    assert_eq!(r.rate, 40_000);
    assert_eq!(r.fee, 280_000);
}
