//! 정원 · 수강 가능 학년 · 수강현황 시험 (v0.1.7).
//!
//! 가장 중요한 것 넷.
//!
//! 1. **미설정은 0이 아니다** — 정원 미설정 반을 0명으로 세면 총 정원이 줄고
//!    충원율이 부풀어 숫자가 거짓이 된다.
//! 2. **넘친 자리는 음수 그대로** — 몇 명이 넘쳤는지 담당자가 알아야 한다.
//! 3. **1·2·4 가 1~4 로 읽히면 안 된다** — 띄엄띄엄 받는 반이 실제로 있다.
//! 4. **정원·학년을 고쳐도 정산이 낡음이 되지 않는다** — 금액이 아니라
//!    운영정보이기 때문이다.

use crate::db::Db;
use crate::model::{
    CapacityStatus, CostItem, DepartmentInput, EnrollmentInput, Fee, SeatFinding, SeatQuery,
    StudentInput, WorkspaceInput,
};
use crate::repo;

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
                        start_date: "2026-03-01".into(),
                        end_date: "2026-03-31".into(),
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

    fn dept(&self, name: &str, class: &str, days: &str, cap: Option<i64>, grades: &[i64]) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class.into()),
                        teacher: Some("김강사".into()),
                        days: Some(days.into()),
                        note: None,
                        fees: vec![fee("INSTRUCTOR", 30_000)],
                        capacity: cap,
                        allowed_grades: grades.to_vec(),
                    },
                )
            })
            .unwrap()
    }

    fn student(&self, grade: i64, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no: "1".into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, student: i64, dept: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
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

    /// 정원만큼 새 학생을 넣는다. 학생 번호는 1~99 라 학년을 돌려 가며 쓴다.
    fn fill(&self, dept: i64, n: i64, grade: i64, from: i64) {
        for i in 0..n {
            let s = self.student(grade, from + i, &format!("학생{}", from + i));
            self.enroll(s, dept);
        }
    }

    fn stats(&self) -> crate::model::CapacityStats {
        self.db
            .read(|c| repo::capacity::stats(c, self.ws))
            .unwrap()
    }

    fn row(&self, name: &str, class: &str) -> crate::model::DeptCapacityRow {
        self.stats()
            .rows
            .into_iter()
            .find(|r| r.name == name && r.class_name == class)
            .unwrap_or_else(|| panic!("{name}{class} 을 찾지 못했습니다"))
    }

    fn seats(&self, q: SeatQuery) -> crate::model::SeatResult {
        self.db
            .read(|c| repo::capacity::find_seats(c, self.ws, &q))
            .unwrap()
    }
}

// ─────────────────────────────────────────────── 정원

#[test]
fn 정원은_1명_미만을_받지_않는다() {
    let f = F::new();

    let zero = f.db.write(|c| {
        repo::department::create(
            c,
            f.ws,
            &DepartmentInput {
                name: "로봇".into(),
                class_name: Some("A".into()),
                teacher: None,
                days: None,
                note: None,
                fees: Vec::new(),
                capacity: Some(0),
                allowed_grades: Vec::new(),
            },
        )
    });
    assert!(zero.is_err(), "정원 0명이 들어갔다");

    let minus = f.db.write(|c| {
        repo::department::create(
            c,
            f.ws,
            &DepartmentInput {
                name: "미술".into(),
                class_name: Some("A".into()),
                teacher: None,
                days: None,
                note: None,
                fees: Vec::new(),
                capacity: Some(-3),
                allowed_grades: Vec::new(),
            },
        )
    });
    assert!(minus.is_err(), "음수 정원이 들어갔다");

    // 1명짜리 반은 있다 (일대일 수업)
    f.dept("바둑", "A", "월", Some(1), &[3]);
    assert_eq!(f.row("바둑", "A").capacity, Some(1));
}

#[test]
fn 남은_자리와_충원율은_정원이_있을_때만_센다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    f.fill(a, 17, 1, 1);

    let r = f.row("로봇", "A");
    assert_eq!(r.current_count, 17);
    assert_eq!(r.remaining, Some(3));
    assert_eq!(r.fill_rate, Some(85.0));
    assert_eq!(r.status, CapacityStatus::Open);
}

