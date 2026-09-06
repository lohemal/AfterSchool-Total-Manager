#![allow(non_snake_case)] // 시험 이름에 재원 코드를 그대로 쓴다 (origin_규칙_..._PLAIN)

//! 정산 엔진 경계값 시험.
//!
//! 금액은 **1원 단위**로 민다. 회계에서 1원이 틀리면 그것으로 끝이기 때문이다.
//! 모든 시험은 끝에 `verify`로 `Σ 배분액 == charge.amount`를 확인한다.

use super::*;
use crate::domain::{Fund, Origin, Program};

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

/// 강사료 > 수용비 > 교재비 > 재료비, 부서는 넘겨받은 차례대로.
fn cfg(dept_order: &[i64]) -> Config {
    Config::new(
        vec![Program::Voucher, Program::FreeVoucher],
        dept_order,
        &[
            강사료.to_string(),
            수용비.to_string(),
            교재비.to_string(),
            재료비.to_string(),
        ],
    )
}

fn charge(enrollment_id: i64, department_id: i64, item: &str, amount: i64) -> ChargeUnit {
    ChargeUnit {
        enrollment_id,
        department_id,
        item_code: item.to_string(),
        amount,
    }
}

fn student(charges: Vec<ChargeUnit>, voucher: Budget, free: Budget) -> StudentInput {
    StudentInput {
        student_id: 1,
        charges,
        voucher,
        free_voucher: free,
    }
}

/// 재원별 합계.
fn by_fund(r: &StudentResult, fund: Fund) -> i64 {
    r.allocs
        .iter()
        .filter(|a| a.fund == fund)
        .map(|a| a.amount)
        .sum()
}

/// 한 칸(수강×항목)의 재원별 배분.
fn cell(r: &StudentResult, enrollment_id: i64, item: &str) -> Vec<(Fund, i64)> {
    r.allocs
        .iter()
        .filter(|a| a.enrollment_id == enrollment_id && a.item_code == item)
        .map(|a| (a.fund, a.amount))
        .collect()
}

/// 매번 부르는 불변식 검사.
fn ok(input: &StudentInput, r: &StudentResult) {
    verify(&input.charges, &r.allocs).expect("불변식이 깨졌습니다");
}

// ─────────────────────────────────────────────── 기본 차감

#[test]
fn 이용권이_넉넉하면_전액_지원된다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000), charge(1, 10, 교재비, 30_000)],
        Budget::of(100_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::Voucher), 70_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 0);
    assert_eq!(r.used.voucher, 70_000);
}

#[test]
fn 자격이_없으면_전액_일반_수익자다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::none(),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::SelfPay), 40_000);
    assert_eq!(r.allocs[0].origin, Origin::Plain);
}

#[test]
fn 영원짜리_항목은_배분을_만들지_않는다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000), charge(1, 10, 재료비, 0)],
        Budget::of(100_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(r.allocs.len(), 1);
    assert!(cell(&r, 1, 재료비).is_empty());
}

// ─────────────────────────────────────────────── 1원 단위 경계

#[test]
fn 한_항목_중간에서_끊기면_그_항목이_쪼개진다() {
    // 남은 이용권 20,000 / 해당 charge 35,000 → 20,000 + 15,000
    let input = student(
        vec![charge(1, 10, 강사료, 35_000)],
        Budget::of(20_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    let c = cell(&r, 1, 강사료);
    assert_eq!(c.len(), 2, "한 칸이 두 재원으로 쪼개져야 한다");
    assert!(c.contains(&(Fund::Voucher, 20_000)));
    assert!(c.contains(&(Fund::VoucherOver, 15_000)));
}

#[test]
fn 가용액이_금액과_정확히_같으면_초과금이_0이다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::of(40_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::Voucher), 40_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 0);
    assert_eq!(r.allocs.len(), 1, "쪼갤 필요가 없으면 한 줄만 남는다");
}

#[test]
fn 가용액이_1원_모자라면_초과금이_정확히_1원이다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::of(39_999),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::Voucher), 39_999);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 1);
}

#[test]
fn 가용액이_1원_남으면_그_1원까지_쓴다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::of(40_001),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::Voucher), 40_000, "남는 돈을 억지로 쓰지 않는다");
    assert_eq!(r.used.voucher, 40_000);
}

#[test]
fn 가용액_1원으로도_1원을_지원한다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::of(1),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::Voucher), 1);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 39_999);
}

#[test]
fn 가용액이_0이면_자격이_있어도_전액_초과금이다() {
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget::of(0),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 40_000);
    assert_eq!(by_fund(&r, Fund::SelfPay), 0, "자격이 있으면 일반 수익자가 아니다");
    assert_eq!(r.allocs[0].origin, Origin::VoucherExhausted);
}

