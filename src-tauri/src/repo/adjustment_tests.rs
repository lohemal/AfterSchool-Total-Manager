//! 추가징수 · 환불 시험 (v0.1.5).
//!
//! 가장 중요한 것 셋.
//!
//! 1. **사람이 고른 것만 기록이 된다** — 수강을 추가했다고, 취소했다고 해서
//!    저절로 추가징수·환불이 되지 않는다.
//! 2. **금액은 만든 시점에서 굳는다** — 나중에 수강생 명단에서 금액을 고쳐도
//!    기록이 저절로 따라 바뀌지 않고, 사람에게 확인을 구한다.
//! 3. **같은 값을 다시 써도(no-op) 확인 필요가 되지 않는다.**

use std::path::PathBuf;

use crate::db::Db;
use crate::model::{
    AdjustmentFilter, AdjustmentInput, CostItem, DepartmentInput, EnrollmentInput, Fee,
    StudentInput, WorkspaceInput,
};
use crate::repo;
use crate::repo::adjustment::{ADDITIONAL, APPLY, KEEP, REFUND};

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

/// 요구사항의 예시 금액 — 합계 85,000.
fn 원래_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 50_000),
        fee(수용비, 5_000),
        fee(교재비, 20_000),
        fee(재료비, 10_000),
    ]
}

/// 취소 후 실제 징수액 — 합계 63,000. 환불 22,000.
fn 취소후_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 30_000),
        fee(수용비, 3_000),
        fee(교재비, 20_000),
        fee(재료비, 10_000),
    ]
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
        let ws = db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    year,
                    &WorkspaceInput {
                        name: "2026년 9월".into(),
                        start_date: "2026-09-01".into(),
                        end_date: "2026-09-30".into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        Self {
            db,
            year,
            ws,
            items,
        }
    }

    fn student(&self, grade: i64, class_no: &str, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no: class_no.into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn dept(&self, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class_name.into()),
                        teacher: None,
                        days: None,
                        note: None,
                        fees,
                        capacity: None,
                        allowed_grades: Vec::new(),
                    },
                )
            })
            .unwrap()
    }

    /// 수강 추가 — `adj` 를 주면 추가징수 대상으로 함께 등록한다.
    /// 명령 계층이 하는 것과 똑같이 **한 트랜잭션**으로 묶는다.
    fn enroll(&self, student: i64, dept: i64, adj: Option<AdjustmentInput>) -> i64 {
        self.db
            .write(|c| {
                let id = repo::enrollment::create(
                    c,
                    self.ws,
                    &EnrollmentInput {
                        student_id: student,
                        department_id: dept,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )?;
                if let Some(a) = adj.as_ref() {
                    repo::adjustment::create_additional(c, id, a, &self.items)?;
                }
                Ok(id)
            })
            .unwrap()
    }

    /// 수강 취소 — `adj` 를 주면 환불 대상으로 함께 등록한다.
    fn cancel(
        &self,
        e: i64,
        fees: Option<&[Fee]>,
        adj: Option<AdjustmentInput>,
    ) -> crate::error::AppResult<()> {
        self.db.write(|c| {
            let before = repo::enrollment::get(c, e, &self.items)?;
            repo::enrollment::cancel(c, e, fees, "중도 포기", &self.items)?;
            if let Some(a) = adj.as_ref() {
                repo::adjustment::create_refund(c, e, &before, a, &self.items)?;
            }
            Ok(())
        })
    }

    fn 금액수정(&self, e: i64, fees: &[Fee]) {
        self.db
            .write(|c| repo::enrollment::update_fees(c, e, fees, "조정", &self.items))
            .unwrap();
    }

    fn view(&self, f: &AdjustmentFilter) -> crate::model::AdjustmentView {
        self.db
            .read(|c| repo::adjustment::view(c, self.ws, f, &self.items))
            .unwrap()
    }

    fn 전체(&self) -> crate::model::AdjustmentView {
        self.view(&AdjustmentFilter::default())
    }

    fn diffs(&self) -> Vec<crate::model::AdjustmentDiff> {
        self.db
            .read(|c| repo::adjustment::diffs(c, self.ws, &self.items))
            .unwrap()
    }

    fn confirm(&self, ids: &[i64], mode: &str) -> crate::error::AppResult<i64> {
        self.db.write(|c| repo::adjustment::confirm(c, ids, mode))
    }

    fn 자료판(&self) -> i64 {
        self.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT data_version FROM workspace WHERE id = ?1",
                    rusqlite::params![self.ws],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn 정산상태(&self) -> String {
        self.db
            .read(|c| repo::settle::status(c, self.ws))
            .unwrap()
            .state
    }
}

fn 금액(a: &crate::model::Adjustment, code: &str) -> i64 {
    a.fees
        .iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap_or(0)
}

fn 줄금액(r: &crate::model::StudentSumRow, code: &str) -> i64 {
    r.fees
        .iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap_or(0)
}

// ─────────────────────────────────── 추가징수

#[test]
fn 체크하지_않고_수강을_추가하면_기록이_생기지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    f.enroll(a, d, None);

    let v = f.전체();
    assert_eq!(v.additional.count, 0, "스스로 추가징수로 만들면 안 된다");
    assert_eq!(v.refund.count, 0);
}

