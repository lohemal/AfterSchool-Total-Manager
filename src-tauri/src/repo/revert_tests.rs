//! 차액 기록을 지우면 금액도 함께 되돌아오는가 (v0.1.9 후보 수정).
//!
//! ## 지키려는 것
//!
//! 부서일괄·개별조정 기록은 charge 를 바꾸면서 만들어졌다. 그 기록을 지우면
//! **그 기록의 차액만큼만** 되돌린다 — '처음 금액'으로 돌리는 것이 아니다.
//!
//! ```text
//! 환불 기록 삭제      금액 += 기록 금액
//! 추가징수 기록 삭제  금액 -= 기록 금액
//! ```
//!
//! 기존·중도추가·수강취소 기록은 예전처럼 기록만 지운다.

use crate::db::Db;
use crate::model::{
    AdjustmentFilter, AdjustmentInput, CostItem, DeltaRegister, DepartmentInput, EnrollmentInput,
    Fee, FeePick, StudentInput, WorkspaceInput,
};
use crate::repo;

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

    fn dept(&self, amount: i64) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: "미술".into(),
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

    fn set_base(&self, dept: i64, amount: i64) {
        self.db
            .write(|c| {
                repo::department::update(
                    c,
                    dept,
                    &DepartmentInput {
                        name: "미술".into(),
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

    fn fill(&self, d: i64, n: i64) -> Vec<i64> {
        (1..=n)
            .map(|i| {
                let s = self
                    .db
                    .write(|c| {
                        repo::student::create(
                            c,
                            self.year,
                            &StudentInput {
                                grade: 3,
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
                                department_id: d,
                                fees: Vec::new(),
                                reason: None,
                            },
                            &self.items,
                        )
                    })
                    .unwrap()
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

    fn 개수(&self, sql: &str) -> i64 {
        self.db
            .read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    fn 개별(&self, e: i64, amount: i64, reg: DeltaRegister) -> Vec<i64> {
        self.db
            .write(|c| {
                repo::adjustment::update_fees_with_delta(
                    c,
                    e,
                    &[fee(강사료, amount)],
                    "수정",
                    &reg,
                    &self.items,
                )
            })
            .unwrap()
    }

    fn 지움(&self, ids: &[i64]) -> crate::error::AppResult<usize> {
        self.db.write(|c| repo::adjustment::delete_many(c, ids))
    }

    fn 일괄(&self, dept: i64, es: &[i64], note: &str) -> i64 {
        let picks: Vec<FeePick> = es
            .iter()
            .map(|e| FeePick {
                enrollment_id: *e,
                item_code: 강사료.into(),
            })
            .collect();
        self.db
            .write(|c| {
                repo::adjustment::create_batch(c, self.ws, dept, &picks, &등록(note), &self.items)
            })
            .unwrap()
            .batch_id
            .unwrap()
    }

    fn 명단(&self) -> (i64, i64) {
        let v = self
            .db
            .read(|c| repo::adjustment::view(c, self.ws, &AdjustmentFilter::default(), &self.items))
            .unwrap();
        (v.refund.total, v.additional.total)
    }
}

// ═══════════════════════════════════════════════ 1 · 2 개별 삭제

#[test]
fn 개별_환불을_지우면_35000원으로_돌아온다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    assert_eq!(f.charge(e), 30_000);

    f.지움(&[id]).unwrap();
    assert_eq!(f.charge(e), 35_000, "환불을 지웠는데 금액이 30,000 에 남았다");
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 0);
    assert_eq!(f.명단(), (0, 0));
}

#[test]
fn 개별_추가징수를_지우면_35000원으로_돌아온다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 40_000, 등록("재료비"))[0];
    assert_eq!(f.charge(e), 40_000);

    f.지움(&[id]).unwrap();
    assert_eq!(f.charge(e), 35_000);
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 0);
}

// ═══════════════════════════════════════════════ 3 · 4 여러 기록

#[test]
fn 여러_환불_가운데_뒤의_것을_지우면_30000원이다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    f.개별(e, 30_000, 등록("A 휴강")); //  -5,000
    let b = f.개별(e, 27_000, 등록("B 개인"))[0]; // -3,000
    assert_eq!(f.charge(e), 27_000);

    f.지움(&[b]).unwrap();
    assert_eq!(f.charge(e), 30_000, "B 의 3,000 만 되돌아와야 한다");
    assert_eq!(f.명단(), (5_000, 0), "A 가 사라졌다");
}

#[test]
fn 여러_환불_가운데_앞의_것을_지우면_32000원이다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let a = f.개별(e, 30_000, 등록("A 휴강"))[0]; // -5,000
    f.개별(e, 27_000, 등록("B 개인")); //             -3,000

    f.지움(&[a]).unwrap();
    // 처음 금액(35,000)으로 돌아가면 안 된다 — B 의 3,000 은 여전히 살아 있다
    assert_eq!(f.charge(e), 32_000);
    assert_eq!(f.명단(), (3_000, 0));

    // 남은 B 가 '확인 필요'로 뜨지 않는다 — 이번 변화는 지운 A 가 설명했다
    let v = f
        .db
        .read(|c| repo::adjustment::view(c, f.ws, &AdjustmentFilter::default(), &f.items))
        .unwrap();
    assert_eq!(v.needs_check_all, 0);
}

#[test]
fn 둘을_함께_지우면_35000원이다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let a = f.개별(e, 30_000, 등록("A"))[0];
    let b = f.개별(e, 27_000, 등록("B"))[0];
    f.지움(&[a, b]).unwrap();
    assert_eq!(f.charge(e), 35_000);
}

