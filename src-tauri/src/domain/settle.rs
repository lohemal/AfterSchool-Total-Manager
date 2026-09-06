//! 정산 엔진 (설계안 6·7장).
//!
//! **DB에 의존하지 않는 순수 함수다.** 입력도 출력도 평범한 구조체이므로
//! 어떤 경계값이든 DB 없이 시험할 수 있다. 이 파일이 이 프로그램에서 가장
//! 중요한 코드이며, 여기서 1원이 틀리면 회계 문제가 된다.
//!
//! ## 지키는 것
//!
//! 1. **불변식** — 모든 (수강 × 비용항목)에 대해
//!    `Σ 배분액 == charge.amount`. 1원도 사라지거나 생기지 않는다.
//! 2. **나눗셈이 없다** — `min(잔액, 금액)`의 뺄셈만 쓰므로 반올림 오차가
//!    원천적으로 생기지 않는다.
//! 3. **결정적이다** — 같은 입력이면 언제나 같은 출력이다. 정렬 키에 동점이
//!    없도록 `(부서순위, 항목순위, 부서id, 항목코드, 수강id)` 다섯을 쓴다.
//! 4. **제도를 끝까지 순회한다** — 이용권이 소진되었다고 그 자리에서 학부모
//!    부담으로 확정하지 않는다. 적용 가능한 제도를 모두 거친 뒤에 남은 금액의
//!    재원을 정한다.

use std::collections::HashMap;

use crate::domain::{Fund, Origin, Program};

// ─────────────────────────────────────────────── 입력

/// 차감 대상 한 칸 — 수강 하나의 비용항목 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeUnit {
    pub enrollment_id: i64,
    pub department_id: i64,
    pub item_code: String,
    pub amount: i64,
}

/// 제도 하나에 대한 이 학생의 상태.
///
/// `eligible`이 false면 자격이 없는 것이고, true인데 `available`이 0이면
/// 자격은 있으나 이미 다 쓴 것이다. **둘은 다르게 다뤄진다** —
/// 자격이 있으면 남은 금액의 재원이 `VOUCHER_OVER`가 된다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budget {
    pub eligible: bool,
    pub available: i64,
}

