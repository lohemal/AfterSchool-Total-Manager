//! 부서 일괄 · 학생 개별 추가징수·환불 시험 (v0.1.9).
//!
//! ## 가장 중요한 것 — 한 번만 반영된다
//!
//! 학생이 최종으로 내는 돈은 **charge 하나**다. 정산·학생별 징수 내역·품의는
//! charge 만 읽고 추가징수·환불 기록은 읽지 않는다. 그래서
//!
//! ```text
//! 35,000 → 30,000 + 환불 5,000      최종 30,000   (25,000 이 아니다)
//! 35,000 → 40,000 + 추가징수 5,000  최종 40,000   (45,000 이 아니다)
//! ```
//!
//! 이 파일의 시험은 이 두 줄이 **정산 · 징수 내역 · 추가징수·환불 명단 · [현재
//! 금액 반영]** 어디에서도 무너지지 않는지 본다.

use crate::db::Db;
use crate::model::{
    AdjustmentFilter, CostItem, DeltaRegister, DepartmentInput, EnrollmentInput, Fee, FeePick,
    StudentInput, WorkspaceInput,
};
use crate::repo;
use crate::repo::adjustment::{
    ADDITIONAL, APPLY, REFUND, SRC_CANCEL, SRC_DEPT_BATCH, SRC_ENROLL, SRC_INDIVIDUAL,
};

const 강사료: &str = "INSTRUCTOR";

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

fn 등록(note: &str) -> DeltaRegister {
    DeltaRegister {
        register: true,
        occurred_on: Some("2026-10-14".into()),
        note: Some(note.into()),
    }
}

fn 금액만() -> DeltaRegister {
    DeltaRegister {
        register: false,
        occurred_on: None,
        note: None,
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
        let items = db.read(repo::cost_items).unwrap();
        let ws = db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    year,
                    &WorkspaceInput {
                        name: "2026년 10월".into(),
                        start_date: "2026-10-01".into(),
                        end_date: "2026-10-31".into(),
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

    fn dept(&self, name: &str, amount: i64) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some("A".into()),
                        teacher: None,
                        days: Some("화".into()),
                        note: None,
                        fees: vec![fee(강사료, amount)],
                        capacity: None,
                        allowed_grades: Vec::new(),
                        custom_sessions: 0,
                    },
                )
            })
            .unwrap()
    }

    fn set_base(&self, dept: i64, name: &str, amount: i64) {
        self.db
            .write(|c| {
                repo::department::update(
                    c,
                    dept,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some("A".into()),
                        teacher: None,
                        days: Some("화".into()),
                        note: None,
                        fees: vec![fee(강사료, amount)],
                        capacity: None,
                        allowed_grades: Vec::new(),
                        custom_sessions: 0,
                    },
                )
            })
            .unwrap();
    }

    fn student(&self, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade: 3,
                        class_no: "1".into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, s: i64, d: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
                    &EnrollmentInput {
                        student_id: s,
                        department_id: d,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap()
    }

    /// 한 부서에 n 명을 넣는다.
    fn fill(&self, d: i64, n: i64) -> Vec<i64> {
        (1..=n)
            .map(|i| {
                let s = self.student(i, &format!("학생{i}"));
                self.enroll(s, d)
            })
            .collect()
    }

    fn charge(&self, e: i64) -> i64 {
        self.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COALESCE(SUM(amount),0) FROM charge WHERE enrollment_id = ?1",
                    rusqlite::params![e],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn 한개(&self, sql: &str) -> i64 {
        self.db
            .read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    /// 추가징수·환불 명단의 (환불 합계, 추가징수 합계).
    fn 명단(&self) -> (i64, i64) {
        let v = self
            .db
            .read(|c| repo::adjustment::view(c, self.ws, &AdjustmentFilter::default(), &self.items))
            .unwrap();
        (v.refund.total, v.additional.total)
    }

    /// 정산을 다시 만들어 배분 합계를 읽는다 — 정산이 무엇을 읽는지 본다.
    fn 정산합(&self) -> i64 {
        self.db.write(|c| repo::settle::generate(c, self.ws)).unwrap();
        self.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COALESCE(SUM(a.amount),0) FROM settlement_alloc a
                       JOIN settlement s ON s.id = a.settlement_id
                      WHERE s.workspace_id = ?1 AND s.is_latest = 1",
                    rusqlite::params![self.ws],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn 징수내역합(&self) -> i64 {
        self.db
            .read(|c| {
                let r = repo::fee_report::build(c, self.ws, &self.items, &Default::default())?;
                Ok(r.rows.iter().map(|x| x.total).sum())
            })
            .unwrap()
    }

    fn 개별(&self, e: i64, amount: i64, reg: DeltaRegister) -> crate::error::AppResult<Vec<i64>> {
        self.db.write(|c| {
            repo::adjustment::update_fees_with_delta(
                c,
                e,
                &[fee(강사료, amount)],
                "수정",
                &reg,
                &self.items,
            )
        })
    }

    fn 일괄(
        &self,
        dept: i64,
        picks: &[i64],
        reg: DeltaRegister,
    ) -> crate::error::AppResult<crate::model::BatchResult> {
        let picks: Vec<FeePick> = picks
            .iter()
            .map(|e| FeePick {
                enrollment_id: *e,
                item_code: 강사료.into(),
            })
            .collect();
        self.db.write(|c| {
            repo::adjustment::create_batch(c, self.ws, dept, &picks, &reg, &self.items)
        })
    }
}

// ═══════════════════════════════════════════════ 사례 1 · 2 · 3

#[test]
fn 사례1_단순_정정은_금액만_바뀌고_환불이_없다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];

    let ids = f.개별(e, 30_000, 금액만()).unwrap();

    assert!(ids.is_empty(), "금액만 고쳤는데 기록이 생겼다");
    assert_eq!(f.charge(e), 30_000);
    assert_eq!(f.명단(), (0, 0), "환불 명단에 올랐다");
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 0);
    assert_eq!(f.정산합(), 30_000);
}

