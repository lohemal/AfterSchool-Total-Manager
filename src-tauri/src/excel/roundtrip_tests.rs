//! Excel 왕복 점검 — 우리가 만든 파일을 우리가 다시 읽을 수 있어야 한다.
//!
//! 양식을 만들고(rust_xlsxwriter) 그 파일을 그대로 읽어(calamine) 검사까지 돌린다.
//! 헤더 이름이 어긋나면 여기서 바로 걸린다.

use std::path::PathBuf;

use crate::excel::{self, write, Staged};
use crate::model::CostItem;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn items() -> Vec<CostItem> {
    [("INSTRUCTOR", "강사료"), ("OPERATION", "수용비"), ("TEXTBOOK", "교재비"), ("MATERIAL", "재료비")]
        .iter()
        .enumerate()
        .map(|(i, (code, name))| CostItem {
            code: code.to_string(),
            name: name.to_string(),
            sort_order: i as i64 + 1,
        })
        .collect()
}

#[test]
fn 학생정보_양식을_만들고_그대로_읽는다() {
    let dir = tmp_dir("student-template");
    let made = excel::template("students", &items(), &dir).unwrap();
    let path = PathBuf::from(&made.path);
    assert!(path.exists());

    // 예시 한 줄이 들어 있고, 그 줄은 정상으로 읽힌다
    let (preview, staged) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.total, 1);
    assert_eq!(preview.ok_count, 1);
    assert!(preview.errors.is_empty());
    match staged {
        Staged::Students(rows) => {
            assert_eq!(rows[0].grade, 3);
            assert_eq!(rows[0].name, "홍길동");
        }
        _ => panic!("학생 자료가 아님"),
    }
}

#[test]
fn 잘못된_줄_때문에_정상_줄을_버리지_않는다() {
    let dir = tmp_dir("student-mixed");
    let path = dir.join("학생.xlsx");
    write::write_sheet(
        &path,
        "학생정보",
        &["학년", "반", "번호", "이름", "비고"],
        &[
            vec!["3".into(), "1".into(), "1".into(), "김하나".into(), "".into()],
            vec!["3".into(), "1".into(), "2".into(), "".into(), "".into()], // 이름 없음
            vec!["세".into(), "1".into(), "3".into(), "박세찌".into(), "".into()], // 숫자 아님
            vec!["3".into(), "1".into(), "1".into(), "중복".into(), "".into()], // 파일 안 중복
            vec!["3".into(), "1".into(), "4".into(), "최네찌".into(), "메모".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, staged) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.total, 5);
    assert_eq!(preview.ok_count, 2, "정상 두 줄은 살아남아야 한다");
    assert_eq!(preview.errors.len(), 3);
    // 사유가 사람이 읽을 수 있는 문장인지
    assert!(preview.errors.iter().any(|e| e.reason.contains("이름이 비어")));
    assert!(preview.errors.iter().any(|e| e.reason.contains("숫자")));
    assert!(preview.errors.iter().any(|e| e.reason.contains("두 번")));
    // 엑셀 화면의 행 번호 그대로인지 (헤더가 1행)
    assert_eq!(preview.errors[0].row, 3);

    match staged {
        Staged::Students(rows) => {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1].note, "메모");
        }
        _ => panic!("학생 자료가 아님"),
    }
}

#[test]
fn 부서_양식은_비용항목_목록에서_열을_만든다() {
    let dir = tmp_dir("dept-template");
    let made = excel::template("departments", &items(), &dir).unwrap();
    let path = PathBuf::from(&made.path);

    let (preview, staged) = excel::preview_departments(&items(), &path).unwrap();
    assert_eq!(preview.headers.len(), 8, "부서명·반명·강사명·요일 + 항목 4개");
    assert_eq!(preview.ok_count, 1);
    match staged {
        Staged::Departments(rows) => {
            assert_eq!(rows[0].name, "로봇과학");
            assert_eq!(rows[0].fees.len(), 4);
        }
        _ => panic!("부서 자료가 아님"),
    }
}

#[test]
fn 금액이_쉼표나_문자여도_알맞게_처리한다() {
    let dir = tmp_dir("dept-amount");
    let path = dir.join("부서.xlsx");
    write::write_sheet(
        &path,
        "부서정보",
        &["부서명", "반명", "강사명", "요일", "강사료", "수용비", "교재비", "재료비"],
        &[
            vec!["로봇과학".into(), "A반".into(), "김".into(), "월".into(),
                 "40,000".into(), "".into(), "30000".into(), "0".into()],
            vec!["미술".into(), "A반".into(), "이".into(), "화".into(),
                 "이만원".into(), "0".into(), "0".into(), "0".into()],
        ],
        &[],
        None,
    )
    .unwrap();

    let (preview, staged) = excel::preview_departments(&items(), &path).unwrap();
    assert_eq!(preview.ok_count, 1);
    assert_eq!(preview.errors.len(), 1);
    assert!(preview.errors[0].reason.contains("숫자로 읽지"));

    match staged {
        Staged::Departments(rows) => {
            let fee = |code: &str| rows[0].fees.iter().find(|f| f.item_code == code).unwrap().amount;
            assert_eq!(fee("INSTRUCTOR"), 40_000, "쉼표가 있어도 읽는다");
            assert_eq!(fee("OPERATION"), 0, "빈 칸은 0원");
            assert_eq!(fee("TEXTBOOK"), 30_000);
        }
        _ => panic!("부서 자료가 아님"),
    }
}

#[test]
fn 열_이름에_공백이나_괄호가_있어도_찾는다() {
    let dir = tmp_dir("header-loose");
    let path = dir.join("학생.xlsx");
    write::write_sheet(
        &path,
        "학생정보",
        &["학 년", "반(필수)", " 번호 ", "이름"],
        &[vec!["3".into(), "1".into(), "1".into(), "김하나".into()]],
        &[],
        None,
    )
    .unwrap();

    let (preview, _) = excel::preview_students(&path).unwrap();
    assert_eq!(preview.ok_count, 1);
    assert!(preview.errors.is_empty());
}

#[test]
fn 필요한_열이_없으면_무엇이_없는지_알려_준다() {
    let dir = tmp_dir("header-missing");
    let path = dir.join("학생.xlsx");
    write::write_sheet(&path, "학생정보", &["학년", "반", "이름"], &[], &[], None).unwrap();

    let err = excel::preview_students(&path).unwrap_err();
    assert!(err.message.contains("번호"), "빠진 열 이름을 알려 줘야 한다: {}", err.message);
    assert!(err.message.contains("업로드 양식 받기"));
}
