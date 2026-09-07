#![allow(non_snake_case)]
//! 한글 반이 Excel 을 오갈 때 그대로 남는지 (최종 QA 요구사항).
//!
//! 업로드 → 학생 매칭 → 내려받기까지 실제 파일로 밟는다.

use std::path::PathBuf;

use crate::db::Db;
use crate::excel::{self, write, Staged};
use crate::model::{
    DepartmentInput, EligibilityInput, Fee, StudentFilter, StudentInput, WorkspaceInput,
};
use crate::repo;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-class-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const STUDENT_HEAD: [&str; 5] = ["학년", "반", "번호", "이름", "비고"];

/// 한글 반 학생 셋이 있는 학년도와 작업공간.
fn 한글반_학교() -> (Db, i64, i64) {
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
                    name: "2026년 4월".into(),
                    start_date: "2026-04-01".into(),
                    end_date: "2026-04-30".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    for (cls, no, name) in [("가", 1, "김하나"), ("나", 1, "이두리"), ("해", 1, "박세찬")] {
        db.write(|c| {
            repo::student::create(
                c,
                year,
                &StudentInput {
                    grade: 3,
                    class_no: cls.into(),
                    student_no: no,
                    name: name.into(),
                    note: None,
                },
            )
        })
        .unwrap();
    }
    (db, year, ws)
}

