#![allow(non_snake_case)]
//! 학생별 징수 내역 (행정자료, v0.1.3).
//!
//! **정산 결과가 아니다.** `Enrollment + Charge` 만 본다. 그래서 정산이 없거나
//! 낡아도 조회된다 — 정산 **전에** 금액을 대조하는 자료이기 때문이다.

use std::path::PathBuf;

use crate::excel::read;

use crate::db::Db;
use crate::model::{
    CostItem, DepartmentInput, EligibilityInput, Enrollment, EnrollmentFilter, EnrollmentInput,
    Fee, StudentInput, WorkspaceInput,
};
use crate::repo;

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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
        let ws = db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    year,
                    &WorkspaceInput {
                        name: "4월".into(),
                        start_date: "2026-04-01".into(),
                        end_date: "2026-04-30".into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        Self { db, year, ws, items }
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

    fn dept(&self, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class_name.into()),
                        teacher: None,
                        days: None,
                        note: None,
                        fees,
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

    fn elig(&self, student: i64, program: &str) {
        self.db
            .write(|c| {
                repo::eligibility::create(
                    c,
                    self.year,
                    &EligibilityInput {
                        student_id: student,
                        program: program.into(),
                        valid_from: None,
                        valid_to: None,
                        note: None,
                    },
                )
            })
            .unwrap();
    }

    fn 취소(&self, e: i64, fees: Option<&[Fee]>, reason: &str) {
        self.db
            .write(|c| repo::enrollment::cancel(c, e, fees, reason, &self.items))
            .unwrap();
    }

    fn 내역(&self, f: &EnrollmentFilter) -> Vec<Enrollment> {
        self.db
            .read(|c| repo::enrollment::list_by_student(c, self.ws, &self.items, f))
            .unwrap()
    }

    /// 집계 서비스 — 화면과 Excel 이 함께 쓰는 결과 그대로.
    fn 보고서(&self, f: &EnrollmentFilter) -> crate::model::FeeReport {
        self.db
            .read(|c| repo::fee_report::build(c, self.ws, &self.items, f))
            .unwrap()
    }

    /// 학생 수 · 수강 건수 · 총액.
    fn 요약(&self, f: &EnrollmentFilter) -> (usize, usize, i64) {
        let r = self.보고서(f);
        (r.students as usize, r.enrollments as usize, r.total)
    }

    /// 한 학생 줄의 항목 금액.
    fn 금액(row: &crate::model::StudentSumRow, code: &str) -> i64 {
        row.fees
            .iter()
            .find(|x| x.item_code == code)
            .map(|x| x.amount)
            .unwrap_or(0)
    }
}

fn 원래_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 30_000),
        fee(수용비, 3_000),
        fee(교재비, 15_000),
        fee(재료비, 10_000),
    ]
}

fn 중도취소_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 15_000),
        fee(수용비, 1_500),
        fee(교재비, 15_000),
        fee(재료비, 10_000),
    ]
}

fn 전액면제() -> Vec<Fee> {
    vec![
        fee(강사료, 0),
        fee(수용비, 0),
        fee(교재비, 0),
        fee(재료비, 0),
    ]
}

#[test]
fn 한_학생이_여러_부서를_수강하면_부서별로_한_줄이다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    let d1 = f.dept("로봇과학", "A", 원래_금액());
    let d2 = f.dept("축구", "A", vec![fee(강사료, 25_000), fee(재료비, 5_000)]);
    f.enroll(hana, d1);
    f.enroll(hana, d2);

    let rows = f.내역(&EnrollmentFilter::default());
    assert_eq!(rows.len(), 2, "부서별로 한 줄");
    assert_eq!(rows[0].dept_label, "로봇과학A");
    assert_eq!(rows[1].dept_label, "축구A");
    assert_eq!(rows[0].total, 58_000);
    assert_eq!(rows[1].total, 30_000);

    let (students, enrollments, total) = f.요약(&EnrollmentFilter::default());
    assert_eq!(students, 1, "행 수가 아니라 실제 학생 수");
    assert_eq!(enrollments, 2);
    assert_eq!(total, 88_000);
}

