//! 품의 집계와 행정자료 Excel 시험 (요구사항 §15).
//!
//! 가장 중요한 것은 **교차검증**이다 — 같은 돈이 화면마다 다른 숫자로 나오면 안 된다.
//! 품의의 네 열 합계는 정산 탭 집계와 1원까지 같아야 한다.

use std::path::PathBuf;

use crate::db::Db;
use crate::domain::Program;
use crate::model::{
    CostItem, DepartmentInput, EligibilityInput, EnrollmentInput, Fee, Period, StudentInput,
    WorkspaceInput,
};
use crate::repo;

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

struct P {
    db: Db,
    year: i64,
    ws: i64,
    items: Vec<CostItem>,
}

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-proposal-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

impl P {
    /// 이용권 연간 500,000(대상학년 3) · 자유수강권 연간 60,000, 기간 없음.
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
                        name: "2026년 4월".into(),
                        start_date: "2026-04-01".into(),
                        end_date: "2026-04-30".into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        let p = Self { db, year, ws, items };
        p.policy(Program::Voucher, 500_000, "3", &[]);
        p.policy(Program::FreeVoucher, 60_000, "", &[]);
        p
    }

    fn policy(&self, program: Program, annual: i64, targets: &str, periods: &[Period]) {
        self.db
            .write(|c| {
                repo::policy::save(c, self.year, program.code(), annual, false, targets, periods)
            })
            .unwrap();
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

    fn dept(&self, name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
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

    fn settle(&self) {
        self.db
            .write(|c| repo::settle::generate(c, self.ws))
            .unwrap();
    }

    fn proposal(&self, kind: &str) -> crate::model::Proposal {
        self.db
            .read(|c| repo::proposal::build(c, self.ws, kind, &self.items))
            .unwrap()
    }

    /// 품의 한 열의 전체 합계
    fn col(&self, p: &crate::model::Proposal, fund: &str) -> i64 {
        let i = p.columns.iter().position(|c| c.fund == fund).unwrap();
        p.total.amounts[i]
    }

    /// 정산 탭 쪽 집계 (교차검증용)
    fn alloc_sum(&self, fund: &str, item_codes: &[&str]) -> i64 {
        self.db
            .read(|c| {
                let list = item_codes
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT COALESCE(SUM(a.amount), 0) FROM settlement_alloc a
                       JOIN settlement s ON s.id = a.settlement_id AND s.is_latest = 1
                      WHERE s.workspace_id = ?1 AND a.fund = ?2 AND a.item_code IN ({list})"
                );
                Ok(c.query_row(&sql, rusqlite::params![self.ws, fund], |r| r.get(0))?)
            })
            .unwrap()
    }
}

// ─────────────────────────────────────────────── 열 이름표

#[test]
fn 이천이십육학년도_품의_열_이름표() {
    let p = P::new();
    // 전교생 1~6학년이 있어야 '수익자(1,2,4,5,6학년)'가 나온다
    for g in 1..=6 {
        p.student(g, 1, &format!("{g}학년생"));
    }
    let cols = p.db.read(|c| repo::proposal::columns(c, p.year)).unwrap();

    assert_eq!(cols.len(), 4);
    assert_eq!(cols[0].fund, "SELF_PAY");
    assert_eq!(cols[0].label, "수익자(1,2,4,5,6학년)");
    assert_eq!(cols[1].fund, "VOUCHER_OVER");
    assert_eq!(cols[1].label, "3학년 초과금");
    assert_eq!(cols[2].fund, "VOUCHER");
    assert_eq!(cols[2].label, "3학년 지원금");
    assert_eq!(cols[3].fund, "FREE_VOUCHER");
    assert_eq!(cols[3].label, "자유수강권");
}

