//! Phase 2 시나리오 점검.
//!
//! 가장 중요한 흐름은 하나다 —
//! **부서 기준금액을 고쳐도 이미 등록된 학생의 금액이 저절로 바뀌지 않는다.**
//! 그리고 사람이 반영을 눌렀을 때도 학생별로 고쳐 둔 값은 지켜져야 한다.

use crate::db::Db;
use crate::model::{
    CostItem, DepartmentInput, EnrollmentFilter, EnrollmentInput, Fee, FeePick, StudentInput,
    WorkspaceInput,
};
use crate::repo;
use crate::repo::enrollment::StudentFeeEdit;

const INSTRUCTOR: &str = "INSTRUCTOR";
const TEXTBOOK: &str = "TEXTBOOK";
const MATERIAL: &str = "MATERIAL";

struct Fixture {
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

fn setup() -> Fixture {
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
    Fixture { db, year, ws, items }
}

impl Fixture {
    fn student(&self, grade: i64, class_no: i64, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no,
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn department(&self, name: &str, fees: Vec<Fee>) -> i64 {
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

    fn enroll(&self, student_id: i64, department_id: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
                    &EnrollmentInput {
                        student_id,
                        department_id,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap()
    }

    fn amount(&self, enrollment_id: i64, code: &str) -> i64 {
        let row = self
            .db
            .read(|c| repo::enrollment::get(c, enrollment_id, &self.items))
            .unwrap();
        row.fees
            .iter()
            .find(|f| f.item_code == code)
            .map(|f| f.amount)
            .unwrap_or(-1)
    }

    fn roster(&self) -> Vec<crate::model::Enrollment> {
        self.db
            .read(|c| {
                repo::enrollment::list(c, self.ws, &self.items, &EnrollmentFilter::default())
            })
            .unwrap()
    }

    fn logs(&self) -> Vec<crate::model::ChangeLog> {
        self.db
            .read(|c| repo::change_log::list(c, self.year, None, None, None, 200))
            .unwrap()
    }
}

// ─────────────────────────────────────────────── 요청하신 전체 흐름

#[test]
fn 전체_흐름_부서금액_변경이_기존_학생에게_저절로_옮겨가지_않는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let duri = f.student(3, 1, 2, "이두리");
    let robot = f.department(
        "로봇과학",
        vec![fee(INSTRUCTOR, 40_000), fee(TEXTBOOK, 30_000)],
    );

    // ① 수강 등록 → 부서 기준금액으로 charge가 만들어진다
    let e_hana = f.enroll(hana, robot);
    let e_duri = f.enroll(duri, robot);
    assert_eq!(f.amount(e_hana, INSTRUCTOR), 40_000);
    assert_eq!(f.amount(e_hana, TEXTBOOK), 30_000);
    assert_eq!(f.amount(e_hana, MATERIAL), 0, "설정하지 않은 항목은 0원");
    assert_eq!(f.amount(e_duri, TEXTBOOK), 30_000);

    // ② 한 학생의 교재비만 면제한다
    f.db.write(|c| {
        repo::enrollment::update_fees(
            c,
            e_hana,
            &[fee(INSTRUCTOR, 40_000), fee(TEXTBOOK, 0), fee(MATERIAL, 0)],
            "교재 자체 준비",
            &f.items,
        )
    })
    .unwrap();
    assert_eq!(f.amount(e_hana, TEXTBOOK), 0);
    assert_eq!(f.amount(e_duri, TEXTBOOK), 30_000, "다른 학생은 그대로여야 한다");

    let hana_row = f
        .db
        .read(|c| repo::enrollment::get(c, e_hana, &f.items))
        .unwrap();
    assert!(hana_row.has_override, "학생별로 고친 표시가 남아야 한다");

    // ③ 부서 기준 강사료를 올린다 → 기존 학생 금액은 그대로
    f.db.write(|c| {
        repo::department::update(
            c,
            robot,
            &DepartmentInput {
                name: "로봇과학".into(),
                class_name: Some("A반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![fee(INSTRUCTOR, 50_000), fee(TEXTBOOK, 30_000)],
            },
        )
    })
    .unwrap();
    assert_eq!(f.amount(e_hana, INSTRUCTOR), 40_000, "자동으로 바뀌면 안 된다");
    assert_eq!(f.amount(e_duri, INSTRUCTOR), 40_000, "자동으로 바뀌면 안 된다");

    // ④ 미리보기 — 바뀔 칸만 나온다
    let diffs = f
        .db
        .read(|c| repo::enrollment::fee_diff(c, f.ws, None, &f.items))
        .unwrap();
    // 김하나: 강사료(40,000→50,000) + 교재비(0→30,000, 학생별 수정)
    // 이두리: 강사료(40,000→50,000)
    assert_eq!(diffs.len(), 3);
    assert_eq!(diffs.iter().filter(|d| d.is_overridden).count(), 1);

    // ⑤ 기본 방식으로 반영 — 학생별로 고친 교재비는 지켜진다
    let result = f
        .db
        .write(|c| {
            repo::enrollment::apply_fees(
                c,
                f.ws,
                None,
                "KEEP_EDITED",
                &[],
                "5월 단가 조정",
                &f.items,
            )
        })
        .unwrap();
    assert_eq!(result.changed, 2, "강사료 두 건만 바뀐다");
    assert_eq!(result.kept, 1, "학생별로 고친 교재비 한 칸은 그대로");
    assert_eq!(f.amount(e_hana, INSTRUCTOR), 50_000);
    assert_eq!(f.amount(e_duri, INSTRUCTOR), 50_000);
    assert_eq!(f.amount(e_hana, TEXTBOOK), 0, "면제해 둔 교재비가 되살아나면 안 된다");

    // ⑥ 수강 취소 — 행은 남는다
    f.db.write(|c| repo::enrollment::cancel(c, e_duri, "개인 사정", &f.items))
        .unwrap();
    let rows = f.roster();
    assert_eq!(rows.len(), 2, "취소해도 명단에서 사라지지 않는다");
    let duri_row = rows.iter().find(|r| r.id == e_duri).unwrap();
    assert_eq!(duri_row.status, "CANCELLED");
    assert_eq!(duri_row.change_reason, "개인 사정");

    // ⑦ 변경이력
    let logs = f.logs();
    let kinds: Vec<&str> = logs.iter().map(|l| l.kind.as_str()).collect();
    assert!(kinds.contains(&"ENROLL_ADD"));
    assert!(kinds.contains(&"CHARGE_EDIT"));
    assert!(kinds.contains(&"DEPT_APPLY"));
    assert!(kinds.contains(&"ENROLL_CANCEL"));

    let cancel_log = logs.iter().find(|l| l.kind == "ENROLL_CANCEL").unwrap();
    assert!(cancel_log.target.contains("이두리"));
    assert!(cancel_log.target.contains("로봇과학A반"));
    assert_eq!(cancel_log.reason, "개인 사정");

    let edit_log = logs.iter().find(|l| l.kind == "CHARGE_EDIT").unwrap();
    assert!(edit_log.before_value.contains("교재비 30,000"));
    assert!(edit_log.after_value.contains("교재비 0"));
    assert_eq!(edit_log.reason, "교재 자체 준비");
}

// ─────────────────────────────────────────────── 반영 방식

#[test]
fn 전체_반영은_학생별_수정까지_덮어쓴다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("미술", vec![fee(INSTRUCTOR, 25_000), fee(MATERIAL, 15_000)]);
    let e = f.enroll(hana, dept);

    f.db.write(|c| {
        repo::enrollment::update_fees(
            c,
            e,
            &[fee(INSTRUCTOR, 25_000), fee(MATERIAL, 0), fee(TEXTBOOK, 0)],
            "재료 지참",
            &f.items,
        )
    })
    .unwrap();

    let r = f
        .db
        .write(|c| repo::enrollment::apply_fees(c, f.ws, None, "ALL", &[], "전체 맞춤", &f.items))
        .unwrap();
    assert_eq!(r.changed, 1);
    assert_eq!(r.kept, 0);
    assert_eq!(f.amount(e, MATERIAL), 15_000, "전체 반영은 수정값도 덮는다");
}

#[test]
fn 고른_칸만_반영할_수_있다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let duri = f.student(3, 1, 2, "이두리");
    let dept = f.department("축구", vec![fee(INSTRUCTOR, 20_000)]);
    let e1 = f.enroll(hana, dept);
    let e2 = f.enroll(duri, dept);

    // 기준금액을 올린다
    f.db.write(|c| {
        repo::department::update(
            c,
            dept,
            &DepartmentInput {
                name: "축구".into(),
                class_name: Some("A반".into()),
                teacher: None,
                days: None,
                note: None,
                fees: vec![fee(INSTRUCTOR, 30_000)],
            },
        )
    })
    .unwrap();

    let r = f
        .db
        .write(|c| {
            repo::enrollment::apply_fees(
                c,
                f.ws,
                None,
                "SELECTED",
                &[FeePick {
                    enrollment_id: e1,
                    item_code: INSTRUCTOR.into(),
                }],
                "한 명만",
                &f.items,
            )
        })
        .unwrap();
    assert_eq!(r.changed, 1);
    assert_eq!(f.amount(e1, INSTRUCTOR), 30_000);
    assert_eq!(f.amount(e2, INSTRUCTOR), 20_000, "고르지 않은 학생은 그대로");
}

#[test]
fn 반영할_것이_없으면_아무_일도_일어나지_않는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("컴퓨터", vec![fee(INSTRUCTOR, 20_000)]);
    f.enroll(hana, dept);