#[test]
fn 취소자도_확정된_징수금액으로_나온다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(hana, d);
    f.취소(e, Some(&중도취소_금액()), "중도 포기");

    let rows = f.내역(&EnrollmentFilter::default());
    assert_eq!(rows.len(), 1, "취소자가 사라지지 않는다");
    assert_eq!(rows[0].status, "CANCELLED");
    assert_eq!(rows[0].total, 41_500, "취소할 때 확정한 금액");
}

#[test]
fn 합계_0원인_취소자도_화면에_나온다() {
    // 정산에서는 빠지지만, 상태 확인을 위해 이 화면에는 보여 준다.
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(hana, d);
    f.취소(e, Some(&전액면제()), "개강 전 취소");

    let rows = f.내역(&EnrollmentFilter::default());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total, 0);
    assert_eq!(rows[0].status, "CANCELLED");

    let (students, enrollments, total) = f.요약(&EnrollmentFilter::default());
    assert_eq!((students, enrollments, total), (1, 1, 0));
}

#[test]
fn 학생_우선_자연정렬로_나온다() {
    // 수강생 명단은 부서 우선이다. 이 자료는 학생 우선이어야 한다.
    let f = F::new();
    let d1 = f.dept("가부서", "A", vec![fee(강사료, 1_000)]);
    let d2 = f.dept("나부서", "A", vec![fee(강사료, 1_000)]);

    let s10 = f.student(1, "10", 1, "십반");
    let s2 = f.student(1, "2", 1, "이반");
    let s1 = f.student(1, "1", 1, "일반");
    let s가 = f.student(1, "가", 1, "가반");
    let s2g = f.student(2, "1", 1, "이학년");

    // 넣는 차례를 섞어도 결과가 같아야 한다
    for s in [s가, s10, s2g, s1, s2] {
        f.enroll(s, d2);
        f.enroll(s, d1);
    }

    let rows = f.내역(&EnrollmentFilter::default());
    let 차례: Vec<String> = rows
        .iter()
        .map(|r| format!("{}-{}-{}", r.grade, r.class_no, r.dept_label))
        .collect();
    assert_eq!(
        차례,
        vec![
            "1-1-가부서A",
            "1-1-나부서A",
            "1-2-가부서A",
            "1-2-나부서A",
            "1-10-가부서A",
            "1-10-나부서A",
            "1-가-가부서A",
            "1-가-나부서A",
            "2-1-가부서A",
            "2-1-나부서A",
        ],
        "학년 → 반 자연정렬 → 번호 → 부서"
    );
}

#[test]
fn 필터_다섯_가지가_각각_듣는다() {
    let f = F::new();
    let d1 = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let d2 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 3, "이두리");
    f.enroll(a, d1);
    f.enroll(a, d2);
    let eb = f.enroll(b, d1);
    f.취소(eb, Some(&[fee(강사료, 5_000)]), "포기");

    let 걸러 = |f2: EnrollmentFilter| f.내역(&f2).len();

    assert_eq!(걸러(EnrollmentFilter::default()), 3, "전체");
    assert_eq!(
        걸러(EnrollmentFilter {
            grade: Some(1),
            ..Default::default()
        }),
        2,
        "학년"
    );
    assert_eq!(
        걸러(EnrollmentFilter {
            class_no: Some("나리".into()),
            ..Default::default()
        }),
        1,
        "반 (한글)"
    );
    assert_eq!(
        걸러(EnrollmentFilter {
            department_id: Some(d1),
            ..Default::default()
        }),
        2,
        "부서"
    );
    assert_eq!(
        걸러(EnrollmentFilter {
            status: Some("CANCELLED".into()),
            ..Default::default()
        }),
        1,
        "수강상태"
    );
    assert_eq!(
        걸러(EnrollmentFilter {
            query: Some("하나".into()),
            ..Default::default()
        }),
        2,
        "이름 부분검색"
    );
}

#[test]
fn 필터를_겹쳐_쓰면_AND_로_걸린다() {
    let f = F::new();
    let d1 = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let d2 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "나리", 2, "김두리");
    f.enroll(a, d1);
    f.enroll(a, d2);
    f.enroll(b, d1);

    let rows = f.내역(&EnrollmentFilter {
        grade: Some(1),
        class_no: Some("가람".into()),
        department_id: Some(d1),
        query: Some("김".into()),
        ..Default::default()
    });
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "김하나");
    assert_eq!(rows[0].dept_label, "로봇과학A");
}