#[test]
fn 대상학년을_바꾸면_이름표만_따라_바뀐다() {
    let p = P::new();
    for g in 1..=6 {
        p.student(g, 1, &format!("{g}학년생"));
    }
    // 대상학년을 3,4로 바꾼다
    p.policy(Program::Voucher, 500_000, "3,4", &[]);

    let cols = p.db.read(|c| repo::proposal::columns(c, p.year)).unwrap();
    assert_eq!(cols[0].label, "수익자(1,2,5,6학년)");
    assert_eq!(cols[1].label, "3·4학년 초과금");
    assert_eq!(cols[2].label, "3·4학년 지원금");
    // 재원 코드는 그대로 — 내부 계산은 학년을 모른다
    assert_eq!(
        cols.iter().map(|c| c.fund.as_str()).collect::<Vec<_>>(),
        vec!["SELF_PAY", "VOUCHER_OVER", "VOUCHER", "FREE_VOUCHER"]
    );
}

#[test]
fn 대상학년_변경은_이미_만든_정산값을_건드리지_않는다() {
    let p = P::new();
    let 삼학년 = p.student(3, 1, "김하나");
    let d = p.dept("로봇과학", vec![fee(강사료, 100_000)]);
    p.enroll(삼학년, d);
    p.elig(삼학년, Program::Voucher);
    p.settle();

    let before = p.alloc_sum("VOUCHER", &[강사료]);
    assert_eq!(before, 100_000);

    // 대상학년을 4학년으로 바꾼다 → 저장된 정산은 그대로다
    p.policy(Program::Voucher, 500_000, "4", &[]);
    assert_eq!(p.alloc_sum("VOUCHER", &[강사료]), before);

    // 다만 정산은 낡음이 되고, 품의는 만들어지지 않는다
    let st = p.db.read(|c| repo::settle::status(c, p.ws)).unwrap();
    assert_eq!(st.state, "STALE_YEAR");
    assert!(p
        .db
        .read(|c| repo::proposal::build(c, p.ws, 강사료, &p.items))
        .is_err());
}

#[test]
fn 대상학년이_없으면_일반적인_이름을_쓴다() {
    let p = P::new();
    p.student(3, 1, "김하나");
    p.policy(Program::Voucher, 500_000, "", &[]);

    let cols = p.db.read(|c| repo::proposal::columns(c, p.year)).unwrap();
    assert_eq!(cols[0].label, "수익자");
    assert_eq!(cols[1].label, "이용권 초과금");
    assert_eq!(cols[2].label, "이용권 지원금");
}

// ─────────────────────────────────────────────── 부서별 집계

/// 요구사항 §15의 상황을 한 작업공간에 모두 담는다.
///
/// * 축구  — 일반 수익자만
/// * 미술  — 이용권으로 전액 지원
/// * 로봇과학 — 이용권 초과금 발생 + 한 charge가 두 재원으로 분할
/// * 컴퓨터 — 이용권 + 자유수강권 중복 학생
fn 여러_상황(p: &P) -> (i64, i64, i64, i64) {
    let 일반 = p.student(5, 1, "박오학"); // 자격 없음
    let 이용권 = p.student(3, 1, "김하나"); // 이용권만
    let 중복 = p.student(3, 2, "이두리"); // 이용권 + 자유수강권
    p.elig(이용권, Program::Voucher);
    p.elig(중복, Program::Voucher);
    p.elig(중복, Program::FreeVoucher);

    let 축구 = p.dept("축구", vec![fee(강사료, 30_000)]);
    let 미술 = p.dept("미술", vec![fee(강사료, 20_000), fee(재료비, 10_000)]);
    let 로봇 = p.dept("로봇과학", vec![fee(강사료, 400_000), fee(교재비, 100_000)]);
    let 컴퓨터 = p.dept("컴퓨터", vec![fee(강사료, 50_000), fee(수용비, 5_000)]);

    p.enroll(일반, 축구);
    p.enroll(이용권, 미술);
    p.enroll(이용권, 로봇);
    p.enroll(중복, 컴퓨터);

    // 차감 순서를 못 박는다 — 미술 → 로봇과학
    p.db.write(|c| repo::priority::dept_save(c, p.ws, &[미술, 로봇, 컴퓨터, 축구]))
        .unwrap();
    p.settle();
    (축구, 미술, 로봇, 컴퓨터)
}