#[test]
fn 정확히_찼으면_정원_도달이다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    f.fill(a, 20, 1, 1);

    let r = f.row("로봇", "A");
    assert_eq!(r.remaining, Some(0));
    assert_eq!(r.fill_rate, Some(100.0));
    assert_eq!(r.status, CapacityStatus::Full);
}

#[test]
fn 넘친_자리는_음수_그대로_둔다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    // 한 학년에 22명을 넣으려면 번호가 1~99 안에 들어간다
    f.fill(a, 22, 1, 1);

    let r = f.row("로봇", "A");
    assert_eq!(r.current_count, 22);
    assert_eq!(r.remaining, Some(-2), "넘친 자리를 0으로 올렸다");
    assert_eq!(r.fill_rate, Some(110.0));
    assert_eq!(r.status, CapacityStatus::Over);
}

#[test]
fn 정원_미설정은_0명이_아니다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", None, &[1]);
    f.fill(a, 5, 1, 1);

    let r = f.row("로봇", "A");
    assert_eq!(r.current_count, 5);
    assert_eq!(r.remaining, None, "미설정인데 남은 자리를 셌다");
    assert_eq!(r.fill_rate, None, "미설정인데 충원율을 냈다");
    assert_eq!(r.status, CapacityStatus::Unset);

    // 요약에서도 정원 쪽 합계에 끼지 않는다
    let s = f.stats().summary;
    assert_eq!(s.classes, 1);
    assert_eq!(s.classes_with_capacity, 0);
    assert_eq!(s.classes_without_capacity, 1);
    assert_eq!(s.total_capacity, 0);
    assert_eq!(s.avg_fill_rate, None);
    // 수강 건수는 미설정 반도 그대로 센다
    assert_eq!(s.total_enrollments, 5);
}

#[test]
fn 취소한_수강은_현재_인원에서_빠진다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    let s1 = f.student(1, 1, "가");
    let s2 = f.student(1, 2, "나");
    f.enroll(s1, a);
    let e2 = f.enroll(s2, a);
    assert_eq!(f.row("로봇", "A").current_count, 2);

    f.db.write(|c| repo::enrollment::cancel(c, e2, None, "중도 포기", &f.items))
        .unwrap();
    let r = f.row("로봇", "A");
    assert_eq!(r.current_count, 1, "취소한 학생이 아직 세어진다");
    assert_eq!(r.remaining, Some(19));
}

#[test]
fn 수강_건수와_수강_학생_수는_다르다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[3]);
    let b = f.dept("미술", "A", "화", Some(20), &[3]);
    let c = f.dept("바둑", "A", "수", Some(20), &[3]);

    let 한명 = f.student(3, 1, "김민수");
    f.enroll(한명, a);
    f.enroll(한명, b);
    f.enroll(한명, c);

    let s = f.stats().summary;
    assert_eq!(s.total_enrollments, 3, "수강 건수");
    assert_eq!(s.total_students, 1, "수강 학생 수");

    let g = f.stats().grades;
    let g3 = g.iter().find(|x| x.grade == 3).unwrap();
    assert_eq!(g3.enrollments, 3);
    assert_eq!(g3.students, 1);
}

#[test]
fn 윗줄_남은_자리는_넘친_반_때문에_줄지_않는다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    let b = f.dept("미술", "A", "화", Some(20), &[2]);
    f.fill(a, 22, 1, 1); // 2명 넘침
    f.fill(b, 15, 2, 1); // 5자리 남음

    let s = f.stats().summary;
    // sum(정원-현재) 였다면 3 이 되어 실제로 받을 수 있는 자리를 속인다
    assert_eq!(s.open_seats, 5, "넘친 반의 음수가 빈자리를 깎아먹었다");
    assert_eq!(s.over_classes, 1);
    assert_eq!(s.total_capacity, 40);
    assert_eq!(s.counted_current, 37);

    // 반별 표에서는 넘친 값을 그대로 보여 준다
    assert_eq!(f.row("로봇", "A").remaining, Some(-2));
}

// ─────────────────────────────────────────────── 수강 가능 학년