#[test]
fn 추가징수를_체크하면_지금_금액이_굳는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    f.enroll(
        a,
        d,
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-10".into()),
            note: Some("9월 중도 등록".into()),
        }),
    );

    let v = f.전체();
    assert_eq!(v.additional.count, 1);
    let one = &v.additional.details[0];
    assert_eq!(one.kind, ADDITIONAL);
    assert_eq!(금액(one, 강사료), 50_000);
    assert_eq!(금액(one, 수용비), 5_000);
    assert_eq!(금액(one, 교재비), 20_000);
    assert_eq!(금액(one, 재료비), 10_000);
    assert_eq!(one.total, 85_000);
    assert_eq!(one.occurred_on, "2026-09-10");
    assert_eq!(one.note, "9월 중도 등록");
    assert!(!one.needs_check, "만든 직후에는 확인할 것이 없다");
}

#[test]
fn 발생일을_비우면_오늘이_들어간다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    f.enroll(
        a,
        d,
        Some(AdjustmentInput {
            occurred_on: None,
            note: None,
        }),
    );
    let today: String = f
        .db
        .read(|c| Ok(c.query_row("SELECT date('now','localtime')", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(f.전체().additional.details[0].occurred_on, today);
}

#[test]
fn 발생일_꼴이_틀리면_아무것도_만들지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);

    let err = f
        .db
        .write(|c| {
            let id = repo::enrollment::create(
                c,
                f.ws,
                &EnrollmentInput {
                    student_id: a,
                    department_id: d,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )?;
            repo::adjustment::create_additional(
                c,
                id,
                &AdjustmentInput {
                    occurred_on: Some("2026/09/10".into()),
                    note: None,
                },
                &f.items,
            )
        })
        .unwrap_err();
    assert!(format!("{err:?}").contains("발생일"), "{err:?}");

    // 수강까지 통째로 되돌아간다 — 한쪽만 남으면 안 된다
    let n = f
        .db
        .read(|c| repo::enrollment::list(c, f.ws, &f.items, &Default::default()))
        .unwrap();
    assert!(n.is_empty(), "수강이 남았다");
    assert_eq!(f.전체().additional.count, 0);
}

#[test]
fn 여러_부서_추가징수가_학생별로_합산된다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let 로봇 = f.dept("로봇과학", "B반", 원래_금액());
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000), fee(재료비, 7_000)]);
    let adj = || {
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-10".into()),
            note: None,
        })
    };
    f.enroll(a, 로봇, adj());
    f.enroll(a, 미술, adj());

    let v = f.전체();
    assert_eq!(v.additional.rows.len(), 1, "학생 한 명이므로 한 줄");
    let row = &v.additional.rows[0];
    assert_eq!(row.details, 2, "누르면 부서 두 줄");
    assert_eq!(줄금액(row, 강사료), 70_000, "50,000 + 20,000");
    assert_eq!(줄금액(row, 재료비), 17_000, "10,000 + 7,000");
    assert_eq!(row.total, 85_000 + 27_000);
    assert_eq!(v.additional.total, row.total);

    // 상세의 부서 이름이 살아 있다
    let 부서: Vec<&str> = v
        .additional
        .details
        .iter()
        .map(|a| a.dept_label.as_str())
        .collect();
    assert_eq!(부서, vec!["로봇과학B반", "미술A반"]);
}

#[test]
fn 추가징수_기록은_정산을_낡게_만들지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.db.write(|c| repo::settle::generate(c, f.ws)).unwrap();
    assert_eq!(f.정산상태(), "FRESH");
    let 판 = f.자료판();

    // 이미 있는 수강 건에 기록만 더한다
    f.db.write(|c| {
        repo::adjustment::create_additional(
            c,
            e,
            &AdjustmentInput {
                occurred_on: Some("2026-09-10".into()),
                note: None,
            },
            &f.items,
        )
    })
    .unwrap();

    assert_eq!(f.자료판(), 판, "자료판이 올라갔다");
    assert_eq!(f.정산상태(), "FRESH", "정산이 낡음이 되었다");
}