#[test]
fn 사례2_실제_환불은_최종_30000원이고_25000원이_아니다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];

    let ids = f.개별(e, 30_000, 등록("강사 사정 휴강")).unwrap();
    assert_eq!(ids.len(), 1);

    // 명단에 5,000
    assert_eq!(f.명단(), (5_000, 0));
    // 최종 실질 부담은 charge 하나 — 어디에서 읽어도 30,000
    assert_eq!(f.charge(e), 30_000);
    assert_eq!(f.정산합(), 30_000, "정산이 환불을 한 번 더 뺐다 (25,000)");
    assert_eq!(f.징수내역합(), 30_000, "징수 내역이 환불을 한 번 더 뺐다");

    // 사유가 남는다
    let note: String = f
        .db
        .read(|c| Ok(c.query_row("SELECT note FROM billing_adjustment", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(note, "강사 사정 휴강");
}

#[test]
fn 사례3_실제_추가징수는_최종_40000원이고_45000원이_아니다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];

    f.개별(e, 40_000, 등록("재료비 조정")).unwrap();

    assert_eq!(f.명단(), (0, 5_000), "추가징수가 차액 5,000 이 아니다");
    assert_eq!(f.charge(e), 40_000);
    assert_eq!(f.정산합(), 40_000, "정산이 추가징수를 한 번 더 더했다 (45,000)");
    assert_eq!(f.징수내역합(), 40_000);
}

// ═══════════════════════════════════════════════ [현재 금액 반영] 에서도 부풀지 않는다

/// v0.1.8 코드에서 실제로 잰 위험 — 차액 추가징수를 '전액' 공식으로 다시 잡으면
/// 5,000 이 40,000 으로 부푼다. 발생 유형으로 공식을 가른 뒤에는 차액으로 남는다.
#[test]
fn 차액_추가징수는_현재_금액_반영에서도_차액으로_남는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 40_000, 등록("재료비 조정")).unwrap()[0];

    // 사람이 '금액만' 다시 고쳐 원본이 달라졌다 → 확인 필요
    f.개별(e, 41_000, 금액만()).unwrap();
    f.db.write(|c| repo::adjustment::confirm(c, &[id], APPLY)).unwrap();

    // 기준 35,000 에서 41,000 이 되었으니 차액은 6,000 이다 — 41,000 이 아니다
    assert_eq!(f.명단(), (0, 6_000), "차액 추가징수가 전액으로 부풀었다");
}