#[test]
fn 음수_가용액이_들어와도_0으로_다룬다() {
    // 한도를 사후에 낮춘 경우 등 — 죽지 않고 전액 초과금이 된다
    let input = student(
        vec![charge(1, 10, 강사료, 40_000)],
        Budget {
            eligible: true,
            available: -5_000,
        },
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 40_000);
    assert_eq!(r.used.voucher, 0);
}

// ─────────────────────────────────────────────── 우선순위

#[test]
fn 부서_우선순위_차례대로_차감한다() {
    // 로봇과학(10) > 미술(20) > 축구(30), 이용권 100,000
    let input = student(
        vec![
            charge(3, 30, 강사료, 20_000),
            charge(1, 10, 강사료, 40_000),
            charge(2, 20, 강사료, 25_000),
            charge(1, 10, 교재비, 30_000),
            charge(2, 20, 재료비, 15_000),
        ],
        Budget::of(100_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10, 20, 30]));
    ok(&input, &r);

    // 설계안 6장의 표 그대로
    assert_eq!(cell(&r, 1, 강사료), vec![(Fund::Voucher, 40_000)]);
    assert_eq!(cell(&r, 1, 교재비), vec![(Fund::Voucher, 30_000)]);
    assert_eq!(cell(&r, 2, 강사료), vec![(Fund::Voucher, 25_000)]);
    let 미술재료비 = cell(&r, 2, 재료비);
    assert!(미술재료비.contains(&(Fund::Voucher, 5_000)));
    assert!(미술재료비.contains(&(Fund::VoucherOver, 10_000)));
    assert_eq!(cell(&r, 3, 강사료), vec![(Fund::VoucherOver, 20_000)]);

    assert_eq!(by_fund(&r, Fund::Voucher), 100_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 30_000);
}

#[test]
fn 부서_순서를_뒤집으면_분포만_바뀌고_총액은_같다() {
    let charges = vec![
        charge(1, 10, 강사료, 40_000),
        charge(2, 20, 강사료, 25_000),
    ];
    let a = settle_student(
        &student(charges.clone(), Budget::of(30_000), Budget::none()),
        &cfg(&[10, 20]),
    );
    let b = settle_student(
        &student(charges.clone(), Budget::of(30_000), Budget::none()),
        &cfg(&[20, 10]),
    );

    assert_eq!(by_fund(&a, Fund::Voucher), 30_000);
    assert_eq!(by_fund(&b, Fund::Voucher), 30_000);

    // 앞선 부서가 먼저 지원받는다.
    // a: 로봇과학(40,000)이 30,000을 받고 10,000이 남는다 → 한 칸이 쪼개진다
    assert!(cell(&a, 1, 강사료).contains(&(Fund::Voucher, 30_000)));
    assert!(cell(&a, 1, 강사료).contains(&(Fund::VoucherOver, 10_000)));
    assert_eq!(cell(&a, 2, 강사료), vec![(Fund::VoucherOver, 25_000)]);

    // b: 미술(25,000)이 먼저 전액을 받고 남은 5,000이 로봇과학으로 간다
    assert_eq!(cell(&b, 2, 강사료), vec![(Fund::Voucher, 25_000)]);
    assert!(cell(&b, 1, 강사료).contains(&(Fund::Voucher, 5_000)));
    assert!(cell(&b, 1, 강사료).contains(&(Fund::VoucherOver, 35_000)));
}

