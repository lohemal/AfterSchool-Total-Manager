//! 정산 통합 시나리오 — 실제 스키마 위에서 요구사항 §21의 목록을 훑는다.
//!
//! 엔진 자체의 경계값은 `domain/settle_tests.rs`에서 이미 본다.
//! 여기서는 **DB에 저장된 정책·기간·이월·선행 정산이 엔진에 제대로 물리는지**를 본다.

use crate::db::Db;
use crate::domain::Program;
use crate::model::{
    BudgetView, CostItem, DepartmentInput, EligibilityInput, EnrollmentInput, Fee, GrantInput,
    Period, StudentInput, WorkspaceInput,
};
use crate::repo;

const 강사료: &str = "INSTRUCTOR";
const 교재비: &str = "TEXTBOOK";

struct S {
    db: Db,
    year: i64,
    items: Vec<CostItem>,
}

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

fn period(name: &str, start: &str, end: &str, limit: i64) -> Period {
    Period {
        id: None,
        name: name.to_string(),
        start_date: start.to_string(),
        end_date: end.to_string(),
        limit_amount: limit,
        seq: 0,
    }
}

impl S {
    fn new() -> Self {
        let db = Db::open_memory().unwrap();
        let year = db
            .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        Self { db, year, items }
    }

    fn ws(&self, name: &str, start: &str, end: &str) -> i64 {
        self.db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    self.year,
                    &WorkspaceInput {
                        name: name.into(),
                        start_date: start.into(),
                        end_date: end.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn student(&self, grade: i64, class_no: impl ToString, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no: class_no.to_string(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn dept(&self, ws: i64, name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some("A반".into()),
                        teacher: None,
                        days: None,
                        note: None,
                        fees,
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, ws: i64, student: i64, dept: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    ws,
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

    fn elig(&self, student: i64, program: Program) {
        self.db
            .write(|c| {
                repo::eligibility::create(
                    c,
                    self.year,
                    &EligibilityInput {
                        student_id: student,
                        program: program.code().into(),
                        valid_from: None,
                        valid_to: None,
                        note: None,
                    },
                )
            })
            .unwrap();
    }

    fn policy(
        &self,
        program: Program,
        annual: i64,
        carryover: bool,
        targets: &str,
        periods: &[Period],
    ) {
        self.db
            .write(|c| {
                repo::policy::save(c, self.year, program.code(), annual, carryover, targets, periods)
            })
            .unwrap();
    }

    fn grant(&self, student: i64, program: Program, period_id: Option<i64>, amount: i64) {
        self.db
            .write(|c| {
                repo::policy::grant_save(
                    c,
                    self.year,
                    &GrantInput {
                        student_id: student,
                        program: program.code().into(),
                        period_id,
                        amount,
                        reason: Some("시험".into()),
                    },
                )
            })
            .unwrap();
    }

    fn period_id(&self, program: Program, name: &str) -> i64 {
        let p = self
            .db
            .read(|c| repo::policy::get(c, self.year, program.code()))
            .unwrap();
        p.periods
            .iter()
            .find(|x| x.name == name)
            .and_then(|x| x.id)
            .expect("지원기간을 찾지 못했습니다")
    }

    fn generate(&self, ws: i64) -> crate::model::GenerateResult {
        self.db.write(|c| repo::settle::generate(c, ws)).unwrap()
    }

    fn status(&self, ws: i64) -> crate::model::SettlementStatus {
        self.db.read(|c| repo::settle::status(c, ws)).unwrap()
    }

    fn summary(&self, ws: i64) -> crate::model::Summary {
        self.db
            .read(|c| repo::settle::summary(c, ws, &self.items))
            .unwrap()
            .expect("정산이 없습니다")
    }

    /// 재원별 합계 (`VOUCHER` 등)
    fn fund(&self, ws: i64, code: &str) -> i64 {
        let s = self.summary(ws);
        match code {
            "SELF_PAY" => s.total.self_pay,
            "VOUCHER" => s.total.voucher,
            "VOUCHER_OVER" => s.total.voucher_over,
            _ => s.total.free_voucher,
        }
    }

    fn budget(&self, ws: i64, student: i64, program: Program) -> BudgetView {
        self.db
            .read(|c| repo::settle::program_rows(c, ws, program, &self.items))
            .unwrap()
            .into_iter()
            .find(|r| r.student_id == student)
            .and_then(|r| r.budget)
            .expect("지원금 기록이 없습니다")
    }
}

/// 1학기(3~8월) 250,000 / 2학기(9~2월) 250,000, 연간 500,000.
fn 학기제(s: &S, carryover: bool) {
    s.policy(
        Program::Voucher,
        500_000,
        carryover,
        "3",
        &[
            period("1학기", "2026-03-01", "2026-08-31", 250_000),
            period("2학기", "2026-09-01", "2027-02-28", 250_000),
        ],
    );
}

// ─────────────────────────────────────────────── 기본

#[test]
fn 자격이_없는_학생은_전액_수익자다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);

    s.generate(ws);
    assert_eq!(s.fund(ws, "SELF_PAY"), 40_000);
    assert_eq!(s.fund(ws, "VOUCHER"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 이용권_대상_학생은_한도만큼_지원받는다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000), fee(교재비, 30_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 70_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 0);

    let b = s.budget(ws, hana, Program::Voucher);
    assert_eq!(b.annual_limit, 500_000);
    assert_eq!(b.period_name, "1학기");
    assert_eq!(b.period_limit, 250_000);
    assert_eq!(b.available, 250_000);
    assert_eq!(b.used_now, 70_000);
    assert_eq!(b.period_left, 180_000);
    assert_eq!(b.annual_left, 430_000);
}

#[test]
fn 대상학년이_아니면_명단에_있어도_지원되지_않는다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let 사학년 = s.student(4, 1, 1, "이사학");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, 사학년, d);
    학기제(&s, false); // 대상학년 3
    s.elig(사학년, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 0);
    assert_eq!(s.fund(ws, "SELF_PAY"), 40_000, "자격이 없으므로 일반 수익자");

    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(issues.iter().any(|i| i.code == "GRADE_MISMATCH"));
}

#[test]
fn 전액_면제로_취소하면_정산에서_빠진다() {
    // v0.1.3 부터 취소 자체가 제외 조건이 아니다. **전액 0원**일 때만 빠진다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let duri = s.student(3, 1, 2, "이두리");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    let e2 = s.enroll(ws, duri, d);
    학기제(&s, false);

    s.db.write(|c| {
        repo::enrollment::cancel(c, e2, Some(&[fee(강사료, 0)]), "개강 전 취소", &s.items)
    })
    .unwrap();
    s.generate(ws);

    assert_eq!(s.fund(ws, "SELF_PAY"), 40_000, "전액 면제한 취소자는 빠진다");
    assert!(s.summary(ws).balanced);
}

#[test]
fn 학생별로_고친_금액이_정산에_그대로_쓰인다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000), fee(교재비, 30_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    // 교재비 면제
    s.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(강사료, 40_000), fee(교재비, 0)], "면제", &s.items)
    })
    .unwrap();

    s.generate(ws);
    assert_eq!(s.summary(ws).total.total, 40_000);
    assert!(s.summary(ws).balanced);
}