#[test]
fn 부서별_집계와_행_합계가_맞는다() {
    let p = P::new();
    let (_축구, _미술, _로봇, _컴퓨터) = 여러_상황(&p);

    let 강사 = p.proposal(강사료);
    assert!(강사.balanced);

    // 각 부서 행: 네 열의 합 == 그 행의 합계
    for r in &강사.rows {
        assert_eq!(
            r.amounts.iter().sum::<i64>(),
            r.total,
            "{} 행 합계가 어긋난다",
            r.dept_label
        );
    }
    // 전체 합계 행: 열별로 각 부서의 합
    for (i, _) in 강사.columns.iter().enumerate() {
        let want: i64 = 강사.rows.iter().map(|r| r.amounts[i]).sum();
        assert_eq!(강사.total.amounts[i], want);
    }
    assert_eq!(
        강사.total.total,
        강사.rows.iter().map(|r| r.total).sum::<i64>()
    );
}

#[test]
fn 일반_수익자만_있는_부서는_수익자_열에만_들어간다() {
    let p = P::new();
    let (축구, _, _, _) = 여러_상황(&p);
    let 강사 = p.proposal(강사료);

    let row = 강사.rows.iter().find(|r| r.department_id == 축구).unwrap();
    assert_eq!(row.dept_label, "축구A반");
    assert_eq!(row.amounts[0], 30_000, "수익자");
    assert_eq!(row.amounts[1], 0, "초과금");
    assert_eq!(row.amounts[2], 0, "지원금");
    assert_eq!(row.amounts[3], 0, "자유수강권");
    assert_eq!(row.total, 30_000);
}

#[test]
fn 전액_지원된_부서는_지원금_열에만_들어간다() {
    let p = P::new();
    let (_, 미술, _, _) = 여러_상황(&p);
    let 강사 = p.proposal(강사료);

    let row = 강사.rows.iter().find(|r| r.department_id == 미술).unwrap();
    assert_eq!(row.amounts[0], 0);
    assert_eq!(row.amounts[2], 20_000, "이용권 지원금");
    assert_eq!(row.total, 20_000);
}

#[test]
fn 한_charge가_두_재원으로_쪼개져도_품의에서_갈라_담긴다() {
    let p = P::new();
    let (_, _, 로봇, _) = 여러_상황(&p);
    let 강사 = p.proposal(강사료);

    // 김하나: 이용권 500,000. 미술 20,000 + 10,000 → 470,000 남음.
    // 로봇 강사료 400,000 전액 지원, 교재비 100,000 중 70,000 지원 + 30,000 초과.
    let row = 강사.rows.iter().find(|r| r.department_id == 로봇).unwrap();
    assert_eq!(row.amounts[2], 400_000, "강사료는 전액 지원");
    assert_eq!(row.amounts[1], 0);

    let 교재 = p.proposal(교재비);
    let row = 교재.rows.iter().find(|r| r.department_id == 로봇).unwrap();
    assert_eq!(row.amounts[2], 70_000, "교재비 일부 지원");
    assert_eq!(row.amounts[1], 30_000, "나머지는 초과금");
    assert_eq!(row.total, 100_000);
}