#[test]
fn 요약의_학생_수는_중복을_뺀_수다() {
    let f = F::new();
    let d1 = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let d2 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    let d3 = f.dept("축구", "A", vec![fee(강사료, 30_000)]);
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    f.enroll(a, d1);
    f.enroll(a, d2);
    f.enroll(a, d3);
    f.enroll(b, d1);

    let (students, enrollments, total) = f.요약(&EnrollmentFilter::default());
    assert_eq!(students, 2, "행은 4개지만 학생은 2명");
    assert_eq!(enrollments, 4);
    assert_eq!(total, 70_000);
}

#[test]
fn 요약은_필터_결과를_따른다() {
    let f = F::new();
    let d1 = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 1, "이두리");
    f.enroll(a, d1);
    f.enroll(b, d1);

    let (students, enrollments, total) = f.요약(&EnrollmentFilter {
        grade: Some(1),
        ..Default::default()
    });
    assert_eq!((students, enrollments, total), (1, 1, 10_000));
}

#[test]
fn 정산이_없어도_조회된다() {
    // 이 자료는 charge 만 본다. 정산이 없어도 봐야 한다.
    let f = F::new();
    let hana = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(hana, d);

    let st = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(st.state, "NONE", "정산이 없는 상태");

    let rows = f.내역(&EnrollmentFilter::default());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total, 58_000);
}

#[test]
fn 정산이_낡아도_조회된다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(hana, d);
    f.db.write(|c| {
        repo::policy::save(c, f.year, "VOUCHER", 500_000, false, "", &[])?;
        repo::policy::save(c, f.year, "FREE_VOUCHER", 500_000, false, "", &[])
    })
    .unwrap();
    f.db.write(|c| repo::settle::generate(c, f.ws))
        .unwrap();

    // 금액을 고쳐 낡음으로 만든다
    f.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(강사료, 99_000)], "정정", &f.items)
    })
    .unwrap();
    let st = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_ne!(st.state, "FRESH", "낡은 상태");

    let rows = f.내역(&EnrollmentFilter::default());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total, 99_000 + 3_000 + 15_000 + 10_000);
}

#[test]
fn 지원유형이_eligibility에서_파생된다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let a = f.student(1, "가람", 1, "일반");
    let b = f.student(1, "가람", 2, "이용권");
    let c = f.student(1, "가람", 3, "자유");
    let dd = f.student(1, "가람", 4, "둘다");
    f.elig(b, "VOUCHER");
    f.elig(c, "FREE_VOUCHER");
    f.elig(dd, "VOUCHER");
    f.elig(dd, "FREE_VOUCHER");
    for s in [a, b, c, dd] {
        f.enroll(s, d);
    }

    let rows = f.내역(&EnrollmentFilter::default());
    assert!(rows[0].programs.is_empty(), "일반");
    assert_eq!(rows[1].programs, vec!["VOUCHER"]);
    assert_eq!(rows[2].programs, vec!["FREE_VOUCHER"]);
    assert_eq!(rows[3].programs, vec!["FREE_VOUCHER", "VOUCHER"]);
}

#[test]
fn 한글_반이_그대로_나온다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    for (cls, no, name) in [("가람", 1, "김동현"), ("나리", 2, "김희선"), ("다솜", 1, "박해선")] {
        let s = f.student(1, cls, no, name);
        f.enroll(s, d);
    }
    let rows = f.내역(&EnrollmentFilter::default());
    let 반: Vec<&str> = rows.iter().map(|r| r.class_no.as_str()).collect();
    assert_eq!(반, vec!["가람", "나리", "다솜"]);
}

// ─────────────────────────────────── 학생 단위 집계 (v0.1.4)
//
// 이 메뉴의 목적은 **한 학생에게 모두 얼마를 징수하는가**다. 그래서 목록은
// 학생당 한 줄이고, 금액은 그 학생이 듣는 모든 부서를 항목별로 더한 값이다.