// ─────────────────────────────────── 환불

#[test]
fn 체크하지_않고_취소하면_기록이_생기지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(e, Some(&취소후_금액()), None).unwrap();

    assert_eq!(f.전체().refund.count, 0, "취소했다고 환불이 되면 안 된다");
}

#[test]
fn 환불액은_취소_직전에서_취소_후를_뺀_값이다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(
        e,
        Some(&취소후_금액()),
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-20".into()),
            note: Some("9월 중도 포기".into()),
        }),
    )
    .unwrap();

    let v = f.전체();
    assert_eq!(v.refund.count, 1);
    let one = &v.refund.details[0];
    assert_eq!(one.kind, REFUND);
    // 요구사항의 예시 그대로
    assert_eq!(금액(one, 강사료), 20_000, "50,000 - 30,000");
    assert_eq!(금액(one, 수용비), 2_000, "5,000 - 3,000");
    assert_eq!(금액(one, 교재비), 0, "배부했으므로 환불 없음");
    assert_eq!(금액(one, 재료비), 0);
    assert_eq!(one.total, 22_000);
    assert_eq!(one.enrollment_status, "CANCELLED");

    // 환불 기준(취소 직전 금액)이 보존된다
    let 기준: Vec<i64> = one.fees.iter().map(|x| x.base_amount).collect();
    assert_eq!(기준, vec![50_000, 5_000, 20_000, 10_000]);
}

#[test]
fn 전액_환불과_환불_0원() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let adj = || {
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-20".into()),
            note: None,
        })
    };

    // 전액 면제 → 전액 환불
    let e1 = f.enroll(a, d, None);
    f.cancel(
        e1,
        Some(&[fee(강사료, 0), fee(수용비, 0), fee(교재비, 0), fee(재료비, 0)]),
        adj(),
    )
    .unwrap();

    // 전액 징수 → 환불 0원
    let e2 = f.enroll(b, d, None);
    f.cancel(e2, Some(&원래_금액()), adj()).unwrap();

    let v = f.전체();
    assert_eq!(v.refund.count, 2);
    let 전액 = v.refund.details.iter().find(|x| x.name == "김하나").unwrap();
    assert_eq!(전액.total, 85_000, "전액 환불");
    let 영원 = v.refund.details.iter().find(|x| x.name == "이두리").unwrap();
    assert_eq!(영원.total, 0, "환불 0원도 기록으로 남는다");
    assert_eq!(v.refund.total, 85_000);
}

#[test]
fn 환불액이_음수면_취소도_되지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);

    // 취소 후 금액이 취소 전보다 크다
    let err = f
        .cancel(
            e,
            Some(&[
                fee(강사료, 60_000),
                fee(수용비, 5_000),
                fee(교재비, 20_000),
                fee(재료비, 10_000),
            ]),
            Some(AdjustmentInput {
                occurred_on: Some("2026-09-20".into()),
                note: None,
            }),
        )
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("강사료"), "{msg}");
    assert!(msg.contains("음수"), "{msg}");

    // 취소도 금액도 되돌아간다
    let row = f
        .db
        .read(|c| repo::enrollment::get(c, e, &f.items))
        .unwrap();
    assert_eq!(row.status, "ACTIVE", "취소가 남았다");
    assert_eq!(row.total, 85_000, "금액이 바뀌었다");
    assert_eq!(f.전체().refund.count, 0);
}

#[test]
fn 환불_체크를_풀면_같은_금액으로도_취소된다() {
    // 기존 취소 기능의 허용 범위는 그대로다 (설계안 26-4).
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);

    f.cancel(e, Some(&[fee(강사료, 60_000)]), None).unwrap();
    let row = f
        .db
        .read(|c| repo::enrollment::get(c, e, &f.items))
        .unwrap();
    assert_eq!(row.status, "CANCELLED");
}

// ─────────────────────────────────── 변경 감지

/// 추가징수 한 건을 만들고 그 수강 건 id 와 조정 id 를 돌려준다.
fn 추가징수_하나(f: &F) -> (i64, i64) {
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(
        a,
        d,
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-10".into()),
            note: None,
        }),
    );
    let id = f.전체().additional.details[0].id;
    (e, id)
}

