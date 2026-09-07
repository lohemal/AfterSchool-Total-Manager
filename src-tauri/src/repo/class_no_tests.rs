#![allow(non_snake_case)]
//! 반이 문자여도 되는지 — DB 를 붙여 끝까지 밟아 본다 (최종 QA 요구사항).
//!
//! `domain::class_no` 는 규칙만 시험한다. 여기서는 **실제로 저장되고, 찾아지고,
//! 정산까지 이어지는지** 를 본다.

use crate::db::Db;
use crate::domain::class_no;
use crate::model::{EligibilityInput, StudentFilter, StudentInput};
use crate::repo;

fn student(grade: i64, class_no: &str, no: i64, name: &str) -> StudentInput {
    StudentInput {
        grade,
        class_no: class_no.to_string(),
        student_no: no,
        name: name.to_string(),
        note: None,
    }
}

fn 학년도() -> (Db, i64) {
    let db = Db::open_memory().unwrap();
    let year = db
        .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
        .unwrap();
    (db, year)
}

fn 반목록(db: &Db, year: i64) -> Vec<String> {
    db.read(|c| repo::student::list(c, year, &StudentFilter::default()))
        .unwrap()
        .into_iter()
        .map(|s| s.class_no)
        .collect()
}

#[test]
fn 숫자_반_학생을_넣을_수_있다() {
    let (db, year) = 학년도();
    let id = db
        .write(|c| repo::student::create(c, year, &student(3, "1", 5, "김하나")))
        .unwrap();
    assert!(id > 0);
    assert_eq!(반목록(&db, year), vec!["1"]);
}

#[test]
fn 가반_학생을_넣을_수_있다() {
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, "가", 1, "김하나")))
        .unwrap();
    assert_eq!(반목록(&db, year), vec!["가"]);
}

#[test]
fn 나반_학생을_넣을_수_있다() {
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, "나", 1, "이두리")))
        .unwrap();
    assert_eq!(반목록(&db, year), vec!["나"]);
}

#[test]
fn 해달별_같은_반_이름도_쓸_수_있다() {
    let (db, year) = 학년도();
    for (i, name) in ["해", "달", "별"].iter().enumerate() {
        db.write(|c| repo::student::create(c, year, &student(3, name, i as i64 + 1, "아무개")))
            .unwrap();
    }
    // 문자 반은 유니코드 차례 = 한글 차례
    assert_eq!(반목록(&db, year), vec!["달", "별", "해"]);
}

#[test]
fn 반이_비면_막는다() {
    let (db, year) = 학년도();
    let e = db
        .write(|c| repo::student::create(c, year, &student(3, "   ", 1, "김하나")))
        .unwrap_err();
    assert!(e.to_string().contains("반"), "{e}");
}

#[test]
fn 앞뒤_공백은_떼고_저장한다() {
    // 엑셀에서 온 '가 '와 손으로 친 '가'가 다른 반이 되면 안 된다.
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, " 가 ", 1, "김하나")))
        .unwrap();
    assert_eq!(반목록(&db, year), vec!["가"]);

    // 같은 반으로 보므로 같은 번호는 두 번 들어가지 않는다
    let e = db
        .write(|c| repo::student::create(c, year, &student(3, "가", 1, "다른사람")))
        .unwrap_err();
    assert!(e.to_string().contains("이미"), "{e}");
}

#[test]
fn 숫자_반은_1_2_3_10_차례로_나온다() {
    let (db, year) = 학년도();
    for (i, cls) in ["10", "2", "1", "3"].iter().enumerate() {
        db.write(|c| repo::student::create(c, year, &student(3, cls, i as i64 + 1, "아무개")))
            .unwrap();
    }
    assert_eq!(반목록(&db, year), vec!["1", "2", "3", "10"]);
}