#[test]
fn 여러_부서를_들으면_항목별로_더해_한_줄이_된다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    let 로봇 = f.dept("로봇과학", "A", vec![fee(강사료, 30_000), fee(재료비, 5_000)]);
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000), fee(교재비, 2_000)]);
    let 축구 = f.dept("축구", "A", vec![fee(강사료, 25_000)]);
    f.enroll(hana, 로봇);
    f.enroll(hana, 미술);
    f.enroll(hana, 축구);

    let r = f.보고서(&EnrollmentFilter::default());
    assert_eq!(r.rows.len(), 1, "학생 한 명이므로 한 줄");
    let row = &r.rows[0];
    assert_eq!(row.details, 3, "부서 세 곳");
    assert_eq!(F::금액(row, 강사료), 75_000, "30,000 + 20,000 + 25,000");
    assert_eq!(F::금액(row, 재료비), 5_000);
    assert_eq!(F::금액(row, 교재비), 2_000);
    assert_eq!(F::금액(row, 수용비), 0);
    assert_eq!(row.total, 82_000, "네 항목 합계");
}

#[test]
fn 학생_줄_합계는_항목_합계의_합이다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(a, d);

    for row in &f.보고서(&EnrollmentFilter::default()).rows {
        assert_eq!(row.fees.iter().map(|x| x.amount).sum::<i64>(), row.total);
    }
}

#[test]
fn 목록_합계와_상세_합계가_같다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 1, "이두리");
    let d1 = f.dept("로봇과학", "A", 원래_금액());
    let d2 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(a, d1);
    f.enroll(a, d2);
    let eb = f.enroll(b, d1);
    f.취소(eb, Some(&중도취소_금액()), "중도 포기");

    let r = f.보고서(&EnrollmentFilter::default());
    // 목록 총액 = 상세 총액 = 항목별 총합의 합
    let 목록: i64 = r.rows.iter().map(|x| x.total).sum();
    let 상세: i64 = r.details.iter().map(|x| x.total).sum();
    assert_eq!(목록, 상세, "목록과 상세가 어긋난다");
    assert_eq!(r.total, 목록);
    assert_eq!(r.fees.iter().map(|x| x.amount).sum::<i64>(), r.total);

    // 학생마다도 맞는가
    for row in &r.rows {
        let 그_학생: i64 = r
            .details
            .iter()
            .filter(|e| e.student_id == row.student_id)
            .map(|e| e.total)
            .sum();
        assert_eq!(row.total, 그_학생, "{} 학생", row.name);
    }
}

#[test]
fn 취소자도_확정_금액으로_합산된다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let 로봇 = f.dept("로봇과학", "A", 원래_금액());
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(a, 로봇);
    let e2 = f.enroll(a, 미술);
    f.취소(e2, Some(&[fee(강사료, 8_000)]), "중도 포기");

    let r = f.보고서(&EnrollmentFilter::default());
    let row = &r.rows[0];
    assert_eq!(row.details, 2, "취소한 수강도 상세에 남는다");
    assert_eq!(F::금액(row, 강사료), 38_000, "30,000 + 취소 확정 8,000");
    // 로봇과학 58,000 + 미술 취소 확정 8,000
    assert_eq!(row.total, 66_000);
}

#[test]
fn 전액_면제로_취소하면_0원으로_들어온다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(a, d);
    f.취소(e, Some(&전액면제()), "개강 전 취소");

    let r = f.보고서(&EnrollmentFilter::default());
    assert_eq!(r.rows.len(), 1, "0원이어도 학생은 나온다");
    assert_eq!(r.rows[0].total, 0);
    assert_eq!(r.total, 0);
}

#[test]
fn 부서_필터는_학생을_찾고_금액은_전체다() {
    let f = F::new();
    let 듣는이 = f.student(1, "가람", 1, "김하나");
    let 안듣는이 = f.student(1, "가람", 2, "이두리");
    let 로봇 = f.dept("로봇과학", "A", vec![fee(강사료, 30_000)]);
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(듣는이, 로봇);
    f.enroll(듣는이, 미술);
    f.enroll(안듣는이, 미술);

    let r = f.보고서(&EnrollmentFilter {
        department_id: Some(로봇),
        ..Default::default()
    });
    assert_eq!(r.rows.len(), 1, "로봇과학을 듣는 학생만");
    assert_eq!(r.rows[0].name, "김하나");
    // 로봇과학으로 찾았지만 금액은 미술까지 더한 전체다
    assert_eq!(r.rows[0].total, 50_000, "30,000 + 20,000");
    assert_eq!(r.rows[0].details, 2, "상세에는 미술도 있다");
    assert_eq!(r.enrollments, 2);
}