#[test]
fn 환불과_추가징수가_섞이면_환불만_되돌린다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let r = f.개별(e, 30_000, 등록("휴강 환불"))[0]; //  -5,000
    f.개별(e, 32_000, 등록("재료 추가")); //             +2,000
    assert_eq!(f.charge(e), 32_000);

    f.지움(&[r]).unwrap();
    assert_eq!(f.charge(e), 37_000, "32,000 + 5,000 이어야 한다");
    assert_eq!(f.명단(), (0, 2_000), "추가징수 2,000 이 사라졌다");
}

// ═══════════════════════════════════════════════ 5 · 6 · 7 부서 일괄

#[test]
fn 묶음을_지우면_10명이_모두_5000원씩_돌아온다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 10);
    f.set_base(d, 30_000);
    let batch = f.일괄(d, &es, "강사 사정 휴강");
    assert!(es.iter().all(|e| f.charge(*e) == 30_000));
    assert_eq!(f.명단(), (50_000, 0));

    let n = f
        .db
        .write(|c| repo::adjustment::delete_batch(c, batch))
        .unwrap();
    assert_eq!(n, 10);
    assert!(es.iter().all(|e| f.charge(*e) == 35_000), "묶음을 지웠는데 금액이 남았다");
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 0);
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_batch"), 0);

    // 이력: 묶음에 한 줄 + 학생마다 금액 변화 한 줄
    let logs = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, None, 500))
        .unwrap();
    assert_eq!(logs.iter().filter(|l| l.kind == "ADJUST_BATCH_DELETE").count(), 1);
    let 원복 = logs
        .iter()
        .filter(|l| l.kind == "CHARGE_EDIT" && l.reason.contains("원복"))
        .count();
    assert_eq!(원복, 10, "학생별 원복이 이력에 남지 않았다");
}

#[test]
fn 뺐던_학생은_묶음을_지워도_그대로다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 18);
    f.set_base(d, 30_000);
    let 뺀 = es[0];
    let batch = f.일괄(d, &es[1..], "휴강"); // 17명만
    assert_eq!(f.charge(뺀), 35_000);

    f.db.write(|c| repo::adjustment::delete_batch(c, batch)).unwrap();
    assert!(es[1..].iter().all(|e| f.charge(*e) == 35_000), "17명이 돌아오지 않았다");
    assert_eq!(f.charge(뺀), 35_000, "뺐던 학생의 금액이 바뀌었다");
}

#[test]
fn 묶음의_한_학생만_지우면_그_학생만_돌아오고_묶음은_남는다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 17);
    f.set_base(d, 30_000);
    let batch = f.일괄(d, &es, "휴강");

    let one: i64 = f.개수(&format!(
        "SELECT id FROM billing_adjustment WHERE enrollment_id = {}",
        es[0]
    ));
    f.지움(&[one]).unwrap();

    assert_eq!(f.charge(es[0]), 35_000, "지운 학생이 돌아오지 않았다");
    assert!(es[1..].iter().all(|e| f.charge(*e) == 30_000), "다른 학생까지 바뀌었다");
    assert_eq!(
        f.개수(&format!("SELECT COUNT(*) FROM billing_adjustment WHERE batch_id = {batch}")),
        16
    );
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_batch"), 1, "묶음이 사라졌다");
}