#[test]
fn 수강_가능_학년은_띄엄띄엄도_된다() {
    let f = F::new();
    f.dept("한학년", "A", "월", Some(10), &[1]);
    f.dept("두학년", "A", "월", Some(10), &[1, 2]);
    f.dept("띄엄", "A", "월", Some(10), &[1, 2, 4]);
    f.dept("전체", "A", "월", Some(10), &[1, 2, 3, 4, 5, 6]);
    f.dept("미설정", "A", "월", Some(10), &[]);

    assert_eq!(f.row("한학년", "A").allowed_grades, vec![1]);
    assert_eq!(f.row("두학년", "A").allowed_grades, vec![1, 2]);
    assert_eq!(
        f.row("띄엄", "A").allowed_grades,
        vec![1, 2, 4],
        "1·2·4 가 1~4 로 번졌다"
    );
    assert_eq!(f.row("전체", "A").allowed_grades, vec![1, 2, 3, 4, 5, 6]);
    assert!(
        f.row("미설정", "A").allowed_grades.is_empty(),
        "미설정이 빈 목록이 아니다"
    );
}

#[test]
fn 띄엄띄엄_학년은_찾기에서도_3학년을_받지_않는다() {
    let f = F::new();
    f.dept("띄엄", "A", "월", Some(10), &[1, 2, 4]);

    let r = f.seats(SeatQuery {
        grade: 3,
        student_id: None,
        include_closed: true,
    });
    let 줄수: usize = r.by_day.iter().map(|d| d.rows.len()).sum();
    assert_eq!(줄수, 0, "1·2·4 반이 3학년에게 보였다");

    // 4학년에게는 보인다
    let r4 = f.seats(SeatQuery {
        grade: 4,
        student_id: None,
        include_closed: true,
    });
    let 줄수4: usize = r4.by_day.iter().map(|d| d.rows.len()).sum();
    assert_eq!(줄수4, 1);
}

#[test]
fn 초등학교_밖의_학년은_받지_않는다() {
    let f = F::new();
    let bad = f.db.write(|c| {
        repo::department::create(
            c,
            f.ws,
            &DepartmentInput {
                name: "중학생반".into(),
                class_name: Some("A".into()),
                teacher: None,
                days: None,
                note: None,
                fees: Vec::new(),
                capacity: Some(10),
                allowed_grades: vec![1, 7],
            },
        )
    });
    assert!(bad.is_err(), "7학년이 들어갔다");
}

// ─────────────────────────────────────────────── 수강 가능 부서 찾기

/// 요구사항에 적힌 fixture 그대로.
fn 찾기_fixture(f: &F) {
    f.dept("로봇", "A", "월", Some(20), &[1, 2]);
    let b = f.dept("로봇", "B", "화", Some(20), &[3, 4]);
    let c = f.dept("미술", "A", "수", Some(25), &[3, 4, 5]);
    let d = f.dept("바둑", "A", "목", Some(15), &[]);
    let e = f.dept("음악", "A", "금", None, &[3]);

    f.fill(b, 20, 4, 1); // 정원 도달
    f.fill(c, 17, 5, 1); // 8자리 남음
    f.fill(d, 10, 6, 1);
    f.fill(e, 8, 2, 1);

    // 로봇A 는 18명 — 다른 학년으로 채운다
    let a = f.row("로봇", "A").department_id;
    f.fill(a, 18, 1, 1);
}

#[test]
fn 찾기는_신청_가능과_확인_필요를_가른다() {
    let f = F::new();
    찾기_fixture(&f);

    let r = f.seats(SeatQuery {
        grade: 3,
        student_id: None,
        include_closed: false,
    });
    let 모두: Vec<_> = r.by_day.iter().flat_map(|d| d.rows.iter()).collect();
    let 이름 = |x: &crate::model::SeatRow| format!("{}{}", x.name, x.class_name);

    let 신청가능: Vec<String> = 모두
        .iter()
        .filter(|x| x.finding == SeatFinding::Open)
        .map(|x| 이름(x))
        .collect();
    assert_eq!(신청가능, vec!["미술A"], "신청 가능이 다르다");
    let 미술 = 모두.iter().find(|x| 이름(x) == "미술A").unwrap();
    assert_eq!(미술.remaining, Some(8));

    let mut 확인필요: Vec<(String, SeatFinding)> = 모두
        .iter()
        .filter(|x| matches!(x.finding, SeatFinding::GradeUnknown | SeatFinding::CapacityUnknown))
        .map(|x| (이름(x), x.finding))
        .collect();
    확인필요.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        확인필요,
        vec![
            ("바둑A".to_string(), SeatFinding::GradeUnknown),
            ("음악A".to_string(), SeatFinding::CapacityUnknown),
        ]
    );

    // 로봇A 는 대상 학년이 아니다 — 확인 필요도 아니고 아예 안 나온다
    assert!(모두.iter().all(|x| 이름(x) != "로봇A"), "대상 학년이 아닌 반이 나왔다");
    // 로봇B 는 정원 도달이라 기본 검색에서 빠진다
    assert!(모두.iter().all(|x| 이름(x) != "로봇B"), "정원 도달 반이 기본 검색에 나왔다");
}