// ─────────────────────────────────────────────── 기간·연간 한도 경계 (1원 단위)

#[test]
fn 기간한도를_정확히_소진한다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 250_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 250_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 0);
    assert_eq!(s.budget(ws, hana, Program::Voucher).period_left, 0);
}

#[test]
fn 기간한도를_1원_넘으면_초과금이_1원이다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 250_001)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 250_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 1);
}

#[test]
fn 지원기간이_없으면_연간한도를_그대로_쓴다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 500_001)]);
    s.enroll(ws, hana, d);
    s.policy(Program::Voucher, 500_000, false, "3", &[]); // 기간 없음
    s.elig(hana, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 500_000, "연간한도를 정확히 소진");
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 1, "연간한도 1원 초과");

    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(issues.iter().any(|i| i.code == "NO_PERIOD"));
}

// ─────────────────────────────────────────────── 이월 / 소멸

/// 1학기에 200,000을 쓰고 2학기를 정산한다.
fn 두_학기_시나리오(carryover: bool) -> (S, i64, i64, i64) {
    let s = S::new();
    let 봄 = s.ws("1학기 4월", "2026-04-01", "2026-04-30");
    let 가을 = s.ws("2학기 10월", "2026-10-01", "2026-10-31");
    let hana = s.student(3, 1, 1, "김하나");

    학기제(&s, carryover);
    s.elig(hana, Program::Voucher);

    let d1 = s.dept(봄, "로봇과학", vec![fee(강사료, 200_000)]);
    s.enroll(봄, hana, d1);
    let d2 = s.dept(가을, "로봇과학", vec![fee(강사료, 400_000)]);
    s.enroll(가을, hana, d2);

    s.generate(봄);
    (s, 봄, 가을, hana)
}

#[test]
fn 소멸정책이면_2학기_가용액은_기간한도_그대로다() {
    let (s, 봄, 가을, hana) = 두_학기_시나리오(false);
    assert_eq!(s.fund(봄, "VOUCHER"), 200_000);

    s.generate(가을);
    let b = s.budget(가을, hana, Program::Voucher);
    assert_eq!(b.carry_in, 0);
    assert_eq!(b.period_limit, 250_000);
    assert_eq!(b.available, 250_000);
    assert_eq!(s.fund(가을, "VOUCHER"), 250_000);
    assert_eq!(s.fund(가을, "VOUCHER_OVER"), 150_000);
}

#[test]
fn 이월정책이면_1학기_잔액이_2학기로_넘어간다() {
    let (s, _봄, 가을, hana) = 두_학기_시나리오(true);

    s.generate(가을);
    let b = s.budget(가을, hana, Program::Voucher);
    assert_eq!(b.carry_in, 50_000, "250,000 − 200,000");
    assert_eq!(b.available, 300_000);
    assert_eq!(b.used_prior_periods, 200_000);
    assert_eq!(s.fund(가을, "VOUCHER"), 300_000);
    assert_eq!(s.fund(가을, "VOUCHER_OVER"), 100_000);
}

#[test]
fn 이월해도_연간한도를_넘지_못한다() {
    let s = S::new();
    let 봄 = s.ws("1학기 4월", "2026-04-01", "2026-04-30");
    let 가을 = s.ws("2학기 10월", "2026-10-01", "2026-10-31");
    let hana = s.student(3, 1, 1, "김하나");

    // 연간 400,000인데 기간 합계는 500,000 — 연간이 더 작다
    s.policy(
        Program::Voucher,
        400_000,
        true,
        "3",
        &[
            period("1학기", "2026-03-01", "2026-08-31", 250_000),
            period("2학기", "2026-09-01", "2027-02-28", 250_000),
        ],
    );
    s.elig(hana, Program::Voucher);

    let d1 = s.dept(봄, "로봇과학", vec![fee(강사료, 100_000)]);
    s.enroll(봄, hana, d1);
    let d2 = s.dept(가을, "로봇과학", vec![fee(강사료, 500_000)]);
    s.enroll(가을, hana, d2);

    s.generate(봄);
    s.generate(가을);

    let b = s.budget(가을, hana, Program::Voucher);
    assert_eq!(b.carry_in, 150_000, "1학기 미사용 150,000");
    assert_eq!(b.available, 300_000, "연간 400,000 − 이미 쓴 100,000");
    assert!(b.capped_by_annual);
    assert_eq!(s.fund(가을, "VOUCHER"), 300_000);
    assert_eq!(b.annual_used, 400_000);
    assert_eq!(b.annual_left, 0);
}

