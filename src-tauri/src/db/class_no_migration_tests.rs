#![allow(non_snake_case)]
//! 반을 숫자에서 문자로 바꾼 마이그레이션(004)이 기존 자료를 지키는지.
//!
//! 이 시험이 이 변경에서 가장 중요하다. 이미 v0.1.1 을 쓰고 있는 학교가 있고,
//! 그 학교의 숫자 반 자료가 뜻이 바뀌거나 사라지면 안 된다. 특히 student 표를
//! 갈아 끼우므로 **수강·지원대상자가 딸려 지워지지 않는지** 를 봐야 한다.

use rusqlite::Connection;

use crate::db::{migrate, Db};

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-mig-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 004 직전(003)까지만 적용하고, 숫자 반 학생과 딸린 자료를 넣어 둔다.
fn 옛_자료(path: &std::path::Path) {
    let mut conn = Connection::open(path).unwrap();
    migrate::run_up_to(&mut conn, 3).unwrap();
    conn.execute_batch(
        "INSERT INTO academic_year (id, year, name, start_date, end_date, is_current)
              VALUES (1, 2026, '2026학년도', '2026-03-01', '2027-02-28', 1);
         INSERT INTO workspace (id, year_id, name, start_date, end_date)
              VALUES (1, 1, '4월', '2026-04-01', '2026-04-30');

         -- 숫자 반 학생 넷. 10반이 있어야 자연 정렬을 볼 수 있다.
         INSERT INTO student (id, year_id, grade, class_no, student_no, name, note) VALUES
           (1, 1, 3,  1, 1, '김하나', '비고1'),
           (2, 1, 3,  2, 1, '이두리', ''),
           (3, 1, 3, 10, 1, '박세찬', ''),
           (4, 1, 3,  1, 2, '최네울', '');

         INSERT INTO department (id, workspace_id, name, class_name)
              VALUES (1, 1, '로봇과학', 'A반');

         -- student(id) 를 ON DELETE CASCADE 로 참조하는 표들
         INSERT INTO enrollment (id, workspace_id, student_id, department_id, status)
              VALUES (1, 1, 1, 1, 'ACTIVE'), (2, 1, 3, 1, 'ACTIVE');
         INSERT INTO charge (enrollment_id, item_code, amount)
              VALUES (1, 'INSTRUCTOR', 40000), (2, 'INSTRUCTOR', 40000);
         INSERT INTO support_eligibility (year_id, student_id, program, source)
              VALUES (1, 1, 'VOUCHER', 'MANUAL');
         INSERT INTO support_grant (year_id, student_id, program, amount)
              VALUES (1, 2, 'VOUCHER', 300000);",
    )
    .unwrap();
}

fn 한줄들(conn: &Connection, sql: &str) -> Vec<String> {
    let mut st = conn.prepare(sql).unwrap();
    st.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn 건수(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn 기존_숫자_반이_같은_뜻의_문자로_남는다() {
    let dir = tmp_dir("keep");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(migrate::current_version(c)?, migrate::latest_version());
        // 1 → '1', 2 → '2', 10 → '10'. 뜻이 바뀌지 않는다.
        let 반 = 한줄들(c, "SELECT class_no FROM student ORDER BY id");
        assert_eq!(반, vec!["1", "2", "10", "1"]);
        // 진짜 문자로 저장되었는지 본다. 숫자로 남아 있으면 매칭이 어긋난다.
        let 타입 = 한줄들(c, "SELECT typeof(class_no) FROM student");
        assert!(타입.iter().all(|t| t == "text"), "{타입:?}");
        Ok(())
    })
    .unwrap();
}

#[test]
fn 학생에_딸린_자료가_지워지지_않는다() {
    // student 표를 DROP 하므로, 외래키를 켠 채였다면 여기가 0 이 된다.
    let dir = tmp_dir("cascade");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(건수(c, "student"), 4, "학생이 사라졌다");
        assert_eq!(건수(c, "enrollment"), 2, "수강이 딸려 지워졌다");
        assert_eq!(건수(c, "charge"), 2, "금액이 딸려 지워졌다");
        assert_eq!(건수(c, "support_eligibility"), 1, "지원대상자가 딸려 지워졌다");
        assert_eq!(건수(c, "support_grant"), 1, "학생별 예외가 딸려 지워졌다");
        Ok(())
    })
    .unwrap();
}