#[test]
fn 숫자와_문자가_섞여도_차례가_늘_같다() {
    let (db, year) = 학년도();
    for (i, cls) in ["나", "10", "가", "2", "1"].iter().enumerate() {
        db.write(|c| repo::student::create(c, year, &student(3, cls, i as i64 + 1, "아무개")))
            .unwrap();
    }
    assert_eq!(반목록(&db, year), vec!["1", "2", "10", "가", "나"]);
}

#[test]
fn DB_정렬_열쇠가_domain_규칙과_같다() {
    // 두 곳이 어긋나면 화면과 엑셀의 차례가 달라진다. 값을 직접 맞대어 본다.
    let (db, year) = 학년도();
    let 보기 = ["1", "2", "10", "99", "100", "가", "나", "해", "달", "별", "1-A"];
    for (i, cls) in 보기.iter().enumerate() {
        db.write(|c| repo::student::create(c, year, &student(3, cls, i as i64 + 1, "아무개")))
            .unwrap();
    }
    db.read(|c| {
        let mut st = c.prepare("SELECT class_no, class_sort FROM student")?;
        let rows: Vec<(String, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (cls, sort) in rows {
            assert_eq!(
                sort,
                class_no::sort_key(&cls),
                "'{cls}' 의 정렬 열쇠가 DB 와 Rust 에서 다릅니다."
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn 한글_반_학생을_이름으로_찾을_수_있다() {
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, "가", 1, "김하나")))
        .unwrap();
    db.write(|c| repo::student::create(c, year, &student(3, "나", 1, "이두리")))
        .unwrap();

    let found = db
        .read(|c| {
            repo::student::list(
                c,
                year,
                &StudentFilter {
                    query: Some("하나".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].class_no, "가");
}

#[test]
fn 한글_반으로_거를_수_있다() {
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, "가", 1, "김하나")))
        .unwrap();
    db.write(|c| repo::student::create(c, year, &student(3, "나", 1, "이두리")))
        .unwrap();

    let only = db
        .read(|c| {
            repo::student::list(
                c,
                year,
                &StudentFilter {
                    class_no: Some("나".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].name, "이두리");

    // 걸러 낼 때도 앞뒤 공백은 무시한다
    let padded = db
        .read(|c| {
            repo::student::list(
                c,
                year,
                &StudentFilter {
                    class_no: Some(" 나 ".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(padded.len(), 1);
}

#[test]
fn 한글_반_학생을_학년반번호로_찾는다() {
    // 지원대상자·수강생 업로드가 학생을 맞추는 길이다.
    let (db, year) = 학년도();
    let id = db
        .write(|c| repo::student::create(c, year, &student(3, "가", 7, "김하나")))
        .unwrap();

    let found = db
        .read(|c| repo::student::find_by_key(c, year, 3, "가", 7))
        .unwrap();
    assert_eq!(found, Some((id, "김하나".to_string())));

    // 다른 반은 잡히지 않는다
    assert!(db
        .read(|c| repo::student::find_by_key(c, year, 3, "나", 7))
        .unwrap()
        .is_none());
}

#[test]
fn 한글_반_학생도_지원대상자로_이어진다() {
    let (db, year) = 학년도();
    let id = db
        .write(|c| repo::student::create(c, year, &student(3, "해", 1, "김하나")))
        .unwrap();
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

    let rows = db
        .read(|c| repo::eligibility::list(c, year, "VOUCHER", None))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].class_no, "해");
    assert_eq!(rows[0].name, "김하나");
}

#[test]
fn 숫자_반은_예전과_같은_글자로_보인다() {
    // 기존 사용자에게 뜻이 달라지면 안 된다. 1 은 계속 '1' 이다.
    let (db, year) = 학년도();
    db.write(|c| repo::student::create(c, year, &student(3, "1", 5, "김하나")))
        .unwrap();
    let rows = db
        .read(|c| repo::student::list(c, year, &StudentFilter::default()))
        .unwrap();
    assert_eq!(rows[0].class_no, "1");
    assert_eq!(
        repo::enrollment::student_label(rows[0].grade, &rows[0].class_no, rows[0].student_no, &rows[0].name),
        "3학년 1반 5번 김하나"
    );
}