#[test]
fn 한글_반_학생정보를_업로드할_수_있다() {
    let dir = tmp_dir("upload");
    let path = dir.join("학생.xlsx");
    write::write_sheet(
        &path,
        "학생정보",
        &STUDENT_HEAD,
        &[
            vec!["3".into(), "가".into(), "1".into(), "김하나".into(), "".into()],
            vec!["3".into(), "나".into(), "2".into(), "이두리".into(), "".into()],
            vec!["3".into(), "해".into(), "3".into(), "박세찬".into(), "".into()],
            // 숫자 반이 섞여 있어도 된다
            vec!["3".into(), "1".into(), "4".into(), "최네울".into(), "".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, staged) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.total, 4);
    assert_eq!(preview.ok_count, 4, "오류: {:?}", preview.errors);
    match staged {
        Staged::Students(rows) => {
            let 반: Vec<&str> = rows.iter().map(|r| r.class_no.as_str()).collect();
            assert_eq!(반, vec!["가", "나", "해", "1"]);
        }
        _ => panic!("학생 자료가 아님"),
    }
}

#[test]
fn 반이_빈_줄은_오류로_걸러진다() {
    let dir = tmp_dir("empty-class");
    let path = dir.join("학생.xlsx");
    write::write_sheet(
        &path,
        "학생정보",
        &STUDENT_HEAD,
        &[
            vec!["3".into(), "".into(), "1".into(), "김하나".into(), "".into()],
            vec!["3".into(), "가".into(), "2".into(), "이두리".into(), "".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, _) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.ok_count, 1, "정상 줄까지 버리면 안 된다");
    assert_eq!(preview.errors.len(), 1);
}

#[test]
fn 같은_한글_반_같은_번호는_두_번_올릴_수_없다() {
    let dir = tmp_dir("dup-class");
    let path = dir.join("학생.xlsx");
    write::write_sheet(
        &path,
        "학생정보",
        &STUDENT_HEAD,
        &[
            vec!["3".into(), "가".into(), "1".into(), "김하나".into(), "".into()],
            vec!["3".into(), "가".into(), "1".into(), "이두리".into(), "".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, _) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.ok_count, 1);
    assert_eq!(preview.errors.len(), 1);
    assert!(preview.errors[0].reason.contains("두 번"));
}

#[test]
fn 지원대상자_업로드가_한글_반_학생을_찾는다() {
    let (db, year, _) = 한글반_학교();
    let dir = tmp_dir("elig");
    let path = dir.join("대상자.xlsx");
    write::write_sheet(
        &path,
        "지원대상자",
        &["학년", "반", "번호", "이름", "적용 시작일", "적용 종료일", "비고"],
        &[
            vec!["3".into(), "가".into(), "1".into(), "김하나".into(), "".into(), "".into(), "".into()],
            vec!["3".into(), "해".into(), "1".into(), "박세찬".into(), "".into(), "".into(), "".into()],
            // 없는 반 — 오류가 나야 한다
            vec!["3".into(), "달".into(), "1".into(), "없는이".into(), "".into(), "".into(), "".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, staged) = db
        .read(|c| excel::preview_eligibility(c, year, "VOUCHER", &path))
        .unwrap();
    assert_eq!(preview.ok_count, 2, "오류: {:?}", preview.errors);
    assert_eq!(preview.errors.len(), 1);
    assert!(preview.errors[0].reason.contains("달반"), "{:?}", preview.errors[0]);
    match staged {
        Staged::Eligibility { rows, .. } => assert_eq!(rows.len(), 2),
        _ => panic!("지원대상자 자료가 아님"),
    }
}

#[test]
fn 수강생_업로드가_한글_반_학생을_찾는다() {
    let (db, year, ws) = 한글반_학교();
    db.write(|c| {
        repo::department::create(
            c,
            ws,
            &DepartmentInput {
                name: "로봇과학".into(),
                class_name: Some("A반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![Fee { item_code: "INSTRUCTOR".into(), amount: 40_000 }],
            },
        )
    })
    .unwrap();

    let dir = tmp_dir("enroll");
    let path = dir.join("수강.xlsx");
    write::write_sheet(
        &path,
        "수강정보",
        &["부서명", "반명", "학년", "반", "번호", "이름"],
        &[
            vec!["로봇과학".into(), "A반".into(), "3".into(), "가".into(), "1".into(), "김하나".into()],
            vec!["로봇과학".into(), "A반".into(), "3".into(), "나".into(), "1".into(), "이두리".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, _) = db
        .read(|c| excel::preview_enrollments(c, year, ws, &path))
        .unwrap();
    assert_eq!(preview.ok_count, 2, "오류: {:?}", preview.errors);
    assert!(preview.errors.is_empty());
}

#[test]
fn 내려받은_파일에_한글_반이_그대로_있다() {
    let (db, year, _) = 한글반_학교();
    let dir = tmp_dir("export");
    let made = db
        .read(|c| excel::export_students(c, year, &StudentFilter::default(), &[], &dir))
        .unwrap();
    let path = PathBuf::from(&made.path);
    assert_eq!(made.rows, 3);

    // 내려받은 파일을 그대로 다시 읽어 본다 — 반이 살아 있어야 한다.
    let (preview, staged) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.ok_count, 3, "오류: {:?}", preview.errors);
    match staged {
        Staged::Students(rows) => {
            let 반: Vec<&str> = rows.iter().map(|r| r.class_no.as_str()).collect();
            // 내려받기는 목록 차례를 따른다 — 문자 반은 한글 차례
            assert_eq!(반, vec!["가", "나", "해"]);
        }
        _ => panic!("학생 자료가 아님"),
    }
}

#[test]
fn 지원대상자_내려받기에도_한글_반이_남는다() {
    let (db, year, _) = 한글반_학교();
    let id = db
        .read(|c| repo::student::find_by_key(c, year, 3, "해", 1))
        .unwrap()
        .unwrap()
        .0;
    db.write(|c| {
        repo::eligibility::create(
            c,
            year,
            &EligibilityInput {
                student_id: id,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();

    let dir = tmp_dir("export-elig");
    let made = db
        .read(|c| excel::export_eligibility(c, year, "VOUCHER", &[], &dir))
        .unwrap();
    assert_eq!(made.rows, 1);

    let sheet = excel::read::read_first_sheet(&PathBuf::from(&made.path)).unwrap();
    let c_class = sheet.require("반").unwrap();
    let (_, cells) = &sheet.rows[0];
    assert_eq!(sheet.cell(cells, Some(c_class)), "해");
}