#[test]
fn 학생과_수강의_연결이_그대로다() {
    let dir = tmp_dir("link");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        let 이름 = 한줄들(
            c,
            "SELECT s.name FROM enrollment e JOIN student s ON s.id = e.student_id
              ORDER BY e.id",
        );
        assert_eq!(이름, vec!["김하나", "박세찬"]);
        // 외래키가 어긋난 곳이 하나도 없어야 한다
        let mut st = c.prepare("PRAGMA foreign_key_check")?;
        assert_eq!(st.query_map([], |_| Ok(()))?.count(), 0);
        Ok(())
    })
    .unwrap();
}

#[test]
fn 옮긴_뒤에도_자연정렬로_나온다() {
    let dir = tmp_dir("sort");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        let 반 = 한줄들(
            c,
            "SELECT class_no FROM student ORDER BY grade, class_sort, class_no, student_no",
        );
        // 사전순이면 1, 1, 10, 2 가 된다. 그게 아니어야 한다.
        assert_eq!(반, vec!["1", "1", "2", "10"]);
        Ok(())
    })
    .unwrap();
}

#[test]
fn 이름과_비고도_그대로다() {
    let dir = tmp_dir("fields");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        let v = 한줄들(c, "SELECT name || '/' || note FROM student ORDER BY id");
        assert_eq!(v[0], "김하나/비고1");
        assert_eq!(v[1], "이두리/");
        Ok(())
    })
    .unwrap();
}

#[test]
fn 학년이_바뀔_때_올리는_트리거가_살아_있다() {
    // 표를 지우면 트리거도 함께 사라진다. 004 가 다시 만들어야 한다.
    let dir = tmp_dir("trigger");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.write(|c| {
        let before: i64 =
            c.query_row("SELECT data_version FROM academic_year WHERE id = 1", [], |r| r.get(0))?;
        c.execute("UPDATE student SET grade = 4 WHERE id = 1", [])?;
        let after: i64 =
            c.query_row("SELECT data_version FROM academic_year WHERE id = 1", [], |r| r.get(0))?;
        assert_eq!(after, before + 1, "학년을 바꿨는데 자료판이 올라가지 않았다");

        // 반만 바꿀 때는 올리지 않는다 — 금액에 영향이 없다.
        c.execute("UPDATE student SET class_no = '가' WHERE id = 1", [])?;
        let last: i64 =
            c.query_row("SELECT data_version FROM academic_year WHERE id = 1", [], |r| r.get(0))?;
        assert_eq!(last, after, "반만 바꿨는데 '다시 정산하세요'가 뜬다");
        Ok(())
    })
    .unwrap();
}

#[test]
fn 옮긴_뒤에_한글_반을_넣을_수_있다() {
    let dir = tmp_dir("mixed");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    db.write(|c| {
        c.execute(
            "INSERT INTO student (year_id, grade, class_no, student_no, name)
             VALUES (1, 3, '가', 1, '새학생')",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    db.read(|c| {
        let 반 = 한줄들(
            c,
            "SELECT class_no FROM student WHERE grade = 3
              ORDER BY class_sort, class_no, student_no",
        );
        assert_eq!(반, vec!["1", "1", "2", "10", "가"]);
        Ok(())
    })
    .unwrap();
}

#[test]
fn 마이그레이션_직전_백업이_남는다() {
    let dir = tmp_dir("backup");
    let path = dir.join("old.db");
    옛_자료(&path);

    let db = Db::open(&path).unwrap();
    let before: Vec<_> = db
        .backups()
        .unwrap()
        .into_iter()
        .filter(|f| f.name.starts_with("before-migrate-v3-"))
        .collect();
    assert_eq!(before.len(), 1, "004 를 적용하기 전 백업이 있어야 한다");

    // 그 백업은 옛 구조 그대로여야 한다 — 되돌릴 수 있어야 하므로.
    let old = Connection::open(dir.join("backups").join(&before[0].name)).unwrap();
    assert_eq!(migrate::current_version(&old).unwrap(), 3);
    assert_eq!(건수(&old, "student"), 4);
}