#[test]
fn 원본_금액이_달라지면_확인이_필요해진다() {
    let f = F::new();
    let (e, id) = 추가징수_하나(&f);
    assert_eq!(f.전체().needs_check_all, 0);

    f.금액수정(
        e,
        &[
            fee(강사료, 45_000),
            fee(수용비, 5_000),
            fee(교재비, 20_000),
            fee(재료비, 10_000),
        ],
    );

    let v = f.전체();
    assert_eq!(v.needs_check_all, 1);
    assert_eq!(v.additional.needs_check, 1);
    assert!(v.additional.details[0].needs_check);

    let d = f.diffs();
    assert_eq!(d.len(), 1, "달라진 칸은 강사료 하나뿐");
    assert_eq!(d[0].adjustment_id, id);
    assert_eq!(d[0].item_name, "강사료");
    assert_eq!(d[0].saved, 50_000, "등록 당시");
    assert_eq!(d[0].current_charge, 45_000, "현재 charge");
    assert_eq!(d[0].suggested, 45_000);
    assert!(!d[0].negative);
}

#[test]
fn 같은_금액을_다시_써도_확인_필요가_되지_않는다() {
    let f = F::new();
    let (e, _) = 추가징수_하나(&f);

    // update_fees 는 같은 값도 charge 에 쓴다 — 그래도 값이 같으므로 조용해야 한다
    f.금액수정(e, &원래_금액());
    assert_eq!(f.전체().needs_check_all, 0, "no-op 이 확인 필요를 만들었다");

    // 부서 기준금액 재반영도 마찬가지 — 바뀔 것이 없으면 아무 일도 없다
    f.db.write(|c| {
        repo::enrollment::apply_fees(c, f.ws, None, "ALL", &[], "재반영", &f.items)
    })
    .unwrap();
    assert_eq!(f.전체().needs_check_all, 0);
}

#[test]
fn 기존_금액_유지는_금액을_두고_경고만_없앤다() {
    let f = F::new();
    let (e, id) = 추가징수_하나(&f);
    f.금액수정(e, &[fee(강사료, 45_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);
    assert_eq!(f.전체().needs_check_all, 1);

    assert_eq!(f.confirm(&[id], KEEP).unwrap(), 1);

    let v = f.전체();
    assert_eq!(v.needs_check_all, 0, "경고가 남았다");
    assert!(f.diffs().is_empty());
    assert_eq!(금액(&v.additional.details[0], 강사료), 50_000, "금액이 바뀌었다");
    assert_eq!(v.additional.details[0].total, 85_000);
}

#[test]
fn 현재_금액_반영은_금액까지_바꾼다() {
    let f = F::new();
    let (e, id) = 추가징수_하나(&f);
    f.금액수정(e, &[fee(강사료, 45_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);

    assert_eq!(f.confirm(&[id], APPLY).unwrap(), 1);

    let v = f.전체();
    assert_eq!(v.needs_check_all, 0);
    assert_eq!(금액(&v.additional.details[0], 강사료), 45_000);
    assert_eq!(v.additional.details[0].total, 80_000);
}

#[test]
fn 확인한_뒤_또_바뀌면_다시_확인이_필요해진다() {
    let f = F::new();
    let (e, id) = 추가징수_하나(&f);
    f.금액수정(e, &[fee(강사료, 45_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);
    f.confirm(&[id], KEEP).unwrap();
    assert_eq!(f.전체().needs_check_all, 0);

    // 한 번 더 고친다
    f.금액수정(e, &[fee(강사료, 40_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);
    assert_eq!(f.전체().needs_check_all, 1, "새 변경은 새 확인이어야 한다");
    let d = f.diffs();
    assert_eq!(d[0].checked_charge, 45_000, "마지막으로 확인한 값");
    assert_eq!(d[0].current_charge, 40_000);
}

#[test]
fn 부서_기준금액_재반영_경로도_잡힌다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    f.enroll(
        a,
        d,
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-10".into()),
            note: None,
        }),
    );

    // 부서 기준금액을 바꾸고 반영한다
    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "로봇과학".into(),
                class_name: Some("B반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![fee(강사료, 35_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)],
                capacity: None,
                allowed_grades: Vec::new(),
            },
        )
    })
    .unwrap();
    f.db.write(|c| {
        repo::enrollment::apply_fees(c, f.ws, Some(d), "ALL", &[], "기준금액 인하", &f.items)
    })
    .unwrap();

    assert_eq!(f.전체().needs_check_all, 1);
    assert_eq!(f.diffs()[0].current_charge, 35_000);
}

#[test]
fn 취소_학생의_금액을_고쳐도_잡힌다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(
        e,
        Some(&취소후_금액()),
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-20".into()),
            note: None,
        }),
    )
    .unwrap();
    assert_eq!(f.전체().needs_check_all, 0);

    // 취소자의 최종 징수액을 63,000 → 60,000 으로 고친다
    f.금액수정(
        e,
        &[fee(강사료, 27_000), fee(수용비, 3_000), fee(교재비, 20_000), fee(재료비, 10_000)],
    );
    assert_eq!(f.전체().needs_check_all, 1);
}