#[test]
fn 중복_지원_학생은_fund를_기준으로_갈린다() {
    // 요구사항 §8 — origin이 FREE_EXHAUSTED여도 최종 부담은 이용권 초과금이다
    let p = P::new();
    let 중복 = p.student(3, 1, "이두리");
    p.elig(중복, Program::Voucher);
    p.elig(중복, Program::FreeVoucher);
    p.policy(Program::Voucher, 300_000, "3", &[]);
    p.policy(Program::FreeVoucher, 60_000, "", &[]);

    let d = p.dept("로봇과학", vec![fee(강사료, 400_000)]);
    p.enroll(중복, d);
    p.settle();

    let 강사 = p.proposal(강사료);
    let row = &강사.rows[0];
    assert_eq!(row.amounts[2], 300_000, "3학년 지원금");
    assert_eq!(row.amounts[3], 60_000, "자유수강권");
    assert_eq!(row.amounts[1], 40_000, "3학년 초과금 — 일반 수익자가 아니다");
    assert_eq!(row.amounts[0], 0, "수익자 열은 0이어야 한다");
    assert_eq!(row.total, 400_000);

    // origin은 FREE_EXHAUSTED이지만 품의 분류는 fund를 따른다
    let allocs = p
        .db
        .read(|c| repo::settle::student_allocs(c, p.ws, 중복))
        .unwrap();
    let over = allocs.iter().find(|a| a.fund == "VOUCHER_OVER").unwrap();
    assert_eq!(over.origin, "FREE_EXHAUSTED");
    assert_eq!(over.amount, 40_000);
}

#[test]
fn 금액이_없는_부서는_품의에_넣지_않는다() {
    let p = P::new();
    let (_, _, _, _) = 여러_상황(&p);

    // 수용비가 있는 부서는 컴퓨터뿐이다
    let 수용 = p.proposal(수용비);
    assert_eq!(수용.rows.len(), 1);
    assert_eq!(수용.rows[0].dept_label, "컴퓨터A반");
    assert_eq!(수용.total.total, 5_000);
}

// ─────────────────────────────────────────────── 비용항목 선택

#[test]
fn 네_가지_항목을_각각_뽑을_수_있다() {
    let p = P::new();
    여러_상황(&p);

    for code in [강사료, 수용비, 교재비, 재료비] {
        let pr = p.proposal(code);
        assert!(pr.balanced, "{code} 품의가 맞지 않는다");
        assert_eq!(
            pr.total.total,
            p.alloc_sum("SELF_PAY", &[code])
                + p.alloc_sum("VOUCHER_OVER", &[code])
                + p.alloc_sum("VOUCHER", &[code])
                + p.alloc_sum("FREE_VOUCHER", &[code]),
            "{code} 총액이 정산과 다르다"
        );
    }
}

#[test]
fn 교재재료비_통합은_두_항목을_더한다() {
    let p = P::new();
    여러_상황(&p);

    let 통합 = p.proposal("TEXTBOOK+MATERIAL");
    assert_eq!(통합.item_label, "교재비·재료비");
    assert_eq!(통합.item_codes, vec![교재비.to_string(), 재료비.to_string()]);

    let 교재 = p.proposal(교재비);
    let 재료 = p.proposal(재료비);
    assert_eq!(통합.total.total, 교재.total.total + 재료.total.total);

    for (i, _) in 통합.columns.iter().enumerate() {
        assert_eq!(
            통합.total.amounts[i],
            교재.total.amounts[i] + 재료.total.amounts[i],
            "{}번째 열이 어긋난다",
            i
        );
    }
    assert!(통합.balanced);
}

#[test]
fn 품의_종류_목록에_교재재료비_통합이_들어_있다() {
    let items = Db::open_memory()
        .unwrap()
        .read(|c| repo::cost_items(c))
        .unwrap();
    let kinds = repo::proposal::kinds(&items);
    assert_eq!(kinds.len(), 5, "항목 4개 + 통합 1개");
    let last = kinds.last().unwrap();
    assert_eq!(last.key, "TEXTBOOK+MATERIAL");
    assert_eq!(last.item_codes.len(), 2);
}

// ─────────────────────────────────────────────── 교차검증