#[test]
fn 같은_기간_안의_작업공간은_한도를_나눠_쓴다() {
    let s = S::new();
    let 사월 = s.ws("4월", "2026-04-01", "2026-04-30");
    let 오월 = s.ws("5월", "2026-05-01", "2026-05-31");
    let hana = s.student(3, 1, 1, "김하나");
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    let d1 = s.dept(사월, "로봇과학", vec![fee(강사료, 150_000)]);
    s.enroll(사월, hana, d1);
    let d2 = s.dept(오월, "로봇과학", vec![fee(강사료, 150_000)]);
    s.enroll(오월, hana, d2);

    s.generate(사월);
    s.generate(오월);

    let b = s.budget(오월, hana, Program::Voucher);
    assert_eq!(b.used_in_period_before, 150_000);
    assert_eq!(b.available, 100_000, "250,000 − 150,000");
    assert_eq!(s.fund(오월, "VOUCHER"), 100_000);
    assert_eq!(s.fund(오월, "VOUCHER_OVER"), 50_000);
}

// ─────────────────────────────────────────────── 학생별 예외 한도

#[test]
fn 학생별_연간_예외는_연간한도만_바꾼다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 400_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    // 연간 한도만 300,000으로. 기간 한도는 그대로 250,000
    s.grant(hana, Program::Voucher, None, 300_000);
    s.generate(ws);

    let b = s.budget(ws, hana, Program::Voucher);
    assert_eq!(b.annual_limit, 300_000);
    assert_eq!(b.period_limit, 250_000, "기간 한도는 건드리지 않는다");
    assert_eq!(b.available, 250_000, "둘 중 작은 쪽");
    assert_eq!(s.fund(ws, "VOUCHER"), 250_000);
}

#[test]
fn 학생별_기간_예외는_그_기간_한도만_바꾼다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 400_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    let 일학기 = s.period_id(Program::Voucher, "1학기");
    s.grant(hana, Program::Voucher, Some(일학기), 300_000);
    s.generate(ws);

    let b = s.budget(ws, hana, Program::Voucher);
    assert_eq!(b.annual_limit, 500_000, "연간은 그대로");
    assert_eq!(b.period_limit, 300_000);
    assert_eq!(b.available, 300_000);
    assert_eq!(s.fund(ws, "VOUCHER"), 300_000);
}

#[test]
fn 연간과_기간_예외를_함께_두면_둘_다_적용된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 400_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    let 일학기 = s.period_id(Program::Voucher, "1학기");
    s.grant(hana, Program::Voucher, None, 200_000);
    s.grant(hana, Program::Voucher, Some(일학기), 300_000);
    s.generate(ws);

    let b = s.budget(ws, hana, Program::Voucher);
    assert_eq!(b.annual_limit, 200_000);
    assert_eq!(b.period_limit, 300_000);
    assert_eq!(b.available, 200_000, "연간 쪽이 더 작다");
}

#[test]
fn 예외_한도는_그_학생에게만_적용된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let duri = s.student(3, 1, 2, "이두리");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 300_000)]);
    s.enroll(ws, hana, d);
    s.enroll(ws, duri, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);
    s.elig(duri, Program::Voucher);

    s.grant(hana, Program::Voucher, None, 100_000);
    s.generate(ws);

    assert_eq!(s.budget(ws, hana, Program::Voucher).available, 100_000);
    assert_eq!(s.budget(ws, duri, Program::Voucher).available, 250_000);
    assert_eq!(s.fund(ws, "VOUCHER"), 350_000);
}

// ─────────────────────────────────────────────── 두 제도

#[test]
fn 두_제도를_모두_가진_학생의_흐름() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 50_000)]);
    s.enroll(ws, hana, d);

    s.policy(Program::Voucher, 20_000, false, "3", &[]);
    s.policy(Program::FreeVoucher, 25_000, false, "", &[]);
    s.elig(hana, Program::Voucher);
    s.elig(hana, Program::FreeVoucher);

    s.generate(ws);
    // 요구사항 §10의 예시
    assert_eq!(s.fund(ws, "VOUCHER"), 20_000);
    assert_eq!(s.fund(ws, "FREE_VOUCHER"), 25_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 5_000);
    assert_eq!(s.fund(ws, "SELF_PAY"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 자유수강권만_가진_학생의_초과분은_수익자로_간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(5, 1, 1, "김오학");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 50_000)]);
    s.enroll(ws, hana, d);

    s.policy(Program::Voucher, 500_000, false, "3", &[]);
    s.policy(Program::FreeVoucher, 20_000, false, "", &[]);
    s.elig(hana, Program::FreeVoucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "FREE_VOUCHER"), 20_000);
    assert_eq!(s.fund(ws, "SELF_PAY"), 30_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 0);

    // 수익자 탭에서 '자유수강권 소진 후'로 구분된다
    let rows = s
        .db
        .read(|c| repo::settle::self_pay_rows(c, ws, &s.items))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].origin_free, 30_000);
    assert_eq!(rows[0].origin_plain, 0);
}

