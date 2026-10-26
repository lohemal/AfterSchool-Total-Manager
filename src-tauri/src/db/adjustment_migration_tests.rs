//! 008 승격 시험 — v0.1.8 까지 만든 추가징수·환불 기록이 그대로 남는가 (v0.1.9).
//!
//! 실사용 DB 에는 기록이 한 건도 없어 그쪽으로는 이것을 볼 수 없었다. 그래서
//! 007 까지만 적용한 DB 에 v0.1.8 이 만들던 꼴 그대로 기록을 넣어 두고, 앱이
//! 열 때 008 을 거치게 한다.

use rusqlite::Connection;

use crate::db::{migrate, Db};
use crate::repo;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-mig008-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 007 까지 적용하고 v0.1.8 의 두 길(중도 추가 · 수강 취소)이 남기던 기록을 넣는다.
///
/// `source` 칸은 아직 없다. 그 칸이 생기는 것이 008 이다.
fn 옛_기록(path: &std::path::Path) {
    let mut conn = Connection::open(path).unwrap();
    migrate::run_up_to(&mut conn, 7).unwrap();
    conn.execute_batch(
        "INSERT INTO academic_year (id, year, name, start_date, end_date, is_current)
              VALUES (1, 2026, '2026학년도', '2026-03-01', '2027-02-28', 1);
         INSERT INTO workspace (id, year_id, name, start_date, end_date, is_current)
              VALUES (1, 1, '10월', '2026-10-01', '2026-10-31', 1);
         INSERT INTO student (id, year_id, grade, class_no, student_no, name)
              VALUES (1, 1, 3, '1', 1, '홍길동'), (2, 1, 3, '1', 2, '이두리');
         INSERT INTO department (id, workspace_id, name, class_name)
              VALUES (1, 1, '미술', 'A');
         INSERT INTO department_fee (department_id, item_code, amount)
              VALUES (1, 'INSTRUCTOR', 35000);

         -- 1) 중도 추가: 수강중 35,000 · 추가징수 전액 35,000
         INSERT INTO enrollment (id, workspace_id, student_id, department_id, status)
              VALUES (1, 1, 1, 1, 'ACTIVE');
         INSERT INTO charge (enrollment_id, item_code, amount) VALUES (1, 'INSTRUCTOR', 35000);
         INSERT INTO billing_adjustment
              (id, workspace_id, student_id, enrollment_id, department_id, kind, occurred_on, note)
              VALUES (1, 1, 1, 1, 1, 'ADDITIONAL_CHARGE', '2026-10-05', '중도 추가');
         INSERT INTO billing_adjustment_amount
              (adjustment_id, item_code, amount, base_amount, checked_charge)
              VALUES (1, 'INSTRUCTOR', 35000, 0, 35000);

         -- 2) 수강 취소: 35,000 → 20,000 · 환불 15,000
         INSERT INTO enrollment (id, workspace_id, student_id, department_id, status)
              VALUES (2, 1, 2, 1, 'CANCELLED');
         INSERT INTO charge (enrollment_id, item_code, amount) VALUES (2, 'INSTRUCTOR', 20000);
         INSERT INTO billing_adjustment
              (id, workspace_id, student_id, enrollment_id, department_id, kind, occurred_on, note)
              VALUES (2, 1, 2, 2, 1, 'REFUND', '2026-10-15', '10/15 취소');
         INSERT INTO billing_adjustment_amount
              (adjustment_id, item_code, amount, base_amount, checked_charge)
              VALUES (2, 'INSTRUCTOR', 15000, 35000, 20000);",
    )
    .unwrap();
}

fn 금액들(c: &Connection) -> Vec<(i64, i64, i64, i64)> {
    let mut st = c
        .prepare(
            "SELECT adjustment_id, amount, base_amount, checked_charge
               FROM billing_adjustment_amount ORDER BY adjustment_id, item_code",
        )
        .unwrap();
    st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn 옛_기록은_기존_기록으로_남고_금액이_그대로다() {
    let dir = tmp_dir("keep");
    let path = dir.join("old.db");
    옛_기록(&path);

    let 옛값 = {
        let c = Connection::open(&path).unwrap();
        금액들(&c)
    };

    let db = Db::open(&path).unwrap(); // 여기서 008 을 거친다
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, migrate::latest_version());

        // 발생 유형은 추측하지 않는다 — 둘 다 LEGACY
        let mut st = c.prepare("SELECT source, batch_id IS NULL FROM billing_adjustment ORDER BY id")?;
        let rows: Vec<(String, bool)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        assert_eq!(
            rows,
            vec![("LEGACY".to_string(), true), ("LEGACY".to_string(), true)],
            "기존 기록에 발생 유형을 추측해 넣었다"
        );

        // 금액·기준·확인값이 한 원도 바뀌지 않았다
        assert_eq!(금액들(c), 옛값, "008 이 기존 금액을 건드렸다");

        // 명단에도 그대로 보인다
        let items = repo::cost_items(c)?;
        let v = repo::adjustment::view(c, 1, &Default::default(), &items)?;
        assert_eq!(v.additional.total, 35_000);
        assert_eq!(v.refund.total, 15_000);
        assert_eq!(v.needs_check_all, 0);
        Ok(())
    })
    .unwrap();
}

/// 옛 기록은 [현재 금액 반영]도 v0.1.8 공식 그대로여야 한다.
#[test]
fn 옛_기록의_현재_금액_반영은_v018_공식_그대로다() {
    let dir = tmp_dir("apply");
    let path = dir.join("old.db");
    옛_기록(&path);
    let db = Db::open(&path).unwrap();
    let items = db.read(repo::cost_items).unwrap();

    // 중도 추가 학생 금액을 40,000 으로, 취소자 최종 금액을 18,000 으로 고친다
    db.write(|c| {
        repo::enrollment::update_fees(c, 1, &[crate::model::Fee { item_code: "INSTRUCTOR".into(), amount: 40_000 }], "정정", &items)?;
        repo::enrollment::update_fees(c, 2, &[crate::model::Fee { item_code: "INSTRUCTOR".into(), amount: 18_000 }], "정정", &items)
    })
    .unwrap();
    db.write(|c| repo::adjustment::confirm(c, &[1, 2], repo::adjustment::APPLY))
        .unwrap();

    let v = db
        .read(|c| repo::adjustment::view(c, 1, &Default::default(), &items))
        .unwrap();
    // v0.1.8: 중도 추가 = 지금(40,000) · 취소 환불 = 35,000 - 18,000
    assert_eq!(v.additional.total, 40_000, "중도 추가 공식이 바뀌었다");
    assert_eq!(v.refund.total, 17_000, "취소 환불 공식이 바뀌었다");
}

#[test]
fn 다시_열어도_008_이_다시_돌지_않는다() {
    let dir = tmp_dir("again");
    let path = dir.join("old.db");
    옛_기록(&path);
    drop(Db::open(&path).unwrap());
    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, 8);
        let n: i64 = c.query_row("SELECT COUNT(*) FROM billing_adjustment", [], |r| r.get(0))?;
        assert_eq!(n, 2);
        Ok(())
    })
    .unwrap();
}