#[test]
fn 차액_환불도_현재_금액_반영에서_차액으로_다시_잡힌다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강")).unwrap()[0];

    f.개별(e, 32_000, 금액만()).unwrap(); // 환불액을 3,000 으로 줄여 바로잡는다
    f.db.write(|c| repo::adjustment::confirm(c, &[id], APPLY)).unwrap();
    assert_eq!(f.명단(), (3_000, 0));
}

// ═══════════════════════════════════════════════ 여러 기록

#[test]
fn 한_학생의_여러_기록이_각각_남고_합이_맞는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];

    f.개별(e, 30_000, 등록("10/10 휴강 환불")).unwrap(); // -5,000
    f.개별(e, 27_000, 등록("10/25 개인 사유")).unwrap(); // -3,000
    f.개별(e, 29_000, 등록("재료 추가")).unwrap(); //       +2,000

    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 3);
    assert_eq!(f.명단(), (8_000, 2_000));
    // 35,000 - 8,000 + 2,000
    assert_eq!(f.charge(e), 29_000);
    assert_eq!(f.정산합(), 29_000);

    // 나중 기록이 앞 기록의 변화를 '설명'했으므로, 앞 기록이 확인 필요로 뜨지 않는다
    let v = f
        .db
        .read(|c| repo::adjustment::view(c, f.ws, &AdjustmentFilter::default(), &f.items))
        .unwrap();
    assert_eq!(v.needs_check_all, 0, "이미 설명한 변화가 확인 필요로 남았다");
}

/// 두 번째 위험 — 앞 기록에서 [현재 금액 반영]을 누르면 뒤 기록의 변화까지 다시
/// 셈해 11,000 이 되던 것을 막는다.
#[test]
fn 앞_기록이_뒤_기록의_변화를_다시_세지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    let 첫째 = f.개별(e, 30_000, 등록("10/10 휴강")).unwrap()[0];
    f.개별(e, 27_000, 등록("10/25 개인")).unwrap();

    // 누가 앞 기록에서 반영을 눌러도
    f.db.write(|c| repo::adjustment::confirm(c, &[첫째], APPLY)).unwrap();
    // 앞 기록은 확인한 값(27,000)이 지금과 같아 그대로 5,000 이다
    assert_eq!(f.명단(), (8_000, 0), "앞 기록이 뒤 변화를 다시 세어 11,000 이 되었다");
}

#[test]
fn 설명하지_않은_변화는_여전히_확인_필요로_남는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    f.개별(e, 30_000, 등록("휴강")).unwrap();
    // 기록 없이 금액만 바꾼다 — 아무도 이 변화를 설명하지 않았다
    f.개별(e, 31_000, 금액만()).unwrap();
    let v = f
        .db
        .read(|c| repo::adjustment::view(c, f.ws, &AdjustmentFilter::default(), &f.items))
        .unwrap();
    assert_eq!(v.needs_check_all, 1, "설명하지 않은 변화를 조용히 덮었다");
}

#[test]
fn 중도_추가_학생의_인상분도_한_번만_셈한다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let s = f.student(1, "김민수");
    // 중도 추가 — 전액 35,000 을 추가징수로 굳힌다 (기존 v0.1.5 길)
    let e = f
        .db
        .write(|c| {
            let e = repo::enrollment::create(
                c,
                f.ws,
                &EnrollmentInput {
                    student_id: s,
                    department_id: d,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )?;
            repo::adjustment::create_additional(
                c,
                e,
                &crate::model::AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )?;
            Ok(e)
        })
        .unwrap();
    assert_eq!(f.명단(), (0, 35_000));

    // 그 학생의 금액을 40,000 으로 올리며 차액 5,000 을 추가징수로 등록
    f.개별(e, 40_000, 등록("재료비 조정")).unwrap();
    // 35,000(전액) + 5,000(차액) = 40,000 — 학생이 낼 돈과 같다
    assert_eq!(f.명단(), (0, 40_000), "인상분이 두 번 셈되었다");

    // 중도 추가 기록에서 [현재 금액 반영]을 눌러도 부풀지 않는다
    let enroll_id: i64 = f.한개(&format!(
        "SELECT id FROM billing_adjustment WHERE source = '{SRC_ENROLL}'"
    ));
    f.db.write(|c| repo::adjustment::confirm(c, &[enroll_id], APPLY)).unwrap();
    assert_eq!(f.명단(), (0, 40_000));
}