#[test]
fn 자유수강권도_지원기간과_이월을_쓴다() {
    let s = S::new();
    let 봄 = s.ws("4월", "2026-04-01", "2026-04-30");
    let 가을 = s.ws("10월", "2026-10-01", "2026-10-31");
    let hana = s.student(5, 1, 1, "김오학");

    s.policy(
        Program::FreeVoucher,
        200_000,
        true,
        "",
        &[
            period("1학기", "2026-03-01", "2026-08-31", 100_000),
            period("2학기", "2026-09-01", "2027-02-28", 100_000),
        ],
    );
    s.elig(hana, Program::FreeVoucher);

    let d1 = s.dept(봄, "미술", vec![fee(강사료, 60_000)]);
    s.enroll(봄, hana, d1);
    let d2 = s.dept(가을, "미술", vec![fee(강사료, 200_000)]);
    s.enroll(가을, hana, d2);

    s.generate(봄);
    s.generate(가을);

    let b = s.budget(가을, hana, Program::FreeVoucher);
    assert_eq!(b.carry_in, 40_000);
    assert_eq!(b.available, 140_000);
    assert_eq!(s.fund(가을, "FREE_VOUCHER"), 140_000);
    assert_eq!(s.fund(가을, "SELF_PAY"), 60_000);
}

// ─────────────────────────────────────────────── 우선순위

#[test]
fn 부서_우선순위_설정이_정산에_반영된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let robot = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    let art = s.dept(ws, "미술", vec![fee(강사료, 25_000)]);
    s.enroll(ws, hana, robot);
    s.enroll(ws, hana, art);
    s.policy(Program::Voucher, 30_000, false, "3", &[]);
    s.elig(hana, Program::Voucher);

    // 미술을 먼저 차감하도록 정한다
    s.db.write(|c| repo::priority::dept_save(c, ws, &[art, robot]))
        .unwrap();
    s.generate(ws);

    let rows = s
        .db
        .read(|c| repo::settle::self_pay_rows(c, ws, &s.items))
        .unwrap();
    let 미술 = rows.iter().find(|r| r.dept_label.starts_with("미술"));
    assert!(미술.is_none(), "미술은 전액 지원되어 수익자 줄이 없다");
    assert_eq!(s.fund(ws, "VOUCHER"), 30_000);

    // 순서를 뒤집으면 분포가 바뀐다
    s.db.write(|c| repo::priority::dept_save(c, ws, &[robot, art]))
        .unwrap();
    s.generate(ws);
    let rows = s
        .db
        .read(|c| repo::settle::self_pay_rows(c, ws, &s.items))
        .unwrap();
    assert!(rows.iter().any(|r| r.dept_label.starts_with("미술")));
    assert_eq!(s.fund(ws, "VOUCHER"), 30_000, "총액은 그대로");
}

#[test]
fn 비용항목_우선순위_설정이_정산에_반영된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000), fee(교재비, 30_000)]);
    s.enroll(ws, hana, d);
    s.policy(Program::Voucher, 30_000, false, "3", &[]);
    s.elig(hana, Program::Voucher);

    // 교재비를 먼저 차감
    s.db.write(|c| {
        repo::priority::item_save(
            c,
            ws,
            &[교재비.to_string(), 강사료.to_string()],
        )
    })
    .unwrap();
    s.generate(ws);

    let sum = s.summary(ws);
    let 교재 = sum.rows.iter().find(|r| r.item_code == 교재비).unwrap();
    assert_eq!(교재.voucher, 30_000, "교재비가 먼저 지원된다");
    let 강사 = sum.rows.iter().find(|r| r.item_code == 강사료).unwrap();
    assert_eq!(강사.voucher, 0);
    assert_eq!(강사.voucher_over, 40_000);
}

// ─────────────────────────────────────────────── stale 판정

#[test]
fn 정산_직후에는_최신이다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);

    assert_eq!(s.status(ws).state, "NONE");
    s.generate(ws);
    assert_eq!(s.status(ws).state, "FRESH");
}

#[test]
fn 수강을_고치면_낡음이_된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let duri = s.student(3, 1, 2, "이두리");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.generate(ws);

    s.enroll(ws, duri, d);
    let st = s.status(ws);
    assert_eq!(st.state, "STALE_DATA");
    assert!(st.message.contains("다시 정산"));
}

#[test]
fn 수강을_취소해도_낡음이_된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.generate(ws);

    s.db.write(|c| repo::enrollment::cancel(c, e, None, "전학", &s.items))
        .unwrap();
    assert_eq!(s.status(ws).state, "STALE_DATA");
    // 기존 정산은 조용히 바뀌지 않는다
    assert_eq!(s.summary(ws).total.total, 40_000);
}

#[test]
fn 지원정책을_고치면_낡음이_된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.generate(ws);

    학기제(&s, true); // 이월정책 변경
    let st = s.status(ws);
    assert_eq!(st.state, "STALE_YEAR");
    assert!(st.message.contains("지원금 설정"));
}

#[test]
fn 선행_작업공간을_재정산하면_후행이_낡음이_된다() {
    let s = S::new();
    let 사월 = s.ws("4월", "2026-04-01", "2026-04-30");
    let 오월 = s.ws("5월", "2026-05-01", "2026-05-31");
    let hana = s.student(3, 1, 1, "김하나");
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    let d1 = s.dept(사월, "로봇과학", vec![fee(강사료, 100_000)]);
    let e1 = s.enroll(사월, hana, d1);
    let d2 = s.dept(오월, "로봇과학", vec![fee(강사료, 200_000)]);
    s.enroll(오월, hana, d2);

    s.generate(사월);
    s.generate(오월);
    assert_eq!(s.status(오월).state, "FRESH");
    assert_eq!(s.budget(오월, hana, Program::Voucher).available, 150_000);

    // 4월 금액을 줄이고 다시 정산한다
    s.db.write(|c| repo::enrollment::update_fees(c, e1, &[fee(강사료, 50_000)], "조정", &s.items))
        .unwrap();
    s.generate(사월);

    let st = s.status(오월);
    assert_eq!(st.state, "STALE_PRIOR");
    assert!(!st.prior_is_earlier_period, "같은 지원기간 안이다");
    assert!(st.message.contains("선행 작업공간"));
    // 자동으로 다시 계산하지 않는다
    assert_eq!(s.fund(오월, "VOUCHER"), 150_000);

    // 사람이 다시 정산하면 새 잔액으로 계산된다
    s.generate(오월);
    assert_eq!(s.status(오월).state, "FRESH");
    assert_eq!(s.budget(오월, hana, Program::Voucher).available, 200_000);
    assert_eq!(s.fund(오월, "VOUCHER"), 200_000);
}