#[test]
fn 비용항목_우선순위_차례대로_차감한다() {
    let input = student(
        vec![
            charge(1, 10, 재료비, 10_000),
            charge(1, 10, 강사료, 40_000),
            charge(1, 10, 교재비, 30_000),
            charge(1, 10, 수용비, 5_000),
        ],
        Budget::of(50_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    // 강사료 40,000 → 수용비 5,000 → 교재비 5,000까지
    assert_eq!(cell(&r, 1, 강사료), vec![(Fund::Voucher, 40_000)]);
    assert_eq!(cell(&r, 1, 수용비), vec![(Fund::Voucher, 5_000)]);
    let 교재 = cell(&r, 1, 교재비);
    assert!(교재.contains(&(Fund::Voucher, 5_000)));
    assert!(교재.contains(&(Fund::VoucherOver, 25_000)));
    assert_eq!(cell(&r, 1, 재료비), vec![(Fund::VoucherOver, 10_000)]);
}

#[test]
fn 항목_순서를_바꾸면_그대로_따른다() {
    let cfg = Config::new(
        vec![Program::Voucher, Program::FreeVoucher],
        &[10],
        &[재료비.to_string(), 강사료.to_string()],
    );
    let input = student(
        vec![charge(1, 10, 강사료, 40_000), charge(1, 10, 재료비, 10_000)],
        Budget::of(10_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg);
    ok(&input, &r);
    assert_eq!(cell(&r, 1, 재료비), vec![(Fund::Voucher, 10_000)]);
    assert_eq!(cell(&r, 1, 강사료), vec![(Fund::VoucherOver, 40_000)]);
}

#[test]
fn 우선순위_목록에_없는_부서는_맨_뒤로_간다() {
    // 부서 99는 목록에 없다
    let input = student(
        vec![charge(2, 99, 강사료, 30_000), charge(1, 10, 강사료, 40_000)],
        Budget::of(40_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(cell(&r, 1, 강사료), vec![(Fund::Voucher, 40_000)]);
    assert_eq!(cell(&r, 2, 강사료), vec![(Fund::VoucherOver, 30_000)]);
}

#[test]
fn 같은_금액이어도_정렬이_흔들리지_않는다() {
    // 우선순위 목록이 비어 부서 순위가 모두 같다 → 부서id, 항목코드로 갈린다
    let charges = vec![
        charge(3, 30, 강사료, 10_000),
        charge(1, 10, 강사료, 10_000),
        charge(2, 20, 강사료, 10_000),
    ];
    let empty = Config::new(vec![Program::Voucher], &[], &[]);

    let first = settle_student(&student(charges.clone(), Budget::of(10_000), Budget::none()), &empty);
    for _ in 0..20 {
        let again =
            settle_student(&student(charges.clone(), Budget::of(10_000), Budget::none()), &empty);
        assert_eq!(first.allocs, again.allocs, "같은 입력은 언제나 같은 출력");
    }
    // 부서 id가 가장 작은 10번이 먼저 지원받는다
    assert_eq!(cell(&first, 1, 강사료), vec![(Fund::Voucher, 10_000)]);
}

// ─────────────────────────────────────────────── 두 제도

#[test]
fn 이용권_소진_뒤_자유수강권이_이어받는다() {
    // 요구사항 §10의 예시: charge 50,000 / 이용권 20,000 / 자유수강권 25,000
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::of(20_000),
        Budget::of(25_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::Voucher), 20_000);
    assert_eq!(by_fund(&r, Fund::FreeVoucher), 25_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 5_000, "남은 5,000원");
    assert_eq!(by_fund(&r, Fund::SelfPay), 0);
}

#[test]
fn 자유수강권만_있으면_남은_금액은_일반_수익자다() {
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::none(),
        Budget::of(20_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::FreeVoucher), 20_000);
    assert_eq!(by_fund(&r, Fund::SelfPay), 30_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 0, "이용권 자격이 없으면 초과금이 아니다");
}

#[test]
fn 두_제도를_모두_써도_남으면_이용권_초과금이_된다() {
    let input = student(
        vec![charge(1, 10, 강사료, 200_000)],
        Budget::of(70_000),
        Budget::of(60_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::Voucher), 70_000);
    assert_eq!(by_fund(&r, Fund::FreeVoucher), 60_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 70_000);
    assert_eq!(70_000 + 60_000 + 70_000, 200_000);
}

#[test]
fn 이용권이_0이어도_자유수강권을_건너뛰지_않는다() {
    // "이용권이 소진되었다고 곧바로 학부모 부담으로 확정하지 않는다"
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::of(0),
        Budget::of(50_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::FreeVoucher), 50_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 0);
}

#[test]
fn 제도_순서를_바꾸면_자유수강권이_먼저_쓰인다() {
    let cfg = Config::new(
        vec![Program::FreeVoucher, Program::Voucher],
        &[10],
        &[강사료.to_string()],
    );
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::of(20_000),
        Budget::of(25_000),
    );
    let r = settle_student(&input, &cfg);
    ok(&input, &r);

    assert_eq!(by_fund(&r, Fund::FreeVoucher), 25_000);
    assert_eq!(by_fund(&r, Fund::Voucher), 20_000);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 5_000);
}

// ─────────────────────────────────────────────── origin 규칙