#[test]
fn 취소한_수강_건에는_차액을_따로_등록하지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    f.db.write(|c| repo::enrollment::cancel(c, e, None, "중도 포기", &f.items))
        .unwrap();

    let err = f.개별(e, 20_000, 등록("환불")).unwrap_err();
    assert!(format!("{err:?}").contains("두 번 셈"), "{err:?}");
    // 막혔으니 금액도 그대로다 (한 트랜잭션)
    assert_eq!(f.charge(e), 35_000);
}

#[test]
fn 사유_없이는_등록하지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    let err = f
        .개별(
            e,
            30_000,
            DeltaRegister {
                register: true,
                occurred_on: None,
                note: Some("   ".into()),
            },
        )
        .unwrap_err();
    assert!(format!("{err:?}").contains("사유"), "{err:?}");
    assert_eq!(f.charge(e), 35_000, "사유가 없어 막혔는데 금액이 바뀌었다");
}

#[test]
fn 같은_요청이_두_번_와도_기록은_하나다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    f.개별(e, 30_000, 등록("휴강")).unwrap();
    // 저장 단추를 연달아 눌렀다 — 두 번째는 이미 30,000 이라 차액이 0
    let second = f.개별(e, 30_000, 등록("휴강")).unwrap();
    assert!(second.is_empty());
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 1);
    assert_eq!(f.명단(), (5_000, 0));
}

// ═══════════════════════════════════════════════ 부서 일괄

#[test]
fn 부서_일괄_환불_18명은_90000원이고_한_묶음이다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 18);
    f.set_base(d, "미술", 30_000);

    let r = f.일괄(d, &es, 등록("강사 사정으로 1회 휴강")).unwrap();
    assert_eq!(r.refunds, 18);
    assert_eq!(r.refund_total, 90_000);
    assert_eq!(r.additionals, 0);
    let batch = r.batch_id.expect("묶음이 없다");

    // 모두 같은 묶음 · 같은 유형
    assert_eq!(
        f.한개(&format!(
            "SELECT COUNT(*) FROM billing_adjustment WHERE batch_id = {batch} AND source = '{SRC_DEPT_BATCH}'"
        )),
        18
    );
    // 묶음 목록이 합계를 딸린 기록에서 낸다
    let bs = f.db.read(|c| repo::adjustment::batches(c, f.ws)).unwrap();
    assert_eq!(bs.len(), 1);
    assert_eq!(bs[0].students, 18);
    assert_eq!(bs[0].refund_total, 90_000);
    assert_eq!(bs[0].note, "강사 사정으로 1회 휴강");

    // 이중 반영 없음 — 학생마다 30,000
    assert!(es.iter().all(|e| f.charge(*e) == 30_000));
    assert_eq!(f.정산합(), 18 * 30_000);
    assert_eq!(f.명단(), (90_000, 0));
}

#[test]
fn 한_명을_빼면_17건_85000원이고_뺀_학생은_그대로다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 18);
    f.set_base(d, "미술", 30_000);

    let 뺀 = es[0];
    let r = f.일괄(d, &es[1..], 등록("휴강")).unwrap();
    assert_eq!(r.refunds, 17);
    assert_eq!(r.refund_total, 85_000);

    assert_eq!(f.charge(뺀), 35_000, "뺀 학생의 금액이 바뀌었다");
    assert_eq!(
        f.한개(&format!(
            "SELECT COUNT(*) FROM billing_adjustment WHERE enrollment_id = {뺀}"
        )),
        0,
        "뺀 학생에게 기록이 생겼다"
    );
}

#[test]
fn 부서_일괄_추가징수_10명은_30000원이다() {
    let f = F::new();
    let d = f.dept("로봇", 35_000);
    let es = f.fill(d, 10);
    f.set_base(d, "로봇", 38_000);

    let r = f.일괄(d, &es, 등록("재료비 조정")).unwrap();
    assert_eq!(r.additionals, 10);
    assert_eq!(r.additional_total, 30_000);
    assert!(es.iter().all(|e| f.charge(*e) == 38_000));
    assert_eq!(f.정산합(), 10 * 38_000, "추가징수가 정산에 한 번 더 더해졌다");
}