#[test]
fn 이전_지원기간을_재정산하면_이월액이_달라지고_낡음이_된다() {
    let s = S::new();
    let 봄 = s.ws("1학기 4월", "2026-04-01", "2026-04-30");
    let 가을 = s.ws("2학기 10월", "2026-10-01", "2026-10-31");
    let hana = s.student(3, 1, 1, "김하나");
    학기제(&s, true); // 이월
    s.elig(hana, Program::Voucher);

    let d1 = s.dept(봄, "로봇과학", vec![fee(강사료, 200_000)]);
    let e1 = s.enroll(봄, hana, d1);
    let d2 = s.dept(가을, "로봇과학", vec![fee(강사료, 400_000)]);
    s.enroll(가을, hana, d2);

    s.generate(봄);
    s.generate(가을);
    assert_eq!(s.budget(가을, hana, Program::Voucher).carry_in, 50_000);
    assert_eq!(s.budget(가을, hana, Program::Voucher).available, 300_000);

    // 1학기 사용액을 230,000으로 늘린다 (요구사항 §14의 예시)
    s.db.write(|c| repo::enrollment::update_fees(c, e1, &[fee(강사료, 230_000)], "조정", &s.items))
        .unwrap();
    s.generate(봄);

    let st = s.status(가을);
    assert_eq!(st.state, "STALE_PRIOR");
    assert!(st.prior_is_earlier_period, "이전 지원기간이다");
    assert!(st.message.contains("이월액"));

    s.generate(가을);
    let b = s.budget(가을, hana, Program::Voucher);
    assert_eq!(b.carry_in, 20_000, "250,000 − 230,000");
    assert_eq!(b.available, 270_000);
}

#[test]
fn 재정산하면_최신이_바뀌고_과거_정산은_남는다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    let first = s.generate(ws);
    s.db.write(|c| repo::enrollment::update_fees(c, e, &[fee(강사료, 50_000)], "인상", &s.items))
        .unwrap();
    let second = s.generate(ws);

    assert_ne!(first.settlement_id, second.settlement_id);
    let history = s.db.read(|c| repo::settle::history(c, ws)).unwrap();
    assert_eq!(history.len(), 2, "과거 정산이 남아 있다");
    assert_eq!(history.iter().filter(|(_, _, latest)| *latest).count(), 1);
    assert_eq!(s.summary(ws).total.total, 50_000);
}

// ─────────────────────────────────────────────── 학생 상세정보 연결

#[test]
fn 상세정보는_정산_상태에_따라_다르게_말한다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    let state = |program: &str| -> String {
        s.db
            .read(|c| repo::settle::student_supports(c, ws, hana))
            .unwrap()
            .into_iter()
            .find(|x| x.program == program)
            .unwrap()
            .state
    };

    // 정산 전
    assert_eq!(state("VOUCHER"), "BEFORE");
    assert_eq!(state("FREE_VOUCHER"), "NONE", "자격이 없으면 해당없음");

    s.generate(ws);
    assert_eq!(state("VOUCHER"), "OK");

    // 자료를 고치면 재정산 필요
    s.db.write(|c| repo::enrollment::update_fees(c, e, &[fee(강사료, 50_000)], "인상", &s.items))
        .unwrap();
    assert_eq!(state("VOUCHER"), "STALE");

    let supports = s
        .db
        .read(|c| repo::settle::student_supports(c, ws, hana))
        .unwrap();
    let v = supports.iter().find(|x| x.program == "VOUCHER").unwrap();
    assert!(v.budget.is_none(), "낡았으면 숫자를 주지 않는다");
}

// ─────────────────────────────────────────────── 검증

#[test]
fn 금액이_없는_수강이_있으면_정산을_막는다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    // charge를 통째로 지운다 (자료 파손 상황)
    s.db.write(|c| {
        c.execute("DELETE FROM charge WHERE enrollment_id = ?1", [e])?;
        Ok(())
    })
    .unwrap();

    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(issues.iter().any(|i| i.level == "ERROR" && i.code == "NO_CHARGE"));

    let err = s.db.write(|c| repo::settle::generate(c, ws)).unwrap_err();
    assert!(err.message.contains("정산을 진행할 수 없습니다"));
}

