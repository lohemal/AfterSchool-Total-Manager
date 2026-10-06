//! 부서별 수강현황 Excel 시험 (v0.1.7).
//!
//! 지키려는 것 둘.
//!
//! 1. **파일의 숫자가 화면의 숫자와 같다** — Excel 쪽에서 다시 셈하지 않으므로
//!    집계 서비스가 낸 값이 그대로 적혀 있어야 한다.
//! 2. **화면 필터가 파일을 깎지 않는다** — 공식 통계에서 반이 조용히 빠지면
//!    결재 자료가 틀린다.

use std::path::PathBuf;

use crate::db::Db;
use crate::excel::{self, read};
use crate::model::{CostItem, DepartmentInput, EnrollmentInput, Fee, StudentInput, WorkspaceInput};
use crate::repo;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-capacity-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
                        name: "2026년 3월".into(),
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
                        teacher: Some("홍길동".into()),
                        days: Some(days.into()),
                        note: None,
                        fees: vec![Fee {
                            item_code: "INSTRUCTOR".into(),
                            amount: 30_000,
                        }],
                        capacity: cap,
                        allowed_grades: grades.to_vec(),
                    },
                )
            })
            .unwrap()
    }

    fn fill(&self, dept: i64, n: i64, grade: i64) {
        for i in 1..=n {
            let s = self
                .db
                .write(|c| {
                    repo::student::create(
                        c,
                        self.year,
                        &StudentInput {
                            grade,
                            class_no: "1".into(),
                            student_no: i,
                            name: format!("학생{i}"),
                            note: None,
                        },
                    )
                })
                .unwrap();
            self.db
                .write(|c| {
                    repo::enrollment::create(
                        c,
                        self.ws,
                        &EnrollmentInput {
                            student_id: s,
                            department_id: dept,
                            fees: Vec::new(),
                            reason: None,
                        },
                        &self.items,
                    )
                })
                .unwrap();
        }
    }
}

#[test]
fn 세_장이_화면과_같은_숫자를_담는다() {
    let f = F::new();
    let a = f.dept("로봇과학", "A반", "월,수", Some(20), &[1, 2, 4]);
    f.dept("미술", "B반", "화", None, &[]);
    f.fill(a, 17, 1);

    let dir = tmp_dir("sheets");
    let out = f
        .db
        .read(|c| excel::export_capacity(c, f.ws, &["2026학년도", "2026년 3월"], &dir))
        .unwrap();
    let path = PathBuf::from(&out.path);
    assert!(path.exists(), "파일이 만들어지지 않았다");
    assert!(
        out.name.contains("부서별 수강현황"),
        "파일 이름이 다르다: {}",
        out.name
    );
    // `export_path` 가 이름에서 공백을 뺀다 — 파일 이름 규칙은 기존 그대로다.
    assert!(out.name.contains("2026년3월"), "작업공간 이름이 빠졌다: {}", out.name);

    // 화면이 쓰는 것과 같은 집계
    let stats = f.db.read(|c| repo::capacity::stats(c, f.ws)).unwrap();

    // ── 1장 부서별 현황
    let s1 = read::read_sheet_at(&path, 0).unwrap();
    let c_name = s1.require("부서명").unwrap();
    let c_grade = s1.require("대상 학년").unwrap();
    let c_cap = s1.require("정원").unwrap();
    let c_now = s1.require("현재 수강").unwrap();
    let c_left = s1.require("남은 자리").unwrap();
    let c_rate = s1.require("충원율").unwrap();
    let c_state = s1.require("상태").unwrap();

    let 로봇 = s1
        .rows
        .iter()
        .find(|(_, cells)| cells.get(c_name).map(|v| v.as_str()) == Some("로봇과학"))
        .expect("로봇과학 줄이 없다")
        .1
        .clone();
    assert_eq!(로봇[c_grade], "1·2·4학년", "띄엄띄엄 학년이 범위로 뭉개졌다");
    assert_eq!(로봇[c_cap], "20");
    assert_eq!(로봇[c_now], "17");
    assert_eq!(로봇[c_left], "3");
    assert_eq!(로봇[c_rate], "85.0%");
    assert_eq!(로봇[c_state], "모집 가능");

    // 정원 미설정 반도 빠지지 않고, 0 으로 둔갑하지도 않는다
    let 미술 = s1
        .rows
        .iter()
        .find(|(_, cells)| cells.get(c_name).map(|v| v.as_str()) == Some("미술"))
        .expect("정원 미설정 반이 파일에서 빠졌다")
        .1
        .clone();
    assert_eq!(미술[c_cap], "-");
    assert_eq!(미술[c_left], "-");
    assert_eq!(미술[c_rate], "-");
    assert_eq!(미술[c_state], "정원 미설정");
    assert_eq!(미술[c_grade], "미설정");

    assert_eq!(s1.rows.len(), stats.rows.len(), "화면과 줄 수가 다르다");

    // ── 2장 학년별 현황
    let s2 = read::read_sheet_at(&path, 1).unwrap();
    let g_grade = s2.require("학년").unwrap();
    let g_students = s2.require("수강 학생 수").unwrap();
    let g_rate = s2.require("참여율").unwrap();
    let 일학년 = s2
        .rows
        .iter()
        .find(|(_, c)| c.get(g_grade).map(|v| v.as_str()) == Some("1학년"))
        .expect("1학년 줄이 없다")
        .1
        .clone();
    assert_eq!(일학년[g_students], "17");
    // 1학년 학생 17명이 모두 듣는다
    assert_eq!(일학년[g_rate], "100.0%");

    // ── 3장 요일별 현황
    let s3 = read::read_sheet_at(&path, 2).unwrap();
    let w_day = s3.require("요일").unwrap();
    let w_enr = s3.require("수강 건수").unwrap();
    let day = |d: &str| {
        s3.rows
            .iter()
            .find(|(_, c)| c.get(w_day).map(|v| v.as_str()) == Some(d))
            .unwrap_or_else(|| panic!("{d} 줄이 없다"))
            .1
            .clone()
    };
    // 월,수 반은 두 요일에 모두 센다
    assert_eq!(day("월")[w_enr], "17");
    assert_eq!(day("수")[w_enr], "17");
    assert_eq!(day("화")[w_enr], "0");

    // 세로 합계가 전체와 다른 까닭이 파일 안에 적혀 있어야 한다
    let 꼬리 = s3
        .rows
        .iter()
        .any(|(_, c)| c.first().map(|v| v.contains("각 요일에 모두 셉니다")) == Some(true));
    assert!(꼬리, "복수 요일 설명이 파일에 없다");
}

#[test]
fn 작업공간_전체를_낸다() {
    let f = F::new();
    for (i, day) in ["월", "화", "수", "목"].iter().enumerate() {
        let d = f.dept(&format!("부서{i}"), "A반", day, Some(10), &[1]);
        f.fill(d, 1, (i as i64) + 1);
    }

    let dir = tmp_dir("all");
    let out = f
        .db
        .read(|c| excel::export_capacity(c, f.ws, &["2026학년도", "2026년 3월"], &dir))
        .unwrap();
    assert_eq!(out.rows, 4, "일부 반이 빠졌다");

    let s1 = read::read_sheet_at(&PathBuf::from(&out.path), 0).unwrap();
    assert_eq!(s1.rows.len(), 4);
}