// ─────────────────────────────────── 환불 재계산

#[test]
fn 환불은_취소_직전_금액을_기준으로_다시_센다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(
        e,
        Some(&취소후_금액()),
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-20".into()),
            note: None,
        }),
    )
    .unwrap();
    let id = f.전체().refund.details[0].id;

    // 취소자의 최종 징수액을 30,000 → 27,000 으로 내린다 (합계 63,000 → 60,000)
    f.금액수정(
        e,
        &[fee(강사료, 27_000), fee(수용비, 3_000), fee(교재비, 20_000), fee(재료비, 10_000)],
    );

    let d0 = f.diffs();
    assert_eq!(d0.len(), 1);
    assert_eq!(d0[0].kind, REFUND);
    assert_eq!(d0[0].base_amount, 50_000, "취소 직전 기준은 그대로");
    assert_eq!(d0[0].current_charge, 27_000);
    assert_eq!(d0[0].saved, 20_000, "저장된 환불액");
    assert_eq!(d0[0].suggested, 23_000, "50,000 - 27,000");

    // 기존 환불액 유지
    f.confirm(&[id], KEEP).unwrap();
    assert_eq!(f.전체().refund.details[0].total, 22_000);
    assert_eq!(f.전체().needs_check_all, 0);

    // 다시 내렸다가 이번에는 현재 기준으로 반영
    f.금액수정(
        e,
        &[fee(강사료, 25_000), fee(수용비, 3_000), fee(교재비, 20_000), fee(재료비, 10_000)],
    );
    f.confirm(&[id], APPLY).unwrap();
    let v = f.전체();
    assert_eq!(금액(&v.refund.details[0], 강사료), 25_000, "50,000 - 25,000");
    assert_eq!(v.refund.details[0].total, 27_000, "25,000 + 2,000");
    assert_eq!(v.needs_check_all, 0);
}

#[test]
fn 음수가_되는_환불은_현재_기준으로_반영하지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(
        e,
        Some(&취소후_금액()),
        Some(AdjustmentInput {
            occurred_on: Some("2026-09-20".into()),
            note: None,
        }),
    )
    .unwrap();
    let id = f.전체().refund.details[0].id;

    // 취소 직전(50,000)보다 큰 금액으로 고친다
    f.금액수정(
        e,
        &[fee(강사료, 60_000), fee(수용비, 3_000), fee(교재비, 20_000), fee(재료비, 10_000)],
    );
    let d0 = f.diffs();
    assert!(d0.iter().any(|x| x.negative), "음수임을 알려야 한다");

    let err = f.confirm(&[id], APPLY).unwrap_err();
    assert!(format!("{err:?}").contains("음수"), "{err:?}");

    // 기존 금액 유지는 된다 — 사람이 확인했다는 뜻이므로
    f.confirm(&[id], KEEP).unwrap();
    assert_eq!(f.전체().refund.details[0].total, 22_000);
    assert_eq!(f.전체().needs_check_all, 0);
}

// ─────────────────────────────────── 필터

#[test]
fn 발생일_기간으로_거른다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let 로봇 = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000)]);
    f.enroll(a, 로봇, Some(AdjustmentInput { occurred_on: Some("2026-09-05".into()), note: None }));
    f.enroll(a, 미술, Some(AdjustmentInput { occurred_on: Some("2026-10-05".into()), note: None }));

    let v = f.view(&AdjustmentFilter {
        from: Some("2026-09-01".into()),
        to: Some("2026-09-30".into()),
        ..Default::default()
    });
    assert_eq!(v.additional.count, 1);
    assert_eq!(v.additional.total, 10_000, "10월 것은 빠진다");
}