#[test]
fn 부서_일괄은_취소한_학생을_넣지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 3);
    f.db.write(|c| repo::enrollment::cancel(c, es[0], None, "포기", &f.items))
        .unwrap();
    f.set_base(d, "미술", 30_000);

    // 화면이 실수로 취소자까지 보내도
    let r = f.일괄(d, &es, 등록("휴강")).unwrap();
    assert_eq!(r.refunds, 2, "취소한 학생이 일괄 대상에 들어갔다");
    assert_eq!(f.charge(es[0]), 35_000, "취소자의 최종 금액이 바뀌었다");
}

#[test]
fn 부서_일괄_금액만은_기록을_남기지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 5);
    f.set_base(d, "미술", 30_000);

    let r = f.일괄(d, &es, 금액만()).unwrap();
    assert!(r.batch_id.is_none());
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_batch"), 0);
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 0);
    assert!(es.iter().all(|e| f.charge(*e) == 30_000));
}

#[test]
fn 부서_일괄을_두_번_눌러도_묶음은_하나다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 5);
    f.set_base(d, "미술", 30_000);
    f.일괄(d, &es, 등록("휴강")).unwrap();
    let again = f.일괄(d, &es, 등록("휴강")).unwrap();

    assert!(again.batch_id.is_none(), "두 번째에 빈 묶음이 생겼다");
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_batch"), 1);
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 5);
}

/// 18명 가운데 하나라도 실패하면 아무것도 남지 않아야 한다.
///
/// 일괄 처리는 `Db::write` 한 번 안에서 돈다. 그 안에서 오류가 나면 금액·기록·
/// 묶음이 함께 되돌려지는지 본다.
#[test]
fn 일괄이_중간에_실패하면_반쪽이_남지_않는다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 18);
    f.set_base(d, "미술", 30_000);
    let picks: Vec<FeePick> = es
        .iter()
        .map(|e| FeePick {
            enrollment_id: *e,
            item_code: 강사료.into(),
        })
        .collect();

    let r: crate::error::AppResult<()> = f.db.write(|c| {
        repo::adjustment::create_batch(c, f.ws, d, &picks, &등록("휴강"), &f.items)?;
        // 다 만든 뒤 마지막에 실패하는 경우
        Err(crate::error::AppError::invalid("일부러 실패"))
    });
    assert!(r.is_err());

    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_batch"), 0, "묶음이 남았다");
    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 0, "기록이 남았다");
    assert!(
        es.iter().all(|e| f.charge(*e) == 35_000),
        "금액이 반쯤 바뀐 채 남았다"
    );
}


// ═══════════════════════════════════════════════ 발생 유형

#[test]
fn 발생_유형이_구분되고_필터가_된다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 3);

    // 수강 취소 환불 (기존 길)
    let before = f.db.read(|c| repo::enrollment::get(c, es[0], &f.items)).unwrap();
    f.db.write(|c| {
        repo::enrollment::cancel(c, es[0], Some(&[fee(강사료, 20_000)]), "포기", &f.items)?;
        repo::adjustment::create_refund(
            c,
            es[0],
            &before,
            &crate::model::AdjustmentInput {
                occurred_on: None,
                note: Some("10/15 취소".into()),
            },
            &f.items,
        )
    })
    .unwrap();
    // 개별 조정
    f.개별(es[1], 32_000, 등록("재료비")).unwrap();
    // 부서 일괄 (남은 ACTIVE 둘 가운데 es[2] 만)
    f.set_base(d, "미술", 30_000);
    f.일괄(d, &[es[2]], 등록("휴강")).unwrap();

    let by = |src: &str| -> i64 {
        f.한개(&format!(
            "SELECT COUNT(*) FROM billing_adjustment WHERE source = '{src}'"
        ))
    };
    assert_eq!(by(SRC_CANCEL), 1);
    assert_eq!(by(SRC_INDIVIDUAL), 1);
    assert_eq!(by(SRC_DEPT_BATCH), 1);

    let only = |src: &str| {
        f.db.read(|c| {
            repo::adjustment::view(
                c,
                f.ws,
                &AdjustmentFilter {
                    source: Some(src.into()),
                    ..Default::default()
                },
                &f.items,
            )
        })
        .unwrap()
    };
    assert_eq!(only(SRC_CANCEL).refund.count, 1);
    assert_eq!(only(SRC_CANCEL).refund.total, 15_000);
    assert_eq!(only(SRC_DEPT_BATCH).refund.total, 5_000);
    assert_eq!(only(SRC_INDIVIDUAL).refund.total, 3_000);
}