#[test]
fn 수강상태_필터도_학생을_찾는_조건이다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    let 로봇 = f.dept("로봇과학", "A", vec![fee(강사료, 30_000)]);
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(a, 로봇);
    let e = f.enroll(a, 미술);
    f.취소(e, Some(&[fee(강사료, 5_000)]), "중도 포기");
    f.enroll(b, 로봇);

    let r = f.보고서(&EnrollmentFilter {
        status: Some("CANCELLED".into()),
        ..Default::default()
    });
    assert_eq!(r.rows.len(), 1, "취소가 하나라도 있는 학생");
    assert_eq!(r.rows[0].name, "김하나");
    // 취소로 찾았어도 수강중인 로봇과학 금액까지 더한다
    assert_eq!(r.rows[0].total, 35_000, "30,000 + 5,000");
}

#[test]
fn 학년_반_필터는_그대로_학생을_가른다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 1, "이두리");
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    f.enroll(a, d);
    f.enroll(b, d);

    let r = f.보고서(&EnrollmentFilter {
        grade: Some(1),
        ..Default::default()
    });
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].name, "김하나");
    assert_eq!(r.total, 10_000);
}

#[test]
fn 목록은_학생_우선_자연정렬이다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 1_000)]);
    for (cls, no) in [("10", 1), ("2", 1), ("1", 1), ("가", 1)] {
        let s = f.student(1, cls, no, "아무개");
        f.enroll(s, d);
    }
    let r = f.보고서(&EnrollmentFilter::default());
    let 반: Vec<&str> = r.rows.iter().map(|x| x.class_no.as_str()).collect();
    assert_eq!(반, vec!["1", "2", "10", "가"], "숫자 반 자연정렬");
}

#[test]
fn 지원유형이_학생_줄에도_실린다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    f.elig(a, "VOUCHER");
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    f.enroll(a, d);

    let r = f.보고서(&EnrollmentFilter::default());
    assert_eq!(r.rows[0].programs, vec!["VOUCHER".to_string()]);
}

#[test]
fn 정산이_없거나_낡아도_학생_집계가_나온다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let e = f.enroll(a, d);

    // 정산 전
    assert_eq!(f.보고서(&EnrollmentFilter::default()).total, 10_000);

    // 정산한 뒤 금액을 고쳐 낡게 만든다
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    f.db
        .write(|c| repo::enrollment::update_fees(c, e, &[fee(강사료, 12_000)], "인상", &f.items))
        .unwrap();
    let st = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_ne!(st.state, "FRESH", "낡음이어야 한다");
    assert_eq!(
        f.보고서(&EnrollmentFilter::default()).total,
        12_000,
        "낡아도 원본 charge 로 조회된다"
    );
}

// ─────────────────────────────────── 학생별 징수 내역 Excel

fn 징수내역_파일(f: &F, filter: &EnrollmentFilter, cond: &str, tag: &str) -> PathBuf {
    let dir = tmp_dir(&format!("feereport-{tag}"));
    let made = f
        .db
        .read(|c| {
            crate::excel::export_fee_report(
                c,
                f.ws,
                &f.items,
                filter,
                &["2026학년도", "4월"],
                cond,
                &dir,
            )
        })
        .unwrap();
    PathBuf::from(&made.path)
}

/// 첫 장(학생별 합계). 맨 아래 합계 줄을 뺀다.
fn 합계장(path: &PathBuf) -> read::Sheet {
    let mut s = read::read_sheet_at(path, 0).unwrap();
    s.rows.pop();
    s
}