#[test]
fn 부서는_학생을_찾는_조건이다() {
    let f = F::new();
    let 듣는이 = f.student(1, "가람", 1, "홍길동");
    let 안듣는이 = f.student(1, "가람", 2, "이두리");
    let 로봇 = f.dept("로봇과학", "B반", vec![fee(강사료, 30_000)]);
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000)]);
    let adj = || Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None });
    f.enroll(듣는이, 로봇, adj());
    f.enroll(듣는이, 미술, adj());
    f.enroll(안듣는이, 미술, adj());

    let v = f.view(&AdjustmentFilter {
        department_id: Some(로봇),
        ..Default::default()
    });
    assert_eq!(v.additional.rows.len(), 1, "로봇과학 추가징수가 있는 학생만");
    assert_eq!(v.additional.rows[0].name, "홍길동");
    // 찾은 학생의 줄에는 미술까지 더해 나온다
    assert_eq!(v.additional.rows[0].total, 50_000, "30,000 + 20,000");
    assert_eq!(v.additional.rows[0].details, 2);
}

#[test]
fn 학년_반_이름으로_거른다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let b = f.student(2, "나리", 1, "이두리");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    let adj = || Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None });
    f.enroll(a, d, adj());
    f.enroll(b, d, adj());

    assert_eq!(f.view(&AdjustmentFilter { grade: Some(1), ..Default::default() }).additional.count, 1);
    assert_eq!(
        f.view(&AdjustmentFilter { class_no: Some("나리".into()), ..Default::default() })
            .additional
            .details[0]
            .name,
        "이두리"
    );
    assert_eq!(
        f.view(&AdjustmentFilter { query: Some("홍길".into()), ..Default::default() })
            .additional
            .count,
        1
    );
}

#[test]
fn 배지는_필터와_무관하게_전체를_센다() {
    let f = F::new();
    let (e, _) = 추가징수_하나(&f);
    f.금액수정(e, &[fee(강사료, 45_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);

    // 아무것도 걸리지 않는 조건으로 봐도 배지는 1이어야 한다
    let v = f.view(&AdjustmentFilter {
        grade: Some(9),
        ..Default::default()
    });
    assert_eq!(v.additional.count, 0, "걸러진 목록은 비어 있고");
    assert_eq!(v.needs_check_all, 1, "배지는 그래도 알려 준다");
}

// ─────────────────────────────────── 과거 기록의 뜻

#[test]
fn 반이나_부서명이_바뀌어도_당시_표시가_남는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    f.enroll(a, d, Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None }));

    // 학생이 반을 옮기고 부서 이름도 바뀐다
    f.db.write(|c| {
        repo::student::update(
            c,
            a,
            &StudentInput {
                grade: 1,
                class_no: "나리".into(),
                student_no: 5,
                name: "홍길동".into(),
                note: None,
            },
        )
    })
    .unwrap();
    f.db.write(|c| {
        repo::department::update(
            c,
            d,
            &DepartmentInput {
                name: "창의로봇".into(),
                class_name: Some("B반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![fee(강사료, 10_000)],
                capacity: None,
                allowed_grades: Vec::new(),
            },
        )
    })
    .unwrap();

    let one = &f.전체().additional.details[0];
    // 목록·필터는 현재 정보를 따른다
    assert_eq!(one.class_no, "나리");
    assert_eq!(one.student_no, 5);
    assert_eq!(one.dept_label, "창의로봇B반");
    // 상세에는 그때가 남아 있다
    assert_eq!(one.student_label_at, "1학년 가람반 1번 홍길동");
    assert_eq!(one.dept_label_at, "로봇과학B반");
}