#[test]
fn 수강이_없어도_빈_정산을_만들_수_있다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    학기제(&s, false);

    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(issues.iter().any(|i| i.code == "NO_ENROLLMENT" && i.level == "WARN"));

    let r = s.generate(ws);
    assert_eq!(r.allocs, 0);
    assert_eq!(s.summary(ws).total.total, 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 선행_작업공간이_정산되지_않았으면_알려_준다() {
    let s = S::new();
    let 사월 = s.ws("4월", "2026-04-01", "2026-04-30");
    let 오월 = s.ws("5월", "2026-05-01", "2026-05-31");
    let hana = s.student(3, 1, 1, "김하나");
    학기제(&s, false);
    let d = s.dept(사월, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(사월, hana, d);
    let d2 = s.dept(오월, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(오월, hana, d2);

    let issues = s.db.read(|c| repo::settle::validate(c, 오월)).unwrap();
    let hit = issues.iter().find(|i| i.code == "PRIOR_NOT_SETTLED");
    assert!(hit.is_some());
    assert!(hit.unwrap().message.contains("4월"));
}

// ─────────────────────────────────────────────── 총합 불변식

#[test]
fn 여러_학생_여러_부서에서도_배분_합계가_원본과_같다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    학기제(&s, false);
    s.policy(Program::FreeVoucher, 37_777, false, "", &[]);

    let robot = s.dept(ws, "로봇과학", vec![fee(강사료, 41_111), fee(교재비, 27_777)]);
    let art = s.dept(ws, "미술", vec![fee(강사료, 25_555)]);
    let soccer = s.dept(ws, "축구", vec![fee(강사료, 19_001)]);

    let mut total = 0i64;
    for i in 1..=6 {
        let sid = s.student(if i % 2 == 0 { 3 } else { 4 }, 1, i, &format!("학생{i}"));
        s.enroll(ws, sid, robot);
        total += 41_111 + 27_777;
        if i % 2 == 0 {
            s.enroll(ws, sid, art);
            total += 25_555;
            s.elig(sid, Program::Voucher);
        }
        if i % 3 == 0 {
            s.enroll(ws, sid, soccer);
            total += 19_001;
            s.elig(sid, Program::FreeVoucher);
        }
    }

    let r = s.generate(ws);
    assert_eq!(r.total, total, "배분 총액이 원본과 같아야 한다");

    let sum = s.summary(ws);
    assert_eq!(sum.total.total, total);
    assert_eq!(sum.charge_total, total);
    assert!(sum.balanced);

    // 행 합계와 열 합계도 맞는지
    let row_sum: i64 = sum.rows.iter().map(|r| r.total).sum();
    assert_eq!(row_sum, sum.total.total);
    let col_sum = sum.total.self_pay + sum.total.voucher + sum.total.voucher_over + sum.total.free_voucher;
    assert_eq!(col_sum, sum.total.total);
}

#[test]
fn 같은_자료를_두_번_정산하면_같은_결과가_나온다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    학기제(&s, false);
    let robot = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000), fee(교재비, 30_000)]);
    let art = s.dept(ws, "미술", vec![fee(강사료, 25_000)]);
    for i in 1..=4 {
        let sid = s.student(3, 1, i, &format!("학생{i}"));
        s.enroll(ws, sid, robot);
        s.enroll(ws, sid, art);
        s.elig(sid, Program::Voucher);
    }

    let take = |s: &S| -> Vec<(i64, i64, String, String, i64)> {
        s.db
            .read(|c| {
                let mut st = c.prepare(
                    "SELECT a.student_id, a.department_id, a.item_code, a.fund, a.amount
                       FROM settlement_alloc a
                       JOIN settlement t ON t.id = a.settlement_id AND t.is_latest = 1
                      WHERE t.workspace_id = ?1
                      ORDER BY a.student_id, a.department_id, a.item_code, a.fund",
                )?;
                let rows = st
                    .query_map([ws], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .unwrap()
    };

    s.generate(ws);
    let first = take(&s);
    s.generate(ws);
    let second = take(&s);
    assert_eq!(first, second, "같은 자료면 언제나 같은 정산 결과");
    assert!(!first.is_empty());
}

// ─────────────────────────────────────────────── 한글 반 (최종 QA)

#[test]
fn 한글_반_학생도_똑같이_정산된다() {
    // 반이 '가'든 '1'이든 금액 계산은 달라질 까닭이 없다. 그걸 못 박아 둔다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, "가", 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000), fee(교재비, 30_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    s.generate(ws);
    assert_eq!(s.fund(ws, "VOUCHER"), 70_000);
    assert_eq!(s.fund(ws, "SELF_PAY"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 한글_반_학생이_이용권_결과_화면에_나온다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, "해", 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);
    s.generate(ws);

    let rows = s
        .db
        .read(|c| repo::settle::program_rows(c, ws, Program::Voucher, &s.items))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].class_no, "해");
}

#[test]
fn 수익자_화면에도_한글_반이_그대로_나온다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, "나", 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 40_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.generate(ws);

    let rows = s.db.read(|c| repo::settle::self_pay_rows(c, ws, &s.items)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].class_no, "나");
}

#[test]
fn 정산_결과도_반_차례대로_나온다() {
    // 숫자 반이 1, 10, 2 로 놓이면 품의·수익자 자료의 차례가 뒤죽박죽이 된다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 10_000)]);
    for (i, cls) in ["나", "10", "가", "2", "1"].iter().enumerate() {
        let id = s.student(3, cls, i as i64 + 1, "아무개");
        s.enroll(ws, id, d);
    }
    학기제(&s, false);
    s.generate(ws);

    let rows = s.db.read(|c| repo::settle::self_pay_rows(c, ws, &s.items)).unwrap();
    let 반: Vec<&str> = rows.iter().map(|r| r.class_no.as_str()).collect();
    assert_eq!(반, vec!["1", "2", "10", "가", "나"]);
}

#[test]
fn 수익자_이용권_자유수강권_Excel에_한글_반이_그대로_나온다() {
    // 품의는 부서별 집계라 반을 쓰지 않는다. 그러나 이 셋은 학생의 반을 찍는다.
    use crate::excel::read;
    use std::path::PathBuf;

    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, "해", 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 600_000)]);
    s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);
    s.elig(hana, Program::FreeVoucher);
    s.generate(ws);

    let dir = std::env::temp_dir().join("afterschool-admin-class");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scope = ["2026학년도", "4월"];

    // 수익자
    let rows = s
        .db
        .read(|c| repo::settle::self_pay_rows(c, ws, &s.items))
        .unwrap();
    assert!(!rows.is_empty(), "한도를 넘겨 수익자 부담이 남아야 한다");
    let made =
        crate::excel::admin::write_self_pay(&rows, &s.items, &scope, &dir).unwrap();
    반_확인(&made.path, "해");

    // 방과후 이용권 · 자유수강권
    for program in [Program::Voucher, Program::FreeVoucher] {
        let rows = s
            .db
            .read(|c| repo::settle::program_rows(c, ws, program, &s.items))
            .unwrap();
        assert!(!rows.is_empty(), "{:?} 결과가 비었다", program);
        let made =
            crate::excel::admin::write_program(
                &rows,
                &s.items,
                program.label(),
                program == Program::Voucher,
                &scope,
                &dir,
            )
            .unwrap();
        반_확인(&made.path, "해");
    }

    fn 반_확인(path: &str, 기대: &str) {
        let sheet = read::read_first_sheet(&PathBuf::from(path)).unwrap();
        let c = sheet.require("반").unwrap();
        for (_, cells) in &sheet.rows {
            assert_eq!(sheet.cell(cells, Some(c)), 기대, "{path}");
        }
    }
}