#[test]
fn Excel_첫_장은_학생별_합계다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    f.elig(hana, "VOUCHER");
    let 로봇 = f.dept("로봇과학", "A", 원래_금액());
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(hana, 로봇);
    f.enroll(hana, 미술);

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "sheet1");
    let sheet = read::read_sheet_at(&path, 0).unwrap();

    for name in ["학년", "반", "번호", "이름", "지원유형", "합계"] {
        assert!(sheet.require(name).is_ok(), "'{name}' 열이 없습니다");
    }
    for it in &f.items {
        assert!(sheet.require(&it.name).is_ok(), "'{}' 열이 없습니다", it.name);
    }
    // 학생당 한 줄이므로 부서·수강상태 열은 없다
    assert!(sheet.col("부서").is_none(), "첫 장에 부서 열이 있으면 안 된다");
    assert!(
        sheet.col("수강상태").is_none(),
        "첫 장에 수강상태 열이 있으면 안 된다"
    );

    assert_eq!(sheet.rows.len(), 2, "학생 1줄 + 합계");
    let (_, cells) = &sheet.rows[0];
    assert_eq!(sheet.cell(cells, sheet.col("반")), "가람");
    assert_eq!(sheet.cell(cells, sheet.col("지원유형")), "방과후 이용권");
    assert_eq!(
        sheet.cell(cells, sheet.col("합계")),
        "78000",
        "58,000 + 20,000"
    );
}

#[test]
fn Excel_둘째_장은_부서별_상세다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    let 로봇 = f.dept("로봇과학", "A", 원래_금액());
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(hana, 로봇);
    let e = f.enroll(hana, 미술);
    f.취소(e, Some(&[fee(강사료, 6_000)]), "중도 포기");

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "sheet2");
    let sheet = read::read_sheet_at(&path, 1).unwrap();

    for name in [
        "학년",
        "반",
        "번호",
        "이름",
        "지원유형",
        "부서",
        "합계",
        "수강상태",
    ] {
        assert!(sheet.require(name).is_ok(), "'{name}' 열이 없습니다");
    }
    assert_eq!(sheet.rows.len(), 2, "부서 두 곳");

    let 부서: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, c)| sheet.cell(c, sheet.col("부서")).to_string())
        .collect();
    assert_eq!(부서, vec!["로봇과학A", "미술A"]);

    let 상태: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, c)| sheet.cell(c, sheet.col("수강상태")).to_string())
        .collect();
    assert_eq!(상태, vec!["수강중", "수강취소"]);
}

#[test]
fn 두_장의_총액이_같다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 1, "이두리");
    let 로봇 = f.dept("로봇과학", "A", 원래_금액());
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(a, 로봇);
    f.enroll(a, 미술);
    let eb = f.enroll(b, 로봇);
    f.취소(eb, Some(&중도취소_금액()), "중도 포기");

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "both");
    let 첫 = 합계장(&path);
    let 둘째 = read::read_sheet_at(&path, 1).unwrap();

    let 합 = |s: &read::Sheet| -> i64 {
        s.rows
            .iter()
            .map(|(_, c)| read::parse_amount(s.cell(c, s.col("합계"))).unwrap())
            .sum()
    };
    assert_eq!(합(&첫), 합(&둘째), "첫 장과 둘째 장의 총액이 다르다");

    // 화면 결과와도 같아야 한다
    let r = f.보고서(&EnrollmentFilter::default());
    assert_eq!(합(&첫), r.total);
}

#[test]
fn Excel_첫_장이_화면_목록과_같다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "나리", 2, "이두리");
    let 로봇 = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(a, 로봇);
    f.enroll(b, 로봇);

    let filter = EnrollmentFilter::default();
    let r = f.보고서(&filter);
    let path = 징수내역_파일(&f, &filter, "", "same");
    let sheet = 합계장(&path);

    let 파일: Vec<(String, i64)> = sheet
        .rows
        .iter()
        .map(|(_, c)| {
            (
                sheet.cell(c, sheet.col("이름")).to_string(),
                read::parse_amount(sheet.cell(c, sheet.col("합계"))).unwrap(),
            )
        })
        .collect();
    let 화면: Vec<(String, i64)> = r.rows.iter().map(|x| (x.name.clone(), x.total)).collect();
    assert_eq!(파일, 화면, "화면과 파일의 줄이 다르다");
}