#[test]
fn 취소_후_다시_수강하면_기록이_서로_섞이지_않는다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());

    // 첫 수강 → 취소(환불)
    let e1 = f.enroll(a, d, None);
    f.cancel(
        e1,
        Some(&취소후_금액()),
        Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None }),
    )
    .unwrap();

    // 같은 부서를 다시 수강 → 추가징수
    let e2 = f.enroll(a, d, Some(AdjustmentInput { occurred_on: Some("2026-09-15".into()), note: None }));
    assert_ne!(e1, e2);

    let v = f.전체();
    assert_eq!(v.refund.count, 1);
    assert_eq!(v.additional.count, 1);
    assert_eq!(v.refund.details[0].enrollment_id, e1);
    assert_eq!(v.additional.details[0].enrollment_id, e2);

    // 한쪽 금액을 고쳐도 다른 쪽은 조용하다
    f.금액수정(e2, &[fee(강사료, 40_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);
    let d0 = f.diffs();
    assert_eq!(d0.len(), 1);
    assert_eq!(d0[0].kind, ADDITIONAL);
}

#[test]
fn 환불_기록이_있는_수강을_되돌려도_기준은_그대로다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", 원래_금액());
    let e = f.enroll(a, d, None);
    f.cancel(
        e,
        Some(&취소후_금액()),
        Some(AdjustmentInput { occurred_on: Some("2026-09-20".into()), note: None }),
    )
    .unwrap();

    // 금액을 부서 기준으로 되돌리며 복원 → charge 가 85,000 으로 돌아간다
    f.db
        .write(|c| repo::enrollment::restore(c, e, true, "착오", &f.items))
        .unwrap();

    let v = f.전체();
    let one = &v.refund.details[0];
    assert_eq!(one.enrollment_status, "ACTIVE", "지금은 다시 수강중이다");
    assert!(one.needs_check, "금액이 달라졌으니 확인을 구해야 한다");
    assert_eq!(one.fees[0].base_amount, 50_000, "환불 기준은 그대로");
    // 지금 기준이면 환불할 것이 없다
    let d0 = f.diffs();
    assert_eq!(d0.iter().find(|x| x.item_name == "강사료").unwrap().suggested, 0);
}

// ─────────────────────────────────── 지우기를 막는다

#[test]
fn 기록이_있으면_학생과_부서를_지우지_못한다() {
    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    f.enroll(a, d, Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None }));

    for (이름, err) in [
        ("학생", f.db.write(|c| repo::student::delete_many(c, &[a])).unwrap_err()),
        ("부서", f.db.write(|c| repo::department::delete_many(c, &[d])).unwrap_err()),
        ("수강 전체", f.db.write(|c| repo::enrollment::delete_all(c, f.ws)).unwrap_err()),
        ("작업공간", f.db.write(|c| repo::year::delete_workspace(c, f.ws)).unwrap_err()),
        ("학년도", f.db.write(|c| repo::year::delete_year(c, f.year)).unwrap_err()),
    ] {
        let msg = format!("{err:?}");
        assert!(msg.contains("추가징수·환불 기록"), "{이름}: {msg}");
    }

    // 기록을 지우면 그때는 지워진다
    let id = f.전체().additional.details[0].id;
    f.db.write(|c| repo::adjustment::delete_many(c, &[id])).unwrap();
    assert_eq!(f.전체().additional.count, 0);
    f.db.write(|c| repo::student::delete_many(c, &[a])).unwrap();
}

// ─────────────────────────────────── Excel

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-adj-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn 파일(f: &F, filter: &AdjustmentFilter, tag: &str) -> crate::error::AppResult<PathBuf> {
    let dir = tmp_dir(tag);
    let v = f.view(filter);
    crate::excel::adjustment::write_adjustments(&v, &f.items, &["2026학년도", "9월"], "", &dir)
        .map(|r| PathBuf::from(&r.path))
}

/// 추가징수 두 건(한 학생) · 환불 한 건.
fn 섞인_상황(f: &F) {
    let a = f.student(1, "가람", 1, "홍길동");
    let b = f.student(2, "나리", 1, "이두리");
    let 로봇 = f.dept("로봇과학", "B반", 원래_금액());
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000), fee(재료비, 7_000)]);
    let adj = |d: &str| Some(AdjustmentInput { occurred_on: Some(d.into()), note: None });

    f.enroll(a, 로봇, adj("2026-09-10"));
    f.enroll(a, 미술, adj("2026-09-12"));

    let e = f.enroll(b, 로봇, None);
    f.cancel(e, Some(&취소후_금액()), adj("2026-09-20")).unwrap();
}

#[test]
fn Excel은_한_파일에_세_장이다() {
    use crate::excel::read;

    let f = F::new();
    섞인_상황(&f);
    let path = 파일(&f, &AdjustmentFilter::default(), "three").unwrap();

    let 추가 = read::read_sheet_at(&path, 0).unwrap();
    let 환불 = read::read_sheet_at(&path, 1).unwrap();
    let 상세 = read::read_sheet_at(&path, 2).unwrap();

    for name in ["학년", "반", "번호", "이름", "합계"] {
        assert!(추가.require(name).is_ok(), "1장 '{name}' 열이 없다");
        assert!(환불.require(name).is_ok(), "2장 '{name}' 열이 없다");
    }
    for name in ["구분", "학년", "반", "번호", "이름", "부서", "합계", "발생일"] {
        assert!(상세.require(name).is_ok(), "3장 '{name}' 열이 없다");
    }
    // 학생별 장에는 부서·발생일이 없다 — 학생 한 줄에 담기지 않는다
    assert!(추가.col("부서").is_none());
    assert!(추가.col("발생일").is_none());
}