#[test]
fn 묶음의_마지막_기록까지_지우면_빈_묶음도_치운다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 2);
    f.set_base(d, 30_000);
    f.일괄(d, &es, "휴강");
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
    f.지움(&ids).unwrap();
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_batch"), 0, "빈 묶음이 남았다");
}

// ═══════════════════════════════════════════════ 8 반쪽 없음

#[test]
fn 묶음_삭제가_중간에_실패하면_아무것도_바뀌지_않는다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 18);
    f.set_base(d, 30_000);
    let batch = f.일괄(d, &es, "휴강");

    let r: crate::error::AppResult<()> = f.db.write(|c| {
        repo::adjustment::delete_batch(c, batch)?;
        Err(crate::error::AppError::invalid("일부러 실패"))
    });
    assert!(r.is_err());

    assert!(
        es.iter().all(|e| f.charge(*e) == 30_000),
        "금액이 일부만 되돌려진 채 남았다"
    );
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 18);
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_batch"), 1);
}

// ═══════════════════════════════════════════════ 안전장치

#[test]
fn 조정_뒤에_금액을_따로_고쳤으면_막고_아무것도_바꾸지_않는다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    // 누군가 '금액만' 다시 고쳤다 — 이 변화와 환불의 관계는 프로그램이 알 수 없다
    f.개별(e, 31_000, 금액만());

    let err = f.지움(&[id]).unwrap_err();
    assert!(format!("{err:?}").contains("자동으로 되돌릴 수 없습니다"), "{err:?}");
    assert_eq!(f.charge(e), 31_000, "막혔는데 금액이 바뀌었다");
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 1, "막혔는데 기록이 지워졌다");
}

#[test]
fn 현재_금액_반영_뒤에는_반영된_차액만큼_되돌린다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    f.개별(e, 32_000, 금액만()); // 환불을 3,000 으로 바로잡을 셈
    f.db.write(|c| repo::adjustment::confirm(c, &[id], repo::adjustment::APPLY))
        .unwrap();
    assert_eq!(f.명단(), (3_000, 0));

    // 이제 그 기록은 35,000 → 32,000 을 설명한다 — 지우면 35,000
    f.지움(&[id]).unwrap();
    assert_eq!(f.charge(e), 35_000);
}

#[test]
fn 기존_금액_유지_뒤에는_기록_금액만큼_되돌린다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    f.개별(e, 32_000, 금액만());
    // '이 2,000 변화는 환불과 별개다' 라고 사람이 확인했다
    f.db.write(|c| repo::adjustment::confirm(c, &[id], repo::adjustment::KEEP))
        .unwrap();
    f.지움(&[id]).unwrap();
    // 환불 5,000 만 되돌리고 별개로 확인한 2,000 은 남는다
    assert_eq!(f.charge(e), 37_000);
}

#[test]
fn 삭제_미리보기는_아무것도_바꾸지_않고_금액을_보여준다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];

    let p = f
        .db
        .read(|c| repo::adjustment::delete_preview(c, &[id], &f.items))
        .unwrap();
    assert_eq!(p.rows.len(), 1);
    assert!(p.rows[0].reverts);
    assert_eq!(p.rows[0].charge_before, 30_000);
    assert_eq!(p.rows[0].charge_after, 35_000);
    assert!(!p.blocked);
    assert_eq!(f.charge(e), 30_000, "미리보기가 금액을 바꿨다");
}

#[test]
fn 묶음_미리보기는_학생_수와_총액을_보여준다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 18);
    f.set_base(d, 30_000);
    let batch = f.일괄(d, &es[1..], "휴강");
    let p = f
        .db
        .read(|c| repo::adjustment::batch_delete_preview(c, batch, &f.items))
        .unwrap();
    assert_eq!(p.students, 17);
    assert_eq!(p.refund_total, 85_000);
    assert!(p.rows.iter().all(|r| r.charge_after == 35_000));
}