    let diffs = f
        .db
        .read(|c| repo::enrollment::fee_diff(c, f.ws, None, &f.items))
        .unwrap();
    assert!(diffs.is_empty(), "같은 값은 미리보기에 나오지 않는다");

    let r = f
        .db
        .write(|c| repo::enrollment::apply_fees(c, f.ws, None, "KEEP_EDITED", &[], "", &f.items))
        .unwrap();
    assert_eq!(r.changed, 0);
    assert_eq!(r.enrollments, 0);
    assert!(
        f.logs().iter().all(|l| l.kind != "DEPT_APPLY"),
        "바뀐 것이 없으면 이력도 남기지 않는다"
    );
}

// ─────────────────────────────────────────────── 중복 · 취소 · 복원

#[test]
fn 같은_학생이_같은_부서를_두_번_수강할_수_없다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);

    let err = f
        .db
        .write(|c| {
            repo::enrollment::create(
                c,
                f.ws,
                &EnrollmentInput {
                    student_id: hana,
                    department_id: dept,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )
        })
        .unwrap_err();
    assert!(err.message.contains("이미 수강 중"), "{}", err.message);
}

#[test]
fn 한_학생이_여러_부서를_수강할_수_있다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let robot = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let art = f.department("미술", vec![fee(INSTRUCTOR, 25_000)]);
    f.enroll(hana, robot);
    f.enroll(hana, art);
    assert_eq!(f.roster().len(), 2);
}

