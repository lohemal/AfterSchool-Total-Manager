#![allow(non_snake_case)]
//! 학생별 징수 내역 (행정자료, v0.1.3).
//!
//! **정산 결과가 아니다.** `Enrollment + Charge` 만 본다. 그래서 정산이 없거나
//! 낡아도 조회된다 — 정산 **전에** 금액을 대조하는 자료이기 때문이다.

use std::collections::HashSet;
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

    /// 명령 계층과 같은 셈 — 중복을 뺀 학생 수 · 건수 · 총액.
    fn 요약(&self, f: &EnrollmentFilter) -> (usize, usize, i64) {
        let rows = self.내역(f);
        let mut seen = HashSet::new();
        for r in &rows {
            seen.insert(r.student_id);
        }
        (seen.len(), rows.len(), rows.iter().map(|r| r.total).sum())
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

// ─────────────────────────────────── 학생별 징수 내역 Excel (v0.1.3)

/// 맨 아래 합계 줄을 뺀 자료 줄만 남긴다.
fn 자료만(mut sheet: read::Sheet) -> read::Sheet {
    sheet.rows.pop();
    sheet
}

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

#[test]
fn 징수내역_Excel에_화면과_같은_열이_들어간다() {
    let f = F::new();
    let hana = f.student(1, "가람", 1, "홍길동");
    f.elig(hana, "VOUCHER");
    let d = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(hana, d);

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "cols");
    let sheet = read::read_first_sheet(&path).unwrap();
    for name in ["학년", "반", "번호", "이름", "지원유형", "부서", "합계", "수강상태"] {
        assert!(sheet.require(name).is_ok(), "'{name}' 열이 없습니다");
    }
    for it in &f.items {
        assert!(sheet.require(&it.name).is_ok(), "'{}' 열이 없습니다", it.name);
    }
    // 정산 결과 열은 넣지 않는다
    assert!(sheet.col("이용권 사용액").is_none());
    assert!(sheet.col("자유수강권 사용액").is_none());

    let (_, cells) = &sheet.rows[0];
    assert_eq!(sheet.cell(cells, sheet.col("반")), "가람");
    assert_eq!(sheet.cell(cells, sheet.col("지원유형")), "방과후 이용권");
    assert_eq!(sheet.cell(cells, sheet.col("수강상태")), "수강중");
    assert_eq!(sheet.cell(cells, sheet.col("합계")), "58000");
}

#[test]
fn 징수내역_Excel이_화면_필터를_그대로_반영한다() {
    let f = F::new();
    let d1 = f.dept("로봇과학", "A", vec![fee(강사료, 10_000)]);
    let d2 = f.dept("미술", "A", vec![fee(강사료, 20_000)]);
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(2, "나리", 1, "이두리");
    f.enroll(a, d1);
    f.enroll(a, d2);
    f.enroll(b, d1);

    // 2학년만
    let path = 징수내역_파일(
        &f,
        &EnrollmentFilter {
            grade: Some(2),
            ..Default::default()
        },
        "2학년",
        "filter-grade",
    );
    let sheet = read::read_first_sheet(&path).unwrap();
    assert_eq!(sheet.rows.len(), 2, "2학년 한 줄 + 합계");
    assert_eq!(sheet.cell(&sheet.rows[0].1, sheet.col("이름")), "이두리");

    // 부서만
    let path = 징수내역_파일(
        &f,
        &EnrollmentFilter {
            department_id: Some(d1),
            ..Default::default()
        },
        "로봇과학A",
        "filter-dept",
    );
    let sheet = read::read_first_sheet(&path).unwrap();
    assert_eq!(sheet.rows.len(), 3, "자료 2줄 + 합계");
}

#[test]
fn 징수내역_Excel에_적용_조건이_적힌다() {
    // 전체 자료로 오해하지 않게 파일 이름과 시트 첫 줄에 조건을 적는다.
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
    let sheet = read::read_first_sheet(&path).unwrap();
    assert!(sheet.require("학년").is_ok());
    assert_eq!(sheet.rows.len(), 2, "자료 1줄 + 합계 1줄");
    let (_, foot) = &sheet.rows[1];
    assert!(foot.iter().any(|v| v.contains("2학년")), "{foot:?}");
    assert!(foot.iter().any(|v| v.contains("학생 1명")), "{foot:?}");
}

#[test]
fn 징수내역_Excel에_취소자와_한글_반이_남는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "나리", 2, "이두리");
    let d = f.dept("로봇과학", "A", 원래_금액());
    f.enroll(a, d);
    let eb = f.enroll(b, d);
    f.취소(eb, Some(&중도취소_금액()), "중도 포기");

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "cancel");
    let sheet = read::read_first_sheet(&path).unwrap();
    assert_eq!(sheet.rows.len(), 3, "자료 2줄 + 합계");
    let sheet = 자료만(sheet);

    let 상태: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, cells)| sheet.cell(cells, sheet.col("수강상태")).to_string())
        .collect();
    assert_eq!(상태, vec!["수강중", "수강취소"]);

    let 반: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, cells)| sheet.cell(cells, sheet.col("반")).to_string())
        .collect();
    assert_eq!(반, vec!["가람", "나리"]);

    let 합계: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, cells)| sheet.cell(cells, sheet.col("합계")).to_string())
        .collect();
    assert_eq!(합계, vec!["58000", "41500"], "취소자는 확정 금액");
}

#[test]
fn 징수내역_Excel에_0원_취소자도_들어간다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A", 원래_금액());
    let e = f.enroll(a, d);
    f.취소(e, Some(&전액면제()), "개강 전 취소");

    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "zero");
    let sheet = read::read_first_sheet(&path).unwrap();
    assert_eq!(sheet.rows.len(), 2, "0원이어도 넣는다 (자료 1줄 + 합계)");
    assert_eq!(sheet.cell(&sheet.rows[0].1, sheet.col("합계")), "0");
    assert_eq!(sheet.cell(&sheet.rows[0].1, sheet.col("수강상태")), "수강취소");
}

#[test]
fn 징수내역_Excel도_학생_우선_차례다() {
    let f = F::new();
    let d = f.dept("로봇과학", "A", vec![fee(강사료, 1_000)]);
    for (cls, no) in [("10", 1), ("2", 1), ("1", 1), ("가", 1)] {
        let s = f.student(1, cls, no, "아무개");
        f.enroll(s, d);
    }
    let path = 징수내역_파일(&f, &EnrollmentFilter::default(), "", "order");
    let sheet = 자료만(read::read_first_sheet(&path).unwrap());
    let 반: Vec<String> = sheet
        .rows
        .iter()
        .map(|(_, cells)| sheet.cell(cells, sheet.col("반")).to_string())
        .collect();
    assert_eq!(반, vec!["1", "2", "10", "가"], "숫자 반 자연정렬");
}