// ─────────────────────────────────────────── 취소자 정산 (v0.1.3, 최종 QA)
//
// **취소 = 전액 0원이 아니다.** 중도에 그만두어도 이미 발생한 비용은 징수한다.
// 교재비·재료비는 배부한 뒤라면 환불하지 않고, 강사료·수용비는 실제 수강한 만큼
// 받는다. 그래서 `status` 가 아니라 charge 금액이 정산 대상을 정한다.

const 수용비: &str = "OPERATION";
const 재료비: &str = "MATERIAL";

/// 요구사항에 적힌 그 예시 — 58,000원 가운데 41,500원을 징수한다.
fn 중도취소_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 15_000),
        fee(수용비, 1_500),
        fee(교재비, 15_000),
        fee(재료비, 10_000),
    ]
}

fn 원래_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 30_000),
        fee(수용비, 3_000),
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

fn 취소(s: &S, e: i64, fees: Option<&[Fee]>, reason: &str) {
    s.db.write(|c| repo::enrollment::cancel(c, e, fees, reason, &s.items))
        .unwrap();
}

fn 총배분(s: &S, ws: i64) -> i64 {
    let t = s.summary(ws).total;
    t.self_pay + t.voucher + t.voucher_over + t.free_voucher
}

#[test]
fn 취소해도_최종_징수금액이_정산에_들어간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    취소(&s, e, Some(&중도취소_금액()), "5월 중도 포기, 교재·재료 배부 완료");
    s.generate(ws);

    assert_eq!(s.fund(ws, "SELF_PAY"), 41_500, "취소해도 41,500원은 징수한다");
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소_전액_0원이면_배분이_없다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    취소(&s, e, Some(&전액면제()), "개강 전 취소");
    s.generate(ws);

    assert_eq!(총배분(&s, ws), 0);
    assert!(s.summary(ws).balanced, "0원이어도 배분 합계는 맞아야 한다");
}

#[test]
fn 취소자가_일반학생이면_수익자로_간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "SELF_PAY"), 41_500);
    assert_eq!(s.fund(ws, "VOUCHER"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자가_이용권_대상이면_이용권에서_나간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.elig(hana, Program::Voucher);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "VOUCHER"), 41_500);
    assert_eq!(s.fund(ws, "SELF_PAY"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자가_이용권_한도를_넘기면_초과금이_된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 400_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false); // 이용권 1학기 250,000
    s.elig(hana, Program::Voucher);

    취소(&s, e, Some(&[fee(강사료, 300_000)]), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "VOUCHER"), 250_000);
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 50_000, "한도를 넘은 만큼 초과금");
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자가_자유수강권_대상이면_자유수강권에서_나간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.policy(Program::FreeVoucher, 600_000, false, "", &[]);
    s.elig(hana, Program::FreeVoucher);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "FREE_VOUCHER"), 41_500);
    assert_eq!(s.fund(ws, "SELF_PAY"), 0);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자가_자유수강권을_다_쓰면_남은_금액은_수익자다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 60_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.policy(Program::FreeVoucher, 30_000, false, "", &[]);
    s.elig(hana, Program::FreeVoucher);

    취소(&s, e, Some(&[fee(강사료, 50_000)]), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "FREE_VOUCHER"), 30_000);
    assert_eq!(s.fund(ws, "SELF_PAY"), 20_000);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자가_두_제도_대상이면_확정된_차감_순서를_따른다() {
    // 이용권 → 자유수강권 → 수익자. 취소자라고 순서가 달라지지 않는다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", vec![fee(강사료, 500_000)]);
    let e = s.enroll(ws, hana, d);
    학기제(&s, false); // 이용권 1학기 250,000
    s.policy(Program::FreeVoucher, 100_000, false, "", &[]);
    s.elig(hana, Program::Voucher);
    s.elig(hana, Program::FreeVoucher);

    취소(&s, e, Some(&[fee(강사료, 400_000)]), "중도 포기");
    s.generate(ws);

    assert_eq!(s.fund(ws, "VOUCHER"), 250_000, "이용권을 먼저 쓴다");
    assert_eq!(s.fund(ws, "FREE_VOUCHER"), 100_000, "다음이 자유수강권");
    assert_eq!(s.fund(ws, "VOUCHER_OVER"), 50_000, "이용권 대상자라 초과금");
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소자를_섞어도_배분_합계가_원본과_같다() {
    // 불변식 — 1원이라도 어긋나면 아무것도 저장하지 않는다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    학기제(&s, false);
    s.policy(Program::FreeVoucher, 40_000, false, "", &[]);

    let a = s.student(3, 1, 1, "수강중일반");
    let b = s.student(3, 1, 2, "취소일반");
    let c = s.student(3, 1, 3, "취소이용권");
    let d4 = s.student(3, 1, 4, "취소자유");
    let e5 = s.student(3, 1, 5, "취소전액면제");
    s.elig(c, Program::Voucher);
    s.elig(d4, Program::FreeVoucher);

    s.enroll(ws, a, d);
    let eb = s.enroll(ws, b, d);
    let ec = s.enroll(ws, c, d);
    let ed = s.enroll(ws, d4, d);
    let ee = s.enroll(ws, e5, d);

    취소(&s, eb, Some(&중도취소_금액()), "포기");
    취소(&s, ec, Some(&중도취소_금액()), "포기");
    취소(&s, ed, Some(&중도취소_금액()), "포기");
    취소(&s, ee, Some(&전액면제()), "개강 전");

    s.generate(ws);
    // 수강중 58,000 + 취소 41,500 × 3 = 182,500
    assert_eq!(총배분(&s, ws), 182_500);
    assert!(s.summary(ws).balanced, "배분 합계 = 원본 charge 합계");
}

#[test]
fn 취소하면_정산이_낡음이_된다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    s.generate(ws);
    assert_eq!(s.status(ws).state, "FRESH");

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    assert_ne!(
        s.status(ws).state, "FRESH",
        "취소했으면 재정산 필요가 되어야 한다"
    );
}