#[test]
fn 묶음에_막히는_학생이_하나라도_있으면_아무도_되돌리지_않는다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 5);
    f.set_base(d, 30_000);
    let batch = f.일괄(d, &es, "휴강");
    f.개별(es[2], 31_000, 금액만()); // 한 학생만 따로 다시 고쳤다

    let err = f
        .db
        .write(|c| repo::adjustment::delete_batch(c, batch))
        .unwrap_err();
    assert!(format!("{err:?}").contains("학생3"), "어느 학생인지 알려 주지 않는다: {err:?}");
    assert!(
        [0, 1, 3, 4].iter().all(|i| f.charge(es[*i]) == 30_000),
        "막혔는데 다른 학생이 되돌려졌다"
    );
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 5);
}

// ═══════════════════════════════════════════════ 9 정산

#[test]
fn 정산은_등록과_삭제마다_낡고_재정산하면_원래_금액으로_돌아온다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let 상태 = || f.db.read(|c| repo::settle::status(c, f.ws)).unwrap().state;
    let 정산합 = || -> i64 {
        f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
        f.db.read(|c| {
            Ok(c.query_row(
                "SELECT COALESCE(SUM(a.amount),0) FROM settlement_alloc a
                   JOIN settlement s ON s.id = a.settlement_id
                  WHERE s.workspace_id = ?1 AND s.is_latest = 1",
                rusqlite::params![f.ws],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    };

    assert_eq!(정산합(), 35_000);
    assert_eq!(상태(), "FRESH");

    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    assert_ne!(상태(), "FRESH", "환불 등록(금액 변경)에도 정산이 낡지 않았다");
    assert_eq!(정산합(), 30_000);
    assert_eq!(상태(), "FRESH");

    f.지움(&[id]).unwrap();
    assert_ne!(상태(), "FRESH", "원복(금액 변경)에도 정산이 낡지 않았다");
    assert_eq!(정산합(), 35_000, "재정산이 원래 금액으로 돌아오지 않았다");
}

// ═══════════════════════════════════════════════ 10 기존 기록 회귀

#[test]
fn 기존_취소_환불과_중도_추가는_기록만_지운다() {
    let f = F::new();
    let d = f.dept(35_000);
    let es = f.fill(d, 2);

    // 중도 추가 (ENROLL) — 수강 0번
    let add = f
        .db
        .write(|c| {
            repo::adjustment::create_additional(
                c,
                es[0],
                &AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )
        })
        .unwrap();
    // 수강 취소 (CANCEL) — 수강 1번, 35,000 → 20,000
    let before = f.db.read(|c| repo::enrollment::get(c, es[1], &f.items)).unwrap();
    let refund = f
        .db
        .write(|c| {
            repo::enrollment::cancel(c, es[1], Some(&[fee(강사료, 20_000)]), "포기", &f.items)?;
            repo::adjustment::create_refund(
                c,
                es[1],
                &before,
                &AdjustmentInput {
                    occurred_on: None,
                    note: None,
                },
                &f.items,
            )
        })
        .unwrap();
    // 기존(LEGACY) 꼴 하나 — 008 이전 기록과 같게 발생 유형만 바꿔 둔다
    f.db.write(|c| {
        c.execute(
            "UPDATE billing_adjustment SET source = 'LEGACY' WHERE id = ?1",
            rusqlite::params![refund],
        )?;
        Ok(())
    })
    .unwrap();

    let p = f
        .db
        .read(|c| repo::adjustment::delete_preview(c, &[add, refund], &f.items))
        .unwrap();
    assert!(p.rows.iter().all(|r| !r.reverts), "기존 기록에 원복을 적용하려 한다");

    f.지움(&[add, refund]).unwrap();
    assert_eq!(f.charge(es[0]), 35_000, "중도 추가 기록 삭제가 금액을 바꿨다");
    assert_eq!(f.charge(es[1]), 20_000, "취소 환불 기록 삭제가 취소자 금액을 바꿨다");
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 0);
}

#[test]
fn 취소한_수강_건의_차액_기록은_자동으로_되돌리지_않는다() {
    let f = F::new();
    let d = f.dept(35_000);
    let e = f.fill(d, 1)[0];
    let id = f.개별(e, 30_000, 등록("휴강"))[0];
    f.db.write(|c| repo::enrollment::cancel(c, e, None, "포기", &f.items))
        .unwrap();

    let err = f.지움(&[id]).unwrap_err();
    assert!(format!("{err:?}").contains("취소한 수강"), "{err:?}");
    assert_eq!(f.개수("SELECT COUNT(*) FROM billing_adjustment"), 1);
}