#[test]
fn Excel_합계_줄에_조건과_학생_수가_적힌다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let a = f.student(2, "나리", 1, "이두리");
    f.enroll(a, d);

    let path = 징수내역_파일(
        &f,
        &EnrollmentFilter {
            grade: Some(2),
            ..Default::default()
        },
        "2학년·나리반",
        "cond",
    );
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.contains("학생별징수내역"), "{name}");
    assert!(name.contains("2학년"), "파일 이름에 조건이 없습니다: {name}");

    // 헤더는 첫 줄에 그대로 있고, 조건은 맨 아래 합계 줄에 적힌다.
    let sheet = read::read_sheet_at(&path, 0).unwrap();
    assert!(sheet.require("학년").is_ok());
    assert_eq!(sheet.rows.len(), 2, "학생 1줄 + 합계 1줄");
    let (_, foot) = &sheet.rows[1];
    assert!(foot.iter().any(|v| v.contains("2학년")), "{foot:?}");
    assert!(foot.iter().any(|v| v.contains("학생 1명")), "{foot:?}");
}

#[test]
fn Excel도_필터를_학생_찾기로_쓴다() {
    let f = F::new();
    let 듣는이 = f.student(1, "가람", 1, "김하나");
    let 안듣는이 = f.student(1, "가람", 2, "이두리");
    let 로봇 = f.dept("로봇과학", "A", vec![fee(강사료, 30_000)]);
    let 미술 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    f.enroll(듣는이, 로봇);
    f.enroll(듣는이, 미술);
    f.enroll(안듣는이, 미술);

    let path = 징수내역_파일(
        &f,
        &EnrollmentFilter {
            department_id: Some(로봇),
            ..Default::default()
        },
        "로봇과학A 수강",
        "dept-filter",
    );
    let 첫 = 합계장(&path);
    assert_eq!(첫.rows.len(), 1, "로봇과학을 듣는 학생 한 명");
    assert_eq!(
        첫.cell(&첫.rows[0].1, 첫.col("합계")),
        "50000",
        "미술까지 더한 값"
    );

    // 둘째 장에는 그 학생의 미술 줄도 있어야 한다 — 그러지 않으면 총액이 어긋난다
    let 둘째 = read::read_sheet_at(&path, 1).unwrap();
    assert_eq!(둘째.rows.len(), 2);
    let 부서: Vec<String> = 둘째
        .rows
        .iter()
        .map(|(_, c)| 둘째.cell(c, 둘째.col("부서")).to_string())
        .collect();
    assert_eq!(부서, vec!["로봇과학A", "미술A"]);
}

#[test]
fn Excel에_0원_취소자도_들어간다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(a, d);
    f.취소(e, Some(&전액면제()), "개강 전 취소");

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "zero");
    let 첫 = 합계장(&path);
    assert_eq!(첫.rows.len(), 1, "0원이어도 넣는다");
    assert_eq!(첫.cell(&첫.rows[0].1, 첫.col("합계")), "0");

    let 둘째 = read::read_sheet_at(&path, 1).unwrap();
    assert_eq!(둘째.cell(&둘째.rows[0].1, 둘째.col("수강상태")), "수강취소");
}

#[test]
fn Excel도_학생_우선_차례다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 1_000)]);
    for (cls, no) in [("10", 1), ("2", 1), ("1", 1), ("가", 1)] {
        let s = f.student(1, cls, no, "아무개");
        f.enroll(s, d);
    }
    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "order");
    let 첫 = 합계장(&path);
    let 반: Vec<String> = 첫
        .rows
        .iter()
        .map(|(_, c)| 첫.cell(c, 첫.col("반")).to_string())
        .collect();
    assert_eq!(반, vec!["1", "2", "10", "가"], "숫자 반 자연정렬");
}

#[test]
fn Excel_금액은_숫자로_들어간다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(a, d);

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "num");
    for idx in [0usize, 1] {
        let s = read::read_sheet_at(&path, idx).unwrap();
        let (_, c) = &s.rows[0];
        assert_eq!(read::parse_amount(s.cell(c, s.col("합계"))).unwrap(), 58_000);
        for it in &f.items {
            assert!(
                read::parse_amount(s.cell(c, s.col(&it.name))).is_some(),
                "{} 장의 {} 가 숫자가 아니다",
                idx,
                it.name
            );
        }
    }
}