#[test]
fn 마감_부서도_보기를_켜면_정원_도달도_보인다() {
    let f = F::new();
    찾기_fixture(&f);

    let r = f.seats(SeatQuery {
        grade: 3,
        student_id: None,
        include_closed: true,
    });
    let 로봇b = r
        .by_day
        .iter()
        .flat_map(|d| d.rows.iter())
        .find(|x| x.name == "로봇" && x.class_name == "B")
        .expect("로봇B 가 보이지 않는다");
    assert_eq!(로봇b.finding, SeatFinding::Full);
    assert_eq!(로봇b.remaining, Some(0));
}

#[test]
fn 학생을_고르면_이미_듣는_반을_뺀다() {
    let f = F::new();
    찾기_fixture(&f);
    let 미술 = f.row("미술", "A").department_id;

    let 김민수 = f.student(3, 50, "김민수");
    f.enroll(김민수, 미술);

    let r = f.seats(SeatQuery {
        grade: 3,
        student_id: Some(김민수),
        include_closed: false,
    });
    let 모두: Vec<_> = r.by_day.iter().flat_map(|d| d.rows.iter()).collect();
    assert!(
        모두.iter().all(|x| x.name != "미술"),
        "이미 듣는 반이 추천에 남았다"
    );
    assert_eq!(r.excluded, vec!["미술A".to_string()]);

    // 추천은 조회일 뿐이다 — 수강이 늘지 않았다
    let n: i64 = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM enrollment WHERE workspace_id = ?1",
                rusqlite::params![f.ws],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(n, 1 + 20 + 17 + 10 + 8 + 18, "찾기가 수강을 건드렸다");
}

#[test]
fn 취소한_수강만_있으면_빼지_않는다() {
    let f = F::new();
    let 미술 = f.dept("미술", "A", "수", Some(25), &[3]);
    let 김민수 = f.student(3, 50, "김민수");
    let e = f.enroll(김민수, 미술);
    f.db.write(|c| repo::enrollment::cancel(c, e, None, "중도 포기", &f.items))
        .unwrap();

    let r = f.seats(SeatQuery {
        grade: 3,
        student_id: Some(김민수),
        include_closed: false,
    });
    let 모두: Vec<_> = r.by_day.iter().flat_map(|d| d.rows.iter()).collect();
    assert_eq!(모두.len(), 1, "지금 듣지 않는데 추천에서 빠졌다");
    assert!(r.excluded.is_empty());
}

// ─────────────────────────────────────────────── 요일

#[test]
fn 여러_요일_반은_각_요일에_모두_센다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월,수", Some(20), &[1]);
    let b = f.dept("미술", "A", "화", Some(10), &[2]);
    let c = f.dept("미정", "A", "미정", None, &[3]);
    f.fill(a, 5, 1, 1);
    f.fill(b, 3, 2, 1);
    f.fill(c, 2, 3, 1);

    let w = f.stats().weekdays;
    let day = |d: &str| w.iter().find(|x| x.day == d).unwrap().clone();

    assert_eq!(day("월").classes, 1);
    assert_eq!(day("월").enrollments, 5);
    assert_eq!(day("수").classes, 1);
    assert_eq!(day("수").enrollments, 5, "수요일 수업이 수요일에서 빠졌다");
    assert_eq!(day("화").enrollments, 3);
    assert_eq!(day("목").classes, 0);

    // 요일을 읽을 수 없는 반은 조용히 사라지지 않는다
    assert_eq!(day("미지정").classes, 1);
    assert_eq!(day("미지정").enrollments, 2);

    // 세로 합계는 전체 수강 건수보다 크다 — 그래서 합계 줄을 두지 않는다
    let 세로합: i64 = w.iter().map(|x| x.enrollments).sum();
    assert_eq!(세로합, 5 + 5 + 3 + 2);
    assert_eq!(f.stats().summary.total_enrollments, 10);
    assert!(세로합 > f.stats().summary.total_enrollments);

    // 요일 차례는 가나다순이 아니라 월→일
    let 차례: Vec<String> = w.iter().map(|x| x.day.clone()).collect();
    assert_eq!(
        차례,
        vec!["월", "화", "수", "목", "금", "토", "일", "미지정"]
    );
}