#[test]
fn 품의_합계가_정산_탭_집계와_1원까지_같다() {
    let p = P::new();
    여러_상황(&p);

    for code in [강사료, 수용비, 교재비, 재료비] {
        let pr = p.proposal(code);
        assert_eq!(p.col(&pr, "SELF_PAY"), p.alloc_sum("SELF_PAY", &[code]));
        assert_eq!(
            p.col(&pr, "VOUCHER_OVER"),
            p.alloc_sum("VOUCHER_OVER", &[code])
        );
        assert_eq!(p.col(&pr, "VOUCHER"), p.alloc_sum("VOUCHER", &[code]));
        assert_eq!(
            p.col(&pr, "FREE_VOUCHER"),
            p.alloc_sum("FREE_VOUCHER", &[code])
        );
    }
}

#[test]
fn 품의_총액이_그_항목의_정산_총액과_같다() {
    let p = P::new();
    여러_상황(&p);

    let 강사 = p.proposal(강사료);
    assert_eq!(강사.total.total, 강사.settlement_total);
    assert!(강사.balanced);

    let 통합 = p.proposal("TEXTBOOK+MATERIAL");
    assert_eq!(통합.total.total, 통합.settlement_total);
}

#[test]
fn 네_항목_품의를_모두_더하면_정산_전체_총액이_된다() {
    let p = P::new();
    여러_상황(&p);

    let sum: i64 = [강사료, 수용비, 교재비, 재료비]
        .iter()
        .map(|c| p.proposal(c).total.total)
        .sum();
    let summary = p
        .db
        .read(|c| repo::settle::summary(c, p.ws, &p.items))
        .unwrap()
        .unwrap();
    assert_eq!(sum, summary.total.total);
    assert!(summary.balanced);
}

#[test]
fn 수익자_탭_합계와_품의의_수익자_두_열_합이_맞는다() {
    let p = P::new();
    여러_상황(&p);

    let rows = p
        .db
        .read(|c| repo::settle::self_pay_rows(c, p.ws, &p.items))
        .unwrap();
    let tab_total: i64 = rows.iter().map(|r| r.total).sum();

    // 수익자 탭은 SELF_PAY + VOUCHER_OVER를 함께 보여 준다
    let proposal_total: i64 = [강사료, 수용비, 교재비, 재료비]
        .iter()
        .map(|c| {
            let pr = p.proposal(c);
            p.col(&pr, "SELF_PAY") + p.col(&pr, "VOUCHER_OVER")
        })
        .sum();
    assert_eq!(tab_total, proposal_total);
}

#[test]
fn 이용권_탭_합계와_품의의_지원금_초과금이_맞는다() {
    let p = P::new();
    여러_상황(&p);

    let rows = p
        .db
        .read(|c| repo::settle::program_rows(c, p.ws, Program::Voucher, &p.items))
        .unwrap();
    let used: i64 = rows.iter().map(|r| r.used_total).sum();
    let over: i64 = rows.iter().map(|r| r.over_total).sum();

    let (p_used, p_over) = [강사료, 수용비, 교재비, 재료비]
        .iter()
        .fold((0, 0), |(u, o), c| {
            let pr = p.proposal(c);
            (u + p.col(&pr, "VOUCHER"), o + p.col(&pr, "VOUCHER_OVER"))
        });
    assert_eq!(used, p_used);
    assert_eq!(over, p_over);
}

#[test]
fn 자유수강권_탭_합계와_품의가_맞는다() {
    let p = P::new();
    여러_상황(&p);

    let rows = p
        .db
        .read(|c| repo::settle::program_rows(c, p.ws, Program::FreeVoucher, &p.items))
        .unwrap();
    let used: i64 = rows.iter().map(|r| r.used_total).sum();

    let p_used: i64 = [강사료, 수용비, 교재비, 재료비]
        .iter()
        .map(|c| p.col(&p.proposal(c), "FREE_VOUCHER"))
        .sum();
    assert_eq!(used, p_used);
}

// ─────────────────────────────────────────────── 출력 차단

