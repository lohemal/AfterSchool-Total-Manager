#![allow(non_snake_case)]
//! 반 이름 규칙 시험.

use super::*;

fn 정렬(mut v: Vec<&str>) -> Vec<&str> {
    v.sort_by(|a, b| cmp(a, b));
    v
}

#[test]
fn 숫자_반은_자연정렬된다() {
    // 사전순이면 1, 10, 2, 3 이 된다. 그게 아니어야 한다.
    assert_eq!(정렬(vec!["10", "2", "1", "3"]), vec!["1", "2", "3", "10"]);
}

#[test]
fn 숫자_반은_두_자리를_넘어도_자연정렬된다() {
    assert_eq!(
        정렬(vec!["100", "9", "20", "1"]),
        vec!["1", "9", "20", "100"]
    );
}

#[test]
fn 문자_반은_한글_차례로_정렬된다() {
    assert_eq!(정렬(vec!["다", "가", "나"]), vec!["가", "나", "다"]);
    assert_eq!(정렬(vec!["해", "달", "별"]), vec!["달", "별", "해"]);
}

#[test]
fn 숫자와_문자가_섞이면_숫자가_먼저다() {
    assert_eq!(
        정렬(vec!["가", "10", "나", "2", "1"]),
        vec!["1", "2", "10", "가", "나"]
    );
}

#[test]
fn 섞여_있어도_차례가_늘_하나로_정해진다() {
    // 어떤 차례로 넣어도 결과가 같아야 한다.
    let 기대 = vec!["1", "2", "10", "가", "나", "달", "별", "해"];
    assert_eq!(정렬(vec!["해", "1", "나", "10", "달", "2", "별", "가"]), 기대);
    assert_eq!(정렬(vec!["가", "별", "2", "달", "10", "해", "나", "1"]), 기대);
    assert_eq!(정렬(기대.clone()), 기대);
}

#[test]
fn 앞자리가_0인_반도_숫자로_본다() {
    // '01' 과 '1' 은 열쇠가 같지만 서로 다른 반이므로 차례가 흔들리면 안 된다.
    assert_eq!(정렬(vec!["1", "01"]), vec!["01", "1"]);
    assert_eq!(정렬(vec!["01", "1"]), vec!["01", "1"]);
}

#[test]
fn 앞뒤_공백은_떼어_낸다() {
    // 엑셀에서 온 '1 ' 과 손으로 친 '1' 이 다른 반이 되면 안 된다.
    assert_eq!(normalize(" 1 "), "1");
    assert_eq!(normalize("\t가\n"), "가");
    assert_eq!(check(" 나 ").unwrap(), "나");
}

#[test]
fn 가운데_글자는_손대지_않는다() {
    // 학교가 쓰는 이름을 프로그램이 고쳐서는 안 된다.
    assert_eq!(normalize("가 나"), "가 나");
    assert_eq!(check("1-A").unwrap(), "1-A");
}

#[test]
fn 빈_반은_막는다() {
    assert!(check("").is_err());
    assert!(check("   ").is_err());
}

#[test]
fn 너무_긴_반은_막는다() {
    assert!(check(&"가".repeat(MAX_LEN)).is_ok());
    assert!(check(&"가".repeat(MAX_LEN + 1)).is_err());
    // 글자 수로 세야 한다. 한글은 한 글자가 3바이트라 바이트로 세면 잘못 막는다.
    assert!(check("가나다라마바사아자차").is_ok());
}

#[test]
fn 정렬_열쇠_모양() {
    assert_eq!(sort_key("1"), "0000001");
    assert_eq!(sort_key("10"), "0000010");
    assert_eq!(sort_key("가"), "1가");
}