#[test]
fn Excel_학생별_장과_상세_장의_총액이_맞는다() {
    use crate::excel::read;

    let f = F::new();
    섞인_상황(&f);
    let path = 파일(&f, &AdjustmentFilter::default(), "sum").unwrap();

    let 합 = |s: &read::Sheet, 합계줄: bool| -> i64 {
        s.rows
            .iter()
            .filter(|(_, c)| 합계줄 || c.first().map(|v| v.trim()) != Some("합계"))
            .map(|(_, c)| read::parse_amount(s.cell(c, s.col("합계"))).unwrap())
            .sum()
    };

    let 추가 = read::read_sheet_at(&path, 0).unwrap();
    let 환불 = read::read_sheet_at(&path, 1).unwrap();
    let 상세 = read::read_sheet_at(&path, 2).unwrap();

    let 상세합 = |구분: &str| -> i64 {
        상세.rows
            .iter()
            .filter(|(_, c)| 상세.cell(c, 상세.col("구분")) == 구분)
            .map(|(_, c)| read::parse_amount(상세.cell(c, 상세.col("합계"))).unwrap())
            .sum()
    };

    // 1장 학생별 합계 = 3장 추가징수 합계
    assert_eq!(합(&추가, false), 상세합("추가징수"), "1장 ≠ 3장 추가징수");
    assert_eq!(합(&추가, false), 85_000 + 27_000);
    // 2장 = 3장 환불 합계
    assert_eq!(합(&환불, false), 상세합("환불"), "2장 ≠ 3장 환불");
    assert_eq!(합(&환불, false), 22_000);

    // 화면 결과와도 같아야 한다
    let v = f.전체();
    assert_eq!(합(&추가, false), v.additional.total);
    assert_eq!(합(&환불, false), v.refund.total);

    // 1장은 학생당 한 줄 — 홍길동은 두 부서지만 한 줄이다
    assert_eq!(추가.rows.len(), 2, "학생 1줄 + 합계 1줄");
    assert_eq!(상세.rows.len(), 3, "추가징수 2건 + 환불 1건");
}

#[test]
fn Excel도_기간을_그대로_따른다() {
    use crate::excel::read;

    let f = F::new();
    섞인_상황(&f);
    let path = 파일(
        &f,
        &AdjustmentFilter {
            from: Some("2026-09-11".into()),
            to: Some("2026-09-19".into()),
            ..Default::default()
        },
        "period",
    )
    .unwrap();

    let 상세 = read::read_sheet_at(&path, 2).unwrap();
    assert_eq!(상세.rows.len(), 1, "9/12 미술 한 건만");
    assert_eq!(상세.cell(&상세.rows[0].1, 상세.col("부서")), "미술A반");
}

#[test]
fn 한쪽이_비어_있어도_파일은_만들어진다() {
    use crate::excel::read;

    let f = F::new();
    let a = f.student(1, "가람", 1, "홍길동");
    let d = f.dept("로봇과학", "B반", vec![fee(강사료, 10_000)]);
    f.enroll(a, d, Some(AdjustmentInput { occurred_on: Some("2026-09-10".into()), note: None }));

    let path = 파일(&f, &AdjustmentFilter::default(), "empty-side").unwrap();
    let 추가 = read::read_sheet_at(&path, 0).unwrap();
    let 환불 = read::read_sheet_at(&path, 1).unwrap();
    assert_eq!(추가.rows.len(), 2, "자료 1줄 + 합계");
    assert!(환불.rows.is_empty(), "환불이 없으면 머리글만 있는 빈 장");
    assert!(환불.require("합계").is_ok(), "머리글은 있어야 한다");
}

#[test]
fn 확인이_필요한_기록이_있으면_Excel을_만들지_않는다() {
    let f = F::new();
    let (e, id) = 추가징수_하나(&f);
    f.금액수정(e, &[fee(강사료, 45_000), fee(수용비, 5_000), fee(교재비, 20_000), fee(재료비, 10_000)]);

    let err = 파일(&f, &AdjustmentFilter::default(), "blocked").unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("확인이 필요한 내역이 1건"), "{msg}");

    // 확인하고 나면 만들어진다
    f.confirm(&[id], KEEP).unwrap();
    assert!(파일(&f, &AdjustmentFilter::default(), "blocked2").is_ok());
}