#[test]
fn 정산_전에는_품의를_만들지_않는다() {
    let p = P::new();
    let s = p.student(3, 1, "김하나");
    let d = p.dept("로봇과학", vec![fee(강사료, 40_000)]);
    p.enroll(s, d);

    let err = p
        .db
        .read(|c| repo::proposal::build(c, p.ws, 강사료, &p.items))
        .unwrap_err();
    assert!(err.message.contains("정산 전"), "{}", err.message);
}

#[test]
fn 낡은_정산으로는_품의를_만들지_않는다() {
    let p = P::new();
    let s = p.student(3, 1, "김하나");
    let d = p.dept("로봇과학", vec![fee(강사료, 40_000)]);
    let e = p.enroll(s, d);
    p.settle();
    assert!(p
        .db
        .read(|c| repo::proposal::build(c, p.ws, 강사료, &p.items))
        .is_ok());

    // 금액을 고치면 낡음이 된다
    p.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(강사료, 50_000)], "인상", &p.items)
    })
    .unwrap();

    let err = p
        .db
        .read(|c| repo::proposal::build(c, p.ws, 강사료, &p.items))
        .unwrap_err();
    assert!(err.message.contains("재정산"), "{}", err.message);
    assert!(err.message.contains("틀린 금액"), "{}", err.message);
}

#[test]
fn 낡은_정산으로는_정산_Excel도_만들지_않는다() {
    let p = P::new();
    let s = p.student(3, 1, "김하나");
    let d = p.dept("로봇과학", vec![fee(강사료, 40_000)]);
    let e = p.enroll(s, d);
    p.settle();
    assert!(p.db.read(|c| repo::settle::require_fresh(c, p.ws)).is_ok());

    p.db.write(|c| repo::enrollment::cancel(c, e, "전학", &p.items))
        .unwrap();
    assert!(p.db.read(|c| repo::settle::require_fresh(c, p.ws)).is_err());
}

// ─────────────────────────────────────────────── Excel

#[test]
fn 품의_Excel을_만들고_다시_읽으면_숫자가_숫자로_들어_있다() {
    use crate::excel::read;

    let p = P::new();
    for g in 1..=6 {
        p.student(g, 9, &format!("{g}학년생"));
    }
    여러_상황(&p);

    let dir = tmp_dir("proposal");
    let pr = p.proposal(강사료);
    let made = crate::excel::admin::write_proposal(&pr, &dir).unwrap();
    let path = PathBuf::from(&made.path);
    assert!(path.exists());
    assert!(made.name.contains("품의_강사료"));
    assert!(made.name.contains("2026학년도"));

    let sheet = read::read_first_sheet(&path).unwrap();
    // 머리글이 한글로 제대로 들어갔는가
    assert_eq!(sheet.headers[0], "부서명");
    assert_eq!(sheet.headers[1], "수익자(1,2,4,5,6학년)");
    assert_eq!(sheet.headers[2], "3학년 초과금");
    assert_eq!(sheet.headers[3], "3학년 지원금");
    assert_eq!(sheet.headers[4], "자유수강권");
    assert_eq!(sheet.headers[5], "합계");

    // 마지막 줄이 합계 행이고, 금액이 숫자로 읽힌다
    let last = sheet.rows.last().unwrap();
    assert_eq!(last.1[0], "합계");
    let total: i64 = read::parse_amount(&last.1[5]).unwrap();
    assert_eq!(total, pr.total.total);

    // 부서 줄도 숫자로 읽히고 행 합계가 맞는다
    for (_, cells) in sheet.rows.iter().take(sheet.rows.len() - 1) {
        let v: Vec<i64> = (1..=4)
            .map(|i| read::parse_amount(&cells[i]).unwrap())
            .collect();
        let t: i64 = read::parse_amount(&cells[5]).unwrap();
        assert_eq!(v.iter().sum::<i64>(), t, "{} 행", cells[0]);
    }
}