#[test]
fn 알_수_없는_발생_유형으로는_거르지_않는다() {
    let f = F::new();
    let r = f.db.read(|c| {
        repo::adjustment::view(
            c,
            f.ws,
            &AdjustmentFilter {
                source: Some("x' OR 1=1 --".into()),
                ..Default::default()
            },
            &f.items,
        )
    });
    assert!(r.is_err(), "알 수 없는 값이 SQL 에 들어갔다");
}

// ═══════════════════════════════════════════════ 정산 규칙

/// 기록 자체는 정산을 낡게 하지 않는다(005 부터의 규칙). 낡음은 금액을 고친 데서
/// 온다 — 학생이 낼 돈이 실제로 바뀌었기 때문이다. 그 규칙을 둘 다 그대로 지킨다.
#[test]
fn 정산은_금액이_바뀌면_낡고_재정산하면_새_금액이_들어간다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    assert_eq!(f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state, "FRESH");

    f.개별(e, 30_000, 등록("휴강")).unwrap();
    assert_ne!(
        f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state,
        "FRESH",
        "금액이 바뀌었는데 정산이 낡지 않았다"
    );
    assert_eq!(f.정산합(), 30_000, "재정산에 환불이 한 번 더 빠졌다");
}

#[test]
fn 기록만_만들고_금액이_그대로면_정산은_그대로다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let s = f.student(1, "김민수");
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();

    // 중도 추가 — 수강을 넣는 것 자체가 정산을 낡게 하므로 그 뒤에 다시 만든다
    let e = f.enroll(s, d);
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    assert_eq!(f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state, "FRESH");

    // 그 수강 건에 추가징수 '기록'만 더한다 — 금액은 그대로
    f.db.write(|c| {
        repo::adjustment::create_additional(
            c,
            e,
            &crate::model::AdjustmentInput {
                occurred_on: None,
                note: None,
            },
            &f.items,
        )
    })
    .unwrap();
    assert_eq!(
        f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state,
        "FRESH",
        "기록만 만들었는데 정산이 낡음이 되었다"
    );
}

#[test]
fn 기존_취소_환불과_중도_추가는_예전_공식_그대로다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 1);
    let before = f.db.read(|c| repo::enrollment::get(c, es[0], &f.items)).unwrap();
    let id = f
        .db
        .write(|c| {
            repo::enrollment::cancel(c, es[0], Some(&[fee(강사료, 20_000)]), "포기", &f.items)?;
            repo::adjustment::create_refund(
                c,
                es[0],
                &before,
                &crate::model::AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )
        })
        .unwrap();
    assert_eq!(f.명단(), (15_000, 0));

    // 취소자의 최종 금액을 고치고 [현재 금액 반영] — 35,000 - 18,000
    f.db.write(|c| {
        repo::enrollment::update_fees(c, es[0], &[fee(강사료, 18_000)], "정정", &f.items)
    })
    .unwrap();
    f.db.write(|c| repo::adjustment::confirm(c, &[id], APPLY)).unwrap();
    assert_eq!(f.명단(), (17_000, 0), "취소 환불의 공식이 바뀌었다");
}