// ─────────────────────────────────────────────── 학년별

#[test]
fn 참여율의_분모는_그_학년_전체_학생_수다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    // 1학년 10명 가운데 4명만 수강
    for i in 1..=10 {
        let s = f.student(1, i, &format!("학생{i}"));
        if i <= 4 {
            f.enroll(s, a);
        }
    }
    // 2학년 5명은 아무도 듣지 않는다
    for i in 1..=5 {
        f.student(2, i, &format!("둘{i}"));
    }

    let g = f.stats().grades;
    let g1 = g.iter().find(|x| x.grade == 1).unwrap();
    assert_eq!(g1.students, 4);
    assert_eq!(g1.total_students, 10);
    assert_eq!(g1.join_rate, Some(40.0));

    let g2 = g.iter().find(|x| x.grade == 2).unwrap();
    assert_eq!(g2.students, 0, "수강생 없는 학년이 줄에서 사라졌다");
    assert_eq!(g2.total_students, 5);
    assert_eq!(g2.join_rate, Some(0.0));
}

// ─────────────────────────────────────────────── 정산과 분리

/// 정원과 수강 가능 학년은 **운영정보**다. 금액 계산에 들어가지 않으므로
/// 이것만 고쳤다고 이미 만든 정산이 낡음이 되어서는 안 된다.
///
/// v0.1.6 까지는 `AFTER UPDATE ON department` 트리거가 **어느 칸을 고쳐도**
/// 작업공간 자료판을 올렸다. 006 에서 값을 견주는 `WHEN` 으로 바꾸었다.
#[test]
fn 정원과_학년을_고쳐도_정산이_낡음이_되지_않는다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1, 2]);
    f.fill(a, 3, 1, 1);

    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    let before = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(before.state, "FRESH", "정산을 만들지 못했다");

    let 자료판 = |f: &F| -> i64 {
        f.db.read(|c| {
            Ok(c.query_row(
                "SELECT data_version FROM workspace WHERE id = ?1",
                rusqlite::params![f.ws],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    };
    let v0 = 자료판(&f);
    let 배분0: i64 = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COALESCE(SUM(amount), 0) FROM settlement_alloc",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();

    // 정원과 학년만 바꾼다. 이름·요일·금액은 그대로.
    f.db.write(|c| {
        repo::department::update(
            c,
            a,
            &DepartmentInput {
                name: "로봇".into(),
                class_name: Some("A".into()),
                teacher: Some("김강사".into()),
                days: Some("월".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)],
                capacity: Some(25),
                allowed_grades: vec![1, 2, 3],
            },
        )
    })
    .unwrap();

    assert_eq!(자료판(&f), v0, "정원·학년을 고쳤는데 자료판이 올랐다");
    let after = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(after.state, "FRESH", "정산이 낡음이 되었다");
    assert_eq!(after.settlement_id, before.settlement_id);

    let 배분1: i64 = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COALESCE(SUM(amount), 0) FROM settlement_alloc",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(배분1, 배분0, "배분 총액이 바뀌었다");
}

/// 거꾸로, 금액에 영향을 주는 변경은 **그대로 낡음이 되어야 한다.**
/// 트리거를 좁히다가 꼭 필요한 감지까지 잃으면 담당자가 조용히 낡은 숫자를 본다.
#[test]
fn 기준금액이나_요일을_고치면_예전처럼_낡음이_된다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    f.fill(a, 3, 1, 1);
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();

    f.db.write(|c| {
        repo::department::update(
            c,
            a,
            &DepartmentInput {
                name: "로봇".into(),
                class_name: Some("A".into()),
                teacher: Some("김강사".into()),
                days: Some("화".into()), // 요일이 바뀌었다
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)],
                capacity: Some(20),
                allowed_grades: vec![1],
            },
        )
    })
    .unwrap();

    let after = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_ne!(after.state, "FRESH", "요일을 고쳤는데 낡음이 되지 않았다");
}