impl Budget {
    pub fn none() -> Self {
        Self {
            eligible: false,
            available: 0,
        }
    }
    pub fn of(available: i64) -> Self {
        Self {
            eligible: true,
            available,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StudentInput {
    pub student_id: i64,
    pub charges: Vec<ChargeUnit>,
    pub voucher: Budget,
    pub free_voucher: Budget,
}

impl StudentInput {
    fn budget(&self, program: Program) -> Budget {
        match program {
            Program::Voucher => self.voucher,
            Program::FreeVoucher => self.free_voucher,
        }
    }
}

/// 차감 순서와 제도 순서.
#[derive(Debug, Clone)]
pub struct Config {
    pub program_order: Vec<Program>,
    dept_rank: HashMap<i64, usize>,
    item_rank: HashMap<String, usize>,
}

impl Config {
    /// * `dept_order` — 부서 id를 차감할 차례대로
    /// * `item_order` — 비용항목 코드를 차감할 차례대로
    ///
    /// 목록에 없는 부서·항목은 **맨 뒤**로 간다. 그 안에서는 id·코드 순이라
    /// 결과가 실행할 때마다 달라지지 않는다.
    pub fn new(program_order: Vec<Program>, dept_order: &[i64], item_order: &[String]) -> Self {
        Self {
            program_order,
            dept_rank: dept_order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect(),
            item_rank: item_order
                .iter()
                .enumerate()
                .map(|(i, c)| (c.clone(), i))
                .collect(),
        }
    }

    pub fn dept_rank(&self, id: i64) -> usize {
        self.dept_rank.get(&id).copied().unwrap_or(usize::MAX)
    }

    pub fn item_rank(&self, code: &str) -> usize {
        self.item_rank.get(code).copied().unwrap_or(usize::MAX)
    }
}

// ─────────────────────────────────────────────── 출력

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alloc {
    pub enrollment_id: i64,
    pub department_id: i64,
    pub item_code: String,
    pub fund: Fund,
    pub origin: Origin,
    pub amount: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Used {
    pub voucher: i64,
    pub free_voucher: i64,
}

impl Used {
    fn add(&mut self, program: Program, amount: i64) {
        match program {
            Program::Voucher => self.voucher += amount,
            Program::FreeVoucher => self.free_voucher += amount,
        }
    }

    pub fn of(&self, program: Program) -> i64 {
        match program {
            Program::Voucher => self.voucher,
            Program::FreeVoucher => self.free_voucher,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StudentResult {
    pub allocs: Vec<Alloc>,
    pub used: Used,
}

// ─────────────────────────────────────────────── 엔진

struct Unit<'a> {
    key: (usize, usize, i64, &'a str, i64),
    charge: &'a ChargeUnit,
    remain: i64,
}

/// 한 학생의 이번 작업공간 금액을 재원별로 나눈다.
pub fn settle_student(input: &StudentInput, cfg: &Config) -> StudentResult {
    // 1) 차감 단위를 우선순위대로 한 줄로 세운다. 0원 항목은 배분할 것이 없다.
    let mut units: Vec<Unit> = input
        .charges
        .iter()
        .filter(|c| c.amount > 0)
        .map(|c| Unit {
            key: (
                cfg.dept_rank(c.department_id),
                cfg.item_rank(&c.item_code),
                c.department_id,
                c.item_code.as_str(),
                c.enrollment_id,
            ),
            charge: c,
            remain: c.amount,
        })
        .collect();
    units.sort_by(|a, b| a.key.cmp(&b.key));

    let mut out: Vec<Alloc> = Vec::new();
    let mut used = Used::default();

    // 이 학생이 자격을 가진 제도 가운데 **마지막으로 거친** 것.
    // 남은 금액의 origin이 여기서 정해진다 (아래 3번 참고).
    let mut last_eligible: Option<Program> = None;

    // 2) 제도를 설정된 순서대로 소진한다.
    //    이용권이 끝났다고 여기서 학부모 부담을 확정하지 않는다.
    for program in &cfg.program_order {
        let budget = input.budget(*program);
        if !budget.eligible {
            continue;
        }
        last_eligible = Some(*program);

        let mut left = budget.available.max(0);
        if left == 0 {
            continue; // 자격은 있으나 이미 다 썼다
        }
        for u in units.iter_mut() {
            if left == 0 {
                break;
            }
            let use_amt = left.min(u.remain);
            if use_amt > 0 {
                out.push(Alloc {
                    enrollment_id: u.charge.enrollment_id,
                    department_id: u.charge.department_id,
                    item_code: u.charge.item_code.clone(),
                    fund: program.fund(),
                    origin: Origin::Plain,
                    amount: use_amt,
                });
                u.remain -= use_amt;
                left -= use_amt;
                used.add(*program, use_amt);
            }
        }
    }

    // 3) 모든 제도를 거치고도 남은 금액만 학부모 부담이 된다.
    //
    //    재원(fund) — 이용권 **자격**이 있으면 초과금(VOUCHER_OVER),
    //                 없으면 일반 수익자(SELF_PAY).
    //    사연(origin) — 이 학생이 거친 마지막 제도.
    //                 자격만 있으면(잔액 0이어도) 거친 것으로 본다.
    //
    //    그래서 두 제도를 모두 가진 학생의 남은 금액은
    //    `VOUCHER_OVER` + `FREE_EXHAUSTED`가 된다. 품의에서는 이용권 대상자의
    //    초과금이고, 내역으로는 자유수강권까지 쓴 뒤 남은 것이라는 뜻이다.
    let fund = if input.voucher.eligible {
        Fund::VoucherOver
    } else {
        Fund::SelfPay
    };
    let origin = match last_eligible {
        None => Origin::Plain,
        Some(Program::Voucher) => Origin::VoucherExhausted,
        Some(Program::FreeVoucher) => Origin::FreeExhausted,
    };

    for u in units.iter().filter(|u| u.remain > 0) {
        out.push(Alloc {
            enrollment_id: u.charge.enrollment_id,
            department_id: u.charge.department_id,
            item_code: u.charge.item_code.clone(),
            fund,
            origin,
            amount: u.remain,
        });
    }

    StudentResult { allocs: out, used }
}

/// 불변식 검사 — `Σ 배분액 == charge.amount`가 모든 칸에서 성립하는가.
///
/// 정산을 저장하기 **전에** 부른다. 어긋나면 아무것도 저장하지 않는다.
pub fn verify(charges: &[ChargeUnit], allocs: &[Alloc]) -> Result<(), String> {
    let mut want: HashMap<(i64, &str), i64> = HashMap::new();
    for c in charges {
        *want.entry((c.enrollment_id, c.item_code.as_str())).or_insert(0) += c.amount;
    }
    let mut got: HashMap<(i64, &str), i64> = HashMap::new();
    for a in allocs {
        if a.amount <= 0 {
            return Err(format!(
                "0원 이하 배분이 있습니다 (수강 {}, {})",
                a.enrollment_id, a.item_code
            ));
        }
        *got.entry((a.enrollment_id, a.item_code.as_str())).or_insert(0) += a.amount;
    }
    for (k, v) in &want {
        let g = got.get(k).copied().unwrap_or(0);
        if g != *v {
            return Err(format!(
                "금액이 맞지 않습니다 (수강 {}, {}): 원본 {v}원 / 배분 {g}원",
                k.0, k.1
            ));
        }
    }
    for k in got.keys() {
        if !want.contains_key(k) {
            return Err(format!(
                "원본에 없는 배분이 있습니다 (수강 {}, {})",
                k.0, k.1
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod tests;