/// 공식 한 곳 — 두 갈래를 표로 고정한다.
///
/// 인자: (구분, 차액기록있음, 금액, 기준, 확인값, 지금)
#[test]
fn 다시_잡는_공식표() {
    use repo::adjustment::recompute as r;

    // ── 차액 기록이 없는 수강 건: v0.1.8 공식 그대로
    // 취소 환불 = 기준 - 지금
    assert_eq!(r(REFUND, false, 15_000, 35_000, 20_000, 18_000), 17_000);
    // 중도 추가 = 지금 (전액)
    assert_eq!(r(ADDITIONAL, false, 35_000, 0, 35_000, 40_000), 40_000);

    // ── 차액 기록이 있는 수강 건: 설명되지 않은 변화분만
    // 이미 설명된 변화(확인값 = 지금)는 그대로
    assert_eq!(r(REFUND, true, 5_000, 35_000, 27_000, 27_000), 5_000);
    assert_eq!(r(ADDITIONAL, true, 35_000, 0, 40_000, 40_000), 35_000);
    // 환불 = 금액 + (확인값 - 지금)
    assert_eq!(r(REFUND, true, 5_000, 35_000, 30_000, 32_000), 3_000);
    // 추가징수 = 금액 + (지금 - 확인값)
    assert_eq!(r(ADDITIONAL, true, 5_000, 35_000, 40_000, 41_000), 6_000);

    // 기록이 하나뿐이면 두 갈래가 같은 값을 낸다 (금액 = 기준 - 확인값 이므로)
    assert_eq!(
        r(REFUND, true, 5_000, 35_000, 30_000, 32_000),
        r(REFUND, false, 5_000, 35_000, 30_000, 32_000)
    );
}

/// 앞뒤 기록이 함께 '확인 필요'일 때 둘 다 [현재 금액 반영]을 눌러도, 그 한 번의
/// 변화는 한 기록만 가져간다.
#[test]
fn 여러_기록이_함께_반영되어도_변화는_한_번만_셈한다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let e = f.fill(d, 1)[0];
    let a = f.개별(e, 30_000, 등록("10/10 휴강")).unwrap()[0];
    let b = f.개별(e, 27_000, 등록("10/25 개인")).unwrap()[0];
    assert_eq!(f.명단(), (8_000, 0));

    // 기록 없이 1,000원 올린다 — 아무도 설명하지 않은 변화
    f.개별(e, 28_000, 금액만()).unwrap();
    // 둘 다 반영
    f.db.write(|c| repo::adjustment::confirm(c, &[a, b], APPLY)).unwrap();

    // 35,000 → 28,000 이니 돌려줄 돈은 7,000 — 9,000 (1,000 을 두 번 셈) 이 아니다
    assert_eq!(f.명단(), (7_000, 0), "같은 변화를 두 기록이 함께 가져갔다");
    assert_eq!(f.charge(e), 28_000);
}

/// v0.1.8 의 취소 환불·중도 추가는 차액 기록이 없으므로 예전 공식 그대로여야 한다.
/// 같은 수강 건에 두 기록이 함께 있는 경우(중도 추가 뒤 취소)도 그렇다.
#[test]
fn 차액_기록이_없는_수강_건은_v018_과_똑같이_반영된다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let s = f.student(1, "김민수");
    // 중도 추가(전액) 뒤 취소(환불) — v0.1.8 에서도 있던 조합
    let (e, add) = f
        .db
        .write(|c| {
            let e = repo::enrollment::create(
                c,
                f.ws,
                &EnrollmentInput {
                    student_id: s,
                    department_id: d,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )?;
            let add = repo::adjustment::create_additional(
                c,
                e,
                &crate::model::AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )?;
            Ok((e, add))
        })
        .unwrap();
    let before = f.db.read(|c| repo::enrollment::get(c, e, &f.items)).unwrap();
    let refund = f
        .db
        .write(|c| {
            repo::enrollment::cancel(c, e, Some(&[fee(강사료, 20_000)]), "포기", &f.items)?;
            repo::adjustment::create_refund(
                c,
                e,
                &before,
                &crate::model::AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )
        })
        .unwrap();

    f.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(강사료, 18_000)], "정정", &f.items)
    })
    .unwrap();
    f.db.write(|c| repo::adjustment::confirm(c, &[add, refund], APPLY))
        .unwrap();

    // v0.1.8: 중도 추가 = 지금(18,000), 취소 환불 = 35,000 - 18,000 = 17,000
    assert_eq!(f.명단(), (17_000, 18_000), "v0.1.8 공식에서 벗어났다");
}