/// 아무것도 바꾸지 않은 저장은 자료판을 올리지 않는다 (006 의 덤).
#[test]
fn 값이_그대로인_저장은_자료판을_올리지_않는다() {
    let f = F::new();
    let a = f.dept("로봇", "A", "월", Some(20), &[1]);
    let same = DepartmentInput {
        name: "로봇".into(),
        class_name: Some("A".into()),
        teacher: Some("김강사".into()),
        days: Some("월".into()),
        note: None,
        fees: vec![fee("INSTRUCTOR", 30_000)],
        capacity: Some(20),
        allowed_grades: vec![1],
    };
    let 자료판 = || -> i64 {
        f.db.read(|c| {
            Ok(c.query_row(
                "SELECT data_version FROM workspace WHERE id = ?1",
                rusqlite::params![f.ws],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    };
    let v0 = 자료판();
    f.db.write(|c| repo::department::update(c, a, &same)).unwrap();
    assert_eq!(자료판(), v0);
}

// ─────────────────────────────────────────────── 변경이력

#[test]
fn 기준금액_정원_학년_변경은_서로_다른_줄로_남는다() {
    let f = F::new();
    let a = f.dept("로봇", "B", "월", Some(20), &[1, 2]);

    f.db.write(|c| {
        repo::department::update(
            c,
            a,
            &DepartmentInput {
                name: "로봇".into(),
                class_name: Some("B".into()),
                teacher: Some("김강사".into()),
                days: Some("월".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 35_000)],
                capacity: Some(25),
                allowed_grades: vec![1, 2, 3],
            },
        )
    })
    .unwrap();

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, None, 100))
        .unwrap();
    let find = |kind: &str| {
        logs.iter()
            .find(|l| l.kind == kind)
            .unwrap_or_else(|| panic!("{kind} 이력이 없다"))
            .clone()
    };

    let fee_log = find("DEPT_FEE_EDIT");
    assert!(fee_log.before_value.contains("30,000"), "{:?}", fee_log);
    assert!(fee_log.after_value.contains("35,000"), "{:?}", fee_log);
    assert_eq!(fee_log.target, "로봇B");

    let cap = find("DEPT_CAPACITY_EDIT");
    assert_eq!(cap.before_value, "정원 20명");
    assert_eq!(cap.after_value, "정원 25명");

    let grade = find("DEPT_ALLOWED_GRADE_EDIT");
    assert_eq!(grade.before_value, "1·2학년");
    assert_eq!(grade.after_value, "1·2·3학년");

    // 학생 금액에 반영한 일(DEPT_APPLY)과는 다른 사건이다 — 섞이면 안 된다
    assert!(
        logs.iter().all(|l| l.kind != "DEPT_APPLY"),
        "기준금액만 고쳤는데 재반영 이력이 남았다"
    );
}

#[test]
fn 바뀌지_않은_것은_이력에_남기지_않는다() {
    let f = F::new();
    let a = f.dept("로봇", "B", "월", Some(20), &[1, 2]);
    let same = DepartmentInput {
        name: "로봇".into(),
        class_name: Some("B".into()),
        teacher: Some("김강사".into()),
        days: Some("월".into()),
        note: None,
        fees: vec![fee("INSTRUCTOR", 30_000)],
        capacity: Some(20),
        allowed_grades: vec![1, 2],
    };
    f.db.write(|c| repo::department::update(c, a, &same)).unwrap();

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, None, 100))
        .unwrap();
    assert!(logs.is_empty(), "바뀐 것이 없는데 이력이 남았다: {logs:?}");
}

#[test]
fn 미설정으로_되돌린_것도_이력에_남는다() {
    let f = F::new();
    let a = f.dept("로봇", "B", "월", Some(20), &[1, 2]);
    f.db.write(|c| {
        repo::department::update(
            c,
            a,
            &DepartmentInput {
                name: "로봇".into(),
                class_name: Some("B".into()),
                teacher: Some("김강사".into()),
                days: Some("월".into()),
                note: None,
                fees: vec![fee("INSTRUCTOR", 30_000)],
                capacity: None,
                allowed_grades: Vec::new(),
            },
        )
    })
    .unwrap();

    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, None, 100))
        .unwrap();
    let cap = logs.iter().find(|l| l.kind == "DEPT_CAPACITY_EDIT").unwrap();
    assert_eq!(cap.after_value, "정원 미설정");
    let g = logs
        .iter()
        .find(|l| l.kind == "DEPT_ALLOWED_GRADE_EDIT")
        .unwrap();
    assert_eq!(g.after_value, "미설정");
}