#[test]
fn 취소한_뒤_다시_등록할_수_있다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let e = f.enroll(hana, dept);

    f.db.write(|c| repo::enrollment::cancel(c, e, "전학", &f.items))
        .unwrap();
    // 취소 상태에서는 다시 등록이 된다 (부분 유니크 인덱스)
    let e2 = f.enroll(hana, dept);
    assert_ne!(e, e2);
    assert_eq!(f.roster().len(), 2);
}

#[test]
fn 취소_사유는_반드시_받는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let e = f.enroll(hana, dept);

    let err = f
        .db
        .write(|c| repo::enrollment::cancel(c, e, "   ", &f.items))
        .unwrap_err();
    assert!(err.message.contains("변경사유"));
}

#[test]
fn 복원은_같은_수강이_이미_있으면_막힌다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let e = f.enroll(hana, dept);
    f.db.write(|c| repo::enrollment::cancel(c, e, "착오", &f.items))
        .unwrap();
    f.enroll(hana, dept); // 새로 등록해 버렸다

    let err = f
        .db
        .write(|c| repo::enrollment::restore(c, e, "되돌림", &f.items))
        .unwrap_err();
    assert!(err.message.contains("이미 수강 중"), "{}", err.message);
}

#[test]
fn 복원하면_다시_수강중이_되고_이력이_남는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let e = f.enroll(hana, dept);
    f.db.write(|c| repo::enrollment::cancel(c, e, "착오", &f.items))
        .unwrap();
    f.db.write(|c| repo::enrollment::restore(c, e, "취소 착오", &f.items))
        .unwrap();

    let row = f
        .db
        .read(|c| repo::enrollment::get(c, e, &f.items))
        .unwrap();
    assert_eq!(row.status, "ACTIVE");
    assert!(f.logs().iter().any(|l| l.kind == "ENROLL_RESTORE"));
}