/// origin은 **그 학생이 자격을 가진 제도 가운데 마지막으로 거친 것**이다.
/// 잔액이 0이어도 자격이 있으면 거친 것으로 본다.
#[test]
fn origin_규칙_자격이_없으면_PLAIN() {
    let input = student(
        vec![charge(1, 10, 강사료, 10_000)],
        Budget::none(),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    assert_eq!(r.allocs[0].origin, Origin::Plain);
    assert_eq!(r.allocs[0].fund, Fund::SelfPay);
}

#[test]
fn origin_규칙_이용권만_있으면_VOUCHER_EXHAUSTED() {
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::of(20_000),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    let over: Vec<&Alloc> = r.allocs.iter().filter(|a| a.fund == Fund::VoucherOver).collect();
    assert_eq!(over.len(), 1);
    assert_eq!(over[0].origin, Origin::VoucherExhausted);
}

#[test]
fn origin_규칙_자유수강권만_있으면_FREE_EXHAUSTED() {
    let input = student(
        vec![charge(1, 10, 강사료, 50_000)],
        Budget::none(),
        Budget::of(20_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    let rest: Vec<&Alloc> = r.allocs.iter().filter(|a| a.fund == Fund::SelfPay).collect();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].origin, Origin::FreeExhausted);
}

#[test]
fn origin_규칙_둘_다_있으면_마지막_제도인_FREE_EXHAUSTED() {
    let input = student(
        vec![charge(1, 10, 강사료, 200_000)],
        Budget::of(70_000),
        Budget::of(60_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    let over: Vec<&Alloc> = r.allocs.iter().filter(|a| a.fund == Fund::VoucherOver).collect();
    assert_eq!(over.len(), 1);
    assert_eq!(
        over[0].origin,
        Origin::FreeExhausted,
        "재원은 이용권 초과금, 사연은 자유수강권까지 소진"
    );
}

#[test]
fn origin_규칙_지원받은_금액은_언제나_PLAIN() {
    let input = student(
        vec![charge(1, 10, 강사료, 200_000)],
        Budget::of(70_000),
        Budget::of(60_000),
    );
    let r = settle_student(&input, &cfg(&[10]));
    for a in r.allocs.iter().filter(|a| {
        a.fund == Fund::Voucher || a.fund == Fund::FreeVoucher
    }) {
        assert_eq!(a.origin, Origin::Plain, "지원액에는 사연이 없다");
    }
}

// ─────────────────────────────────────────────── 불변식

#[test]
fn 여러_부서_여러_항목에서도_1원도_새지_않는다() {
    let charges = vec![
        charge(1, 10, 강사료, 41_111),
        charge(1, 10, 수용비, 3_333),
        charge(1, 10, 교재비, 27_777),
        charge(2, 20, 강사료, 25_555),
        charge(2, 20, 재료비, 15_999),
        charge(3, 30, 강사료, 19_001),
    ];
    let total: i64 = charges.iter().map(|c| c.amount).sum();

    // 가용액을 1원씩 옮겨 가며 모든 경계를 훑는다
    for avail in [0, 1, 41_110, 41_111, 41_112, total - 1, total, total + 1] {
        let input = student(charges.clone(), Budget::of(avail), Budget::of(7_777));
        let r = settle_student(&input, &cfg(&[10, 20, 30]));
        ok(&input, &r);

        let sum: i64 = r.allocs.iter().map(|a| a.amount).sum();
        assert_eq!(sum, total, "가용액 {avail}원에서 총액이 어긋났다");
        assert_eq!(r.used.voucher, avail.min(total));
    }
}

#[test]
fn 불변식_검사기는_어긋난_배분을_잡아낸다() {
    let charges = vec![charge(1, 10, 강사료, 40_000)];
    let 모자람 = vec![Alloc {
        enrollment_id: 1,
        department_id: 10,
        item_code: 강사료.into(),
        fund: Fund::Voucher,
        origin: Origin::Plain,
        amount: 39_999,
    }];
    assert!(verify(&charges, &모자람).is_err());

    let 없는칸 = vec![Alloc {
        enrollment_id: 1,
        department_id: 10,
        item_code: 재료비.into(),
        fund: Fund::Voucher,
        origin: Origin::Plain,
        amount: 40_000,
    }];
    assert!(verify(&charges, &없는칸).is_err());

    let 영원 = vec![Alloc {
        enrollment_id: 1,
        department_id: 10,
        item_code: 강사료.into(),
        fund: Fund::Voucher,
        origin: Origin::Plain,
        amount: 0,
    }];
    assert!(verify(&charges, &영원).is_err(), "0원 배분은 만들지 않는다");
}

#[test]
fn 수강이_없으면_배분도_없다() {
    let input = student(Vec::new(), Budget::of(100_000), Budget::of(50_000));
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert!(r.allocs.is_empty());
    assert_eq!(r.used, Used::default());
}

#[test]
fn 큰_금액에서도_넘치지_않는다() {
    let input = student(
        vec![charge(1, 10, 강사료, 1_000_000_000)],
        Budget::of(999_999_999),
        Budget::none(),
    );
    let r = settle_student(&input, &cfg(&[10]));
    ok(&input, &r);
    assert_eq!(by_fund(&r, Fund::Voucher), 999_999_999);
    assert_eq!(by_fund(&r, Fund::VoucherOver), 1);
}