#[test]
fn 한_학생의_항목이_엇갈리면_환불과_추가징수가_각각_생긴다() {
    let f = F::new();
    let d = f
        .db
        .write(|c| {
            repo::department::create(
                c,
                f.ws,
                &DepartmentInput {
                    name: "미술".into(),
                    class_name: Some("A".into()),
                    teacher: None,
                    days: Some("화".into()),
                    note: None,
                    fees: vec![fee(강사료, 35_000), fee("MATERIAL", 10_000)],
                    capacity: None,
                    allowed_grades: Vec::new(),
                    custom_sessions: 0,
                },
            )
        })
        .unwrap();
    let e = f.fill(d, 1)[0];
    // 강사료 -5,000 · 재료비 +3,000
    f.db.write(|c| {
        repo::adjustment::update_fees_with_delta(
            c,
            e,
            &[fee(강사료, 30_000), fee("MATERIAL", 13_000)],
            "조정",
            &등록("강사료 인하·재료비 인상"),
            &f.items,
        )
    })
    .unwrap();

    assert_eq!(f.한개("SELECT COUNT(*) FROM billing_adjustment"), 2);
    assert_eq!(f.명단(), (5_000, 3_000));
    assert_eq!(f.charge(e), 43_000); // 30,000 + 13,000

    // 반대 방향 항목이 [현재 금액 반영]에서 음수로 다시 잡히지 않는다
    let ids: Vec<i64> = f
        .db
        .read(|c| {
            let mut st = c.prepare("SELECT id FROM billing_adjustment")?;
            let v = st
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
            Ok(v)
        })
        .unwrap();
    f.db.write(|c| repo::adjustment::confirm(c, &ids, APPLY)).unwrap();
    assert_eq!(f.명단(), (5_000, 3_000));
}

// ═══════════════════════════════════════════════ Excel

/// 부서 일괄 환불과 개별 추가징수가 기존 추가징수·환불 Excel 에 그대로 실린다.
/// 화면의 금액과 파일의 금액이 같아야 하고, 새로 붙인 '발생 유형'이 맨 끝에 있다.
#[test]
fn 새_기록이_기존_추가징수_환불_엑셀에_실린다() {
    let f = F::new();
    let d = f.dept("미술", 35_000);
    let es = f.fill(d, 3);
    f.set_base(d, "미술", 30_000);
    f.일괄(d, &es[..2], 등록("강사 사정 휴강")).unwrap(); // 환불 5,000 × 2
    f.개별(es[2], 38_000, 등록("재료비 조정")).unwrap(); //  추가징수 3,000

    let view = f
        .db
        .read(|c| repo::adjustment::view(c, f.ws, &AdjustmentFilter::default(), &f.items))
        .unwrap();
    // 확인 필요가 남으면 Excel 이 막힌다 — 새 기록이 그런 상태를 만들지 않아야 한다
    assert_eq!(view.additional.needs_check + view.refund.needs_check, 0);

    let dir = std::env::temp_dir().join("afterschool-delta-xl");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = crate::excel::adjustment::write_adjustments(&view, &f.items, &["2026학년도"], "", &dir)
        .expect("Excel 을 만들지 못했다");
    let path = std::path::PathBuf::from(&out.path);

    // 상세 장 — 마지막 장이다
    let mut sheet = None;
    for i in 0..4 {
        if let Ok(s) = crate::excel::read::read_sheet_at(&path, i) {
            if s.col("발생 유형").is_some() {
                sheet = Some(s);
            }
        }
    }
    let s = sheet.expect("발생 유형 칸이 있는 장이 없다");
    let c_kind = s.require("구분").unwrap();
    let c_total = s.require("합계").unwrap();
    let c_note = s.require("사유").unwrap();
    let c_src = s.require("발생 유형").unwrap();
    assert!(c_src > c_note, "발생 유형이 맨 끝이 아니다 — 앞 칸 자리가 밀렸다");

    let mut 환불 = 0i64;
    let mut 추가 = 0i64;
    let mut 유형: Vec<String> = Vec::new();
    for (_, cells) in &s.rows {
        let total: i64 = cells[c_total].replace(',', "").parse().unwrap_or(0);
        match cells[c_kind].as_str() {
            "환불" => 환불 += total,
            "추가징수" => 추가 += total,
            _ => {}
        }
        유형.push(cells[c_src].clone());
        assert!(!cells[c_note].is_empty(), "사유가 파일에서 빠졌다");
    }
    // 화면(view)과 같은 금액
    assert_eq!(환불, view.refund.total);
    assert_eq!(추가, view.additional.total);
    assert_eq!((환불, 추가), (10_000, 3_000));
    assert!(유형.iter().any(|t| t == "부서일괄"));
    assert!(유형.iter().any(|t| t == "개별조정"));
}