// ─────────────────────────────────────────────── 명단 · 필터

#[test]
fn 명단은_수강_자료가_있는_학생만_보여_준다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    f.student(3, 1, 2, "이두리"); // 수강하지 않는 학생
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);

    let rows = f.roster();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "김하나");
}

#[test]
fn 수강상태와_부서로_거를_수_있다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let duri = f.student(4, 2, 3, "이두리");
    let robot = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let art = f.department("미술", vec![fee(INSTRUCTOR, 25_000)]);
    let e1 = f.enroll(hana, robot);
    f.enroll(duri, art);
    f.db.write(|c| repo::enrollment::cancel(c, e1, "사정", &f.items))
        .unwrap();

    let only_active = f
        .db
        .read(|c| {
            repo::enrollment::list(
                c,
                f.ws,
                &f.items,
                &EnrollmentFilter {
                    status: Some("ACTIVE".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(only_active.len(), 1);
    assert_eq!(only_active[0].name, "이두리");

    let only_robot = f
        .db
        .read(|c| {
            repo::enrollment::list(
                c,
                f.ws,
                &f.items,
                &EnrollmentFilter {
                    department_id: Some(robot),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(only_robot.len(), 1);
    assert_eq!(only_robot[0].name, "김하나");

    let grade4 = f
        .db
        .read(|c| {
            repo::enrollment::list(
                c,
                f.ws,
                &f.items,
                &EnrollmentFilter {
                    grade: Some(4),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert_eq!(grade4.len(), 1);
    assert_eq!(grade4[0].name, "이두리");
}

#[test]
fn 다른_작업공간의_부서에는_등록할_수_없다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    let other = f
        .db
        .write(|c| {
            repo::year::create_workspace(
                c,
                f.year,
                &WorkspaceInput {
                    name: "2026년 5월".into(),
                    start_date: "2026-05-01".into(),
                    end_date: "2026-05-31".into(),
                    note: None,
                },
            )
        })
        .unwrap();

    let err = f
        .db
        .write(|c| {
            repo::enrollment::create(
                c,
                other,
                &EnrollmentInput {
                    student_id: hana,
                    department_id: dept,
                    fees: Vec::new(),
                    reason: None,
                },
                &f.items,
            )
        })
        .unwrap_err();
    assert!(err.message.contains("다른 작업공간의 부서"), "{}", err.message);
}

// ─────────────────────────────────────────────── 학생별 금액 수정 팝업

#[test]
fn 학생별_수정은_바뀐_학생만_저장하고_부서금액은_건드리지_않는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let duri = f.student(3, 1, 2, "이두리");
    let dept = f.department(
        "미술",
        vec![fee(INSTRUCTOR, 25_000), fee(MATERIAL, 15_000)],
    );
    let e1 = f.enroll(hana, dept);
    let e2 = f.enroll(duri, dept);

    let changed = f
        .db
        .write(|c| {
            repo::enrollment::save_student_fees(
                c,
                &[
                    StudentFeeEdit {
                        enrollment_id: e1,
                        fees: vec![fee(INSTRUCTOR, 25_000), fee(MATERIAL, 0), fee(TEXTBOOK, 0)],
                    },
                    // 이두리는 그대로 — 저장 대상이 아니어야 한다
                    StudentFeeEdit {
                        enrollment_id: e2,
                        fees: vec![
                            fee(INSTRUCTOR, 25_000),
                            fee(MATERIAL, 15_000),
                            fee(TEXTBOOK, 0),
                        ],
                    },
                ],
                "재료 지참",
                &f.items,
            )
        })
        .unwrap();
    assert_eq!(changed, 1, "실제로 바뀐 학생만 센다");
    assert_eq!(f.amount(e1, MATERIAL), 0);
    assert_eq!(f.amount(e2, MATERIAL), 15_000);

    // 부서 기준금액은 그대로여야 한다
    let base = f
        .db
        .read(|c| repo::department::fees_of(c, dept))
        .unwrap();
    assert_eq!(
        base.iter().find(|b| b.item_code == MATERIAL).unwrap().amount,
        15_000
    );
}

#[test]
fn 금액을_기준값으로_되돌리면_학생별_수정_표시가_사라진다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("미술", vec![fee(INSTRUCTOR, 25_000)]);
    let e = f.enroll(hana, dept);

    f.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(INSTRUCTOR, 10_000)], "할인", &f.items)
    })
    .unwrap();
    assert!(
        f.db.read(|c| repo::enrollment::get(c, e, &f.items))
            .unwrap()
            .has_override
    );

    f.db.write(|c| {
        repo::enrollment::update_fees(c, e, &[fee(INSTRUCTOR, 25_000)], "원복", &f.items)
    })
    .unwrap();
    assert!(
        !f.db
            .read(|c| repo::enrollment::get(c, e, &f.items))
            .unwrap()
            .has_override,
        "기준값과 같아지면 더 이상 '수정됨'이 아니다"
    );
}

#[test]
fn 음수_금액은_받지_않는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("미술", vec![fee(INSTRUCTOR, 25_000)]);
    let e = f.enroll(hana, dept);

    let err = f
        .db
        .write(|c| {
            repo::enrollment::update_fees(c, e, &[fee(INSTRUCTOR, -1)], "잘못", &f.items)
        })
        .unwrap_err();
    assert!(err.message.contains("0원 이상"));
}

// ─────────────────────────────────────────────── 학생 상세정보

#[test]
fn 상세정보는_자격이_없는_제도를_해당없음으로_돌려_준다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);

    let d = f
        .db
        .read(|c| repo::enrollment::student_detail(c, f.year, hana, &f.items))
        .unwrap();
    assert_eq!(d.student.name, "김하나");
    assert_eq!(d.supports.len(), 2);
    assert!(d.supports.iter().all(|s| !s.eligible), "자격이 없으면 해당없음");
    assert_eq!(d.workspaces.len(), 1);
    assert_eq!(d.workspaces[0].rows.len(), 1);
    assert_eq!(d.workspaces[0].active_total, 40_000);
}

#[test]
fn 상세정보는_자격_기간을_문구로_보여_준다() {
    use crate::model::EligibilityInput;
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    f.db.write(|c| {
        repo::eligibility::create(
            c,
            f.year,
            &EligibilityInput {
                student_id: hana,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();
    f.db.write(|c| {
        repo::eligibility::create(
            c,
            f.year,
            &EligibilityInput {
                student_id: hana,
                program: "FREE_VOUCHER".into(),
                valid_from: Some("2026-04-01".into()),
                valid_to: Some("2026-08-31".into()),
                note: None,
            },
        )
    })
    .unwrap();

    let d = f
        .db
        .read(|c| repo::enrollment::student_detail(c, f.year, hana, &f.items))
        .unwrap();
    let voucher = d.supports.iter().find(|s| s.program == "VOUCHER").unwrap();
    assert!(voucher.eligible);
    assert_eq!(voucher.periods, vec!["학년도 내내"]);

    let free = d.supports.iter().find(|s| s.program == "FREE_VOUCHER").unwrap();
    assert_eq!(free.periods, vec!["2026-04-01 ~ 2026-08-31"]);
}

#[test]
fn 상세정보는_수강이_없는_작업공간을_보여_주지_않는다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    f.db.write(|c| {
        repo::year::create_workspace(
            c,
            f.year,
            &WorkspaceInput {
                name: "2026년 5월".into(),
                start_date: "2026-05-01".into(),
                end_date: "2026-05-31".into(),
                note: None,
            },
        )
    })
    .unwrap();
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);

    let d = f
        .db
        .read(|c| repo::enrollment::student_detail(c, f.year, hana, &f.items))
        .unwrap();
    assert_eq!(d.workspaces.len(), 1, "4월만 나온다");
    assert_eq!(d.workspaces[0].workspace_name, "2026년 4월");
}

// ─────────────────────────────────────────────── 지원유형 표시

#[test]
fn 명단의_지원유형은_작업공간_기간에_유효한_자격만_센다() {
    use crate::model::EligibilityInput;
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);

    // 3월에 끝난 자격 — 4월 작업공간에서는 잡히면 안 된다
    f.db.write(|c| {
        repo::eligibility::create(
            c,
            f.year,
            &EligibilityInput {
                student_id: hana,
                program: "VOUCHER".into(),
                valid_from: None,
                valid_to: Some("2026-03-31".into()),
                note: None,
            },
        )
    })
    .unwrap();
    assert!(f.roster()[0].programs.is_empty());

    // 4월부터 유효한 자유수강권
    f.db.write(|c| {
        repo::eligibility::create(
            c,
            f.year,
            &EligibilityInput {
                student_id: hana,
                program: "FREE_VOUCHER".into(),
                valid_from: Some("2026-04-01".into()),
                valid_to: None,
                note: None,
            },
        )
    })
    .unwrap();
    assert_eq!(f.roster()[0].programs, vec!["FREE_VOUCHER"]);
}

// ─────────────────────────────────────────────── 이력 조회

#[test]
fn 이력은_학생과_작업공간으로_좁힐_수_있다() {
    let f = setup();
    let hana = f.student(3, 1, 1, "김하나");
    let duri = f.student(3, 1, 2, "이두리");
    let dept = f.department("로봇과학", vec![fee(INSTRUCTOR, 40_000)]);
    f.enroll(hana, dept);
    f.enroll(duri, dept);

    let mine = f
        .db
        .read(|c| repo::change_log::list(c, f.year, None, Some(hana), None, 100))
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].target.contains("김하나"));

    let adds = f
        .db
        .read(|c| repo::change_log::list(c, f.year, Some(f.ws), None, Some("ENROLL_ADD"), 100))
        .unwrap();
    assert_eq!(adds.len(), 2);
    assert_eq!(adds[0].kind_label, "수강 추가");
    assert_eq!(adds[0].workspace_name, "2026년 4월");
}

#[test]
fn 금액_문구는_천단위_구분을_쓴다() {
    use crate::repo::enrollment::won;
    assert_eq!(won(0), "0");
    assert_eq!(won(1_000), "1,000");
    assert_eq!(won(40_000), "40,000");
    assert_eq!(won(1_234_567), "1,234,567");
    assert_eq!(won(-500), "-500");
}