#[test]
fn 취소_후_금액을_다시_고칠_수_있다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.generate(ws);

    // 징수금액을 잘못 넣었다면 다시 고칠 수 있어야 한다
    s.db.write(|c| {
        repo::enrollment::update_fees(
            c,
            e,
            &[
                fee(강사료, 20_000),
                fee(수용비, 1_500),
                fee(교재비, 15_000),
                fee(재료비, 10_000),
            ],
            "실제 수강 횟수 정정",
            &s.items,
        )
    })
    .unwrap();
    assert_ne!(
        s.status(ws).state, "FRESH",
        "금액을 고쳤으면 재정산 필요가 되어야 한다"
    );

    s.generate(ws);
    assert_eq!(s.fund(ws, "SELF_PAY"), 46_500);
    assert!(s.summary(ws).balanced);
}

#[test]
fn 취소를_되돌려도_금액은_취소_때_확정한_값이다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.db.write(|c| repo::enrollment::restore(c, e, false, "착오", &s.items))
        .unwrap();

    let row = s.db.read(|c| repo::enrollment::get(c, e, &s.items)).unwrap();
    assert_eq!(row.status, "ACTIVE");
    assert_eq!(row.total, 41_500, "프로그램이 임의로 되돌리지 않는다");
}

#[test]
fn 복원할_때_고르면_부서_기준금액으로_돌아간다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);

    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.db.write(|c| repo::enrollment::restore(c, e, true, "다시 수강", &s.items))
        .unwrap();

    let row = s.db.read(|c| repo::enrollment::get(c, e, &s.items)).unwrap();
    assert_eq!(row.total, 58_000);
    assert!(!row.has_override, "기준금액과 같으므로 수정 표시가 없다");
}

#[test]
fn 취소_이력이_한_줄에_이전과_최종_금액을_담는다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);

    취소(&s, e, Some(&중도취소_금액()), "5월 중도 포기, 교재·재료 배부 완료");

    let logs = s
        .db
        .read(|c| repo::change_log::list(c, s.year, Some(ws), None, Some("ENROLL_CANCEL"), 50))
        .unwrap();
    assert_eq!(logs.len(), 1, "한 번의 취소는 한 줄이다");
    let l = &logs[0];
    assert!(l.before_value.contains("수강중"), "{}", l.before_value);
    assert!(l.before_value.contains("58,000"), "{}", l.before_value);
    assert!(l.after_value.contains("취소"), "{}", l.after_value);
    assert!(l.after_value.contains("41,500"), "{}", l.after_value);
    assert!(l.after_value.contains("15,000"), "항목별 금액도 남는다");
    assert_eq!(l.reason, "5월 중도 포기, 교재·재료 배부 완료");
}

#[test]
fn 예전에_취소된_수강은_금액을_확인하라고_알린다() {
    // 예전 버전에서 취소한 건은 금액이 취소 전 그대로다. 그것이 이제 정산에
    // 들어가므로 프로그램이 추측하지 않고 사람에게 확인시킨다.
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);

    취소(&s, e, None, "전학"); // 금액을 손대지 않은 취소 = 예전 방식
    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(
        issues.iter().any(|i| i.code == "CANCEL_FULL_AMOUNT"),
        "{issues:?}"
    );

    // 금액을 확정하면 안내가 사라진다
    s.db.write(|c| {
        repo::enrollment::update_fees(c, e, &중도취소_금액(), "실제 징수금액 확정", &s.items)
    })
    .unwrap();
    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(!issues.iter().any(|i| i.code == "CANCEL_FULL_AMOUNT"));
}

#[test]
fn 취소자만_있어도_빈_정산이_아니다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    취소(&s, e, Some(&중도취소_금액()), "중도 포기");

    let issues = s.db.read(|c| repo::settle::validate(c, ws)).unwrap();
    assert!(
        !issues.iter().any(|i| i.code == "NO_ENROLLMENT"),
        "징수할 금액이 있으므로 빈 정산이 아니다: {issues:?}"
    );
}

#[test]
fn 취소자도_수익자_화면에_나온다() {
    let s = S::new();
    let ws = s.ws("4월", "2026-04-01", "2026-04-30");
    let hana = s.student(3, 1, 1, "김하나");
    let d = s.dept(ws, "로봇과학", 원래_금액());
    let e = s.enroll(ws, hana, d);
    학기제(&s, false);
    취소(&s, e, Some(&중도취소_금액()), "중도 포기");
    s.generate(ws);

    let rows = s
        .db
        .read(|c| repo::settle::self_pay_rows(c, ws, &s.items))
        .unwrap();
    assert_eq!(rows.len(), 1, "취소자도 수익자 화면에 나온다");
    assert_eq!(rows[0].total, 41_500);
}