#[test]
fn 수익자_Excel을_만든다() {
    use crate::excel::read;

    let p = P::new();
    여러_상황(&p);
    let dir = tmp_dir("self-pay");
    let rows = p
        .db
        .read(|c| repo::settle::self_pay_rows(c, p.ws, &p.items))
        .unwrap();
    let made =
        crate::excel::admin::write_self_pay(&rows, &p.items, &["2026학년도", "4월"], &dir).unwrap();

    let sheet = read::read_first_sheet(&PathBuf::from(&made.path)).unwrap();
    assert_eq!(sheet.headers[0], "학년");
    assert_eq!(sheet.headers[4], "부서");
    assert_eq!(sheet.headers[5], "강사료");
    assert_eq!(sheet.headers[9], "합계");
    assert_eq!(sheet.headers[10], "발생원인");
    assert_eq!(sheet.rows.len(), rows.len());

    // 내부 코드가 아니라 한글 문구가 들어 있어야 한다
    for (_, cells) in &sheet.rows {
        let origin = &cells[10];
        assert!(
            origin.contains("수익자") || origin.contains("소진"),
            "발생원인이 사람 말이어야 한다: {origin}"
        );
        assert!(!origin.contains("PLAIN") && !origin.contains("EXHAUSTED"));
    }
}

#[test]
fn 이용권_Excel에_초과금과_지원금_스냅샷이_들어간다() {
    use crate::excel::read;

    let p = P::new();
    여러_상황(&p);
    let dir = tmp_dir("voucher");
    let rows = p
        .db
        .read(|c| repo::settle::program_rows(c, p.ws, Program::Voucher, &p.items))
        .unwrap();
    let made =
        crate::excel::admin::write_program(&rows, &p.items, "방과후이용권", true, &["4월"], &dir)
            .unwrap();

    let sheet = read::read_first_sheet(&PathBuf::from(&made.path)).unwrap();
    assert!(sheet.headers.contains(&"사용 합계".to_string()));
    assert!(sheet.headers.contains(&"초과 합계".to_string()));
    assert!(sheet.headers.contains(&"이월액".to_string()));
    assert!(sheet.headers.contains(&"연간 잔액".to_string()));
    assert_eq!(sheet.rows.len(), rows.len());

    let i = sheet.headers.iter().position(|h| h == "사용 합계").unwrap();
    let total: i64 = sheet
        .rows
        .iter()
        .map(|(_, c)| read::parse_amount(&c[i]).unwrap())
        .sum();
    assert_eq!(total, rows.iter().map(|r| r.used_total).sum::<i64>());
}

#[test]
fn 자유수강권_Excel에는_초과금_열이_없다() {
    use crate::excel::read;

    let p = P::new();
    여러_상황(&p);
    let dir = tmp_dir("free");
    let rows = p
        .db
        .read(|c| repo::settle::program_rows(c, p.ws, Program::FreeVoucher, &p.items))
        .unwrap();
    let made =
        crate::excel::admin::write_program(&rows, &p.items, "자유수강권", false, &["4월"], &dir)
            .unwrap();

    let sheet = read::read_first_sheet(&PathBuf::from(&made.path)).unwrap();
    assert!(sheet.headers.contains(&"사용 합계".to_string()));
    assert!(
        !sheet.headers.iter().any(|h| h.starts_with("초과")),
        "자유수강권 초과분은 수익자 탭으로 간다"
    );
}

#[test]
fn 맞지_않는_품의는_파일을_만들지_않는다() {
    let p = P::new();
    여러_상황(&p);
    let dir = tmp_dir("unbalanced");

    let mut pr = p.proposal(강사료);
    pr.balanced = false; // 검증 실패 상황을 흉내낸다
    let err = crate::excel::admin::write_proposal(&pr, &dir).unwrap_err();
    assert!(err.message.contains("맞지 않아"));
    assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "파일이 없어야 한다");
}
