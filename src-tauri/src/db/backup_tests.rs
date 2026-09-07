//! 백업 · 복원 시험 (요구사항 §18).
//!
//! 여기서 가장 중요한 것은 **자료 보존**이다 —
//! 잘못된 파일을 골랐을 때 현재 자료가 멀쩡한지, 복원이 정확히 이전 상태로
//! 돌아가는지를 본다. 메모리 DB로는 파일을 갈아 끼우는 과정을 볼 수 없으므로
//! 임시 폴더에 실제 파일 DB를 만들어 시험한다.

use std::path::PathBuf;

use rusqlite::Connection;

use crate::db::backup::{inspect, BackupKind, KEEP_AUTO};
use crate::db::{migrate, Db};
use crate::model::{StudentInput, WorkspaceInput};
use crate::repo;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-backup-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 파일 DB를 열고 학년도 하나를 만들어 둔다.
fn fixture(tag: &str) -> (PathBuf, Db, i64) {
    let dir = tmp_dir(tag);
    let db = Db::open(&dir.join("afterschool.db")).unwrap();
    let year = db
        .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
        .unwrap();
    (dir, db, year)
}

fn student(db: &Db, year: i64, no: i64, name: &str) {
    db.write(|c| {
        repo::student::create(
            c,
            year,
            &StudentInput {
                grade: 3,
                class_no: "1".into(),
                student_no: no,
                name: name.into(),
                note: None,
            },
        )
    })
    .unwrap();
}

fn student_count(db: &Db, year: i64) -> i64 {
    db.read(|c| repo::student::count(c, year)).unwrap()
}

// ─────────────────────────────────────────────── 백업

#[test]
fn 수동_백업을_만들면_목록에_나온다() {
    let (_dir, db, year) = fixture("manual");
    student(&db, year, 1, "김하나");

    let f = db.backup(BackupKind::Manual).unwrap();
    assert!(f.name.starts_with("afterschool-backup-"));
    assert!(f.name.ends_with(".db"));
    assert!(f.size > 0, "백업 파일이 비어 있다");
    assert_eq!(f.kind_label, "수동 백업");
    assert!(!f.prunable, "수동 백업은 자동 정리 대상이 아니다");
    assert!(!f.created_at.is_empty(), "파일 이름에서 만든 시각을 읽어야 한다");

    let list = db.backups().unwrap();
    assert!(list.iter().any(|x| x.name == f.name));
}

#[test]
fn 백업_파일은_그_자체로_열린다() {
    // WAL에만 있는 내용이 빠지지 않는지 — 파일 복사가 아니라 백업 API를 쓰는 이유
    let (_dir, db, year) = fixture("selfcontained");
    for i in 1..=5 {
        student(&db, year, i, &format!("학생{i}"));
    }
    let f = db.backup(BackupKind::Manual).unwrap();

    let info = inspect(&PathBuf::from(&f.path)).unwrap();
    assert_eq!(info.student_count, 5, "방금 넣은 자료가 백업에 들어 있어야 한다");
    assert_eq!(info.year_count, 1);
    assert_eq!(info.user_version, migrate::latest_version());
}

#[test]
fn 자동_백업은_정해진_개수만_남는다() {
    let (_dir, db, _year) = fixture("prune");
    for _ in 0..KEEP_AUTO + 4 {
        // 파일 이름이 초 단위라 같은 초에 겹치지 않도록 조금 벌린다
        std::thread::sleep(std::time::Duration::from_millis(2));
        db.backup(BackupKind::Startup).unwrap();
    }
    let autos = db
        .backups()
        .unwrap()
        .into_iter()
        .filter(|f| f.kind_label.contains("앱 시작"))
        .count();
    assert!(autos <= KEEP_AUTO, "자동 백업이 {autos}개 남았다");
}

#[test]
fn 수동_백업은_개수_제한을_받지_않는다() {
    let (_dir, db, _year) = fixture("manual-keep");
    for _ in 0..KEEP_AUTO + 4 {
        std::thread::sleep(std::time::Duration::from_millis(2));
        db.backup(BackupKind::Manual).unwrap();
    }
    let manual = db
        .backups()
        .unwrap()
        .into_iter()
        .filter(|f| f.kind_label == "수동 백업")
        .count();
    assert!(manual > KEEP_AUTO, "수동 백업을 지우면 안 된다 (남은 것 {manual}개)");
}

#[test]
fn 백업을_지울_수_있다() {
    let (_dir, db, _year) = fixture("delete");
    let f = db.backup(BackupKind::Manual).unwrap();
    assert!(PathBuf::from(&f.path).exists());

    db.delete_backup(&f.name).unwrap();
    assert!(!PathBuf::from(&f.path).exists());
    assert!(db.backups().unwrap().iter().all(|x| x.name != f.name));
}

// ─────────────────────────────────────────────── 복원

#[test]
fn 복원하면_백업_시점_상태로_정확히_돌아간다() {
    let (_dir, db, year) = fixture("roundtrip");
    student(&db, year, 1, "김하나");
    student(&db, year, 2, "이두리");
    let f = db.backup(BackupKind::Manual).unwrap();
    assert_eq!(student_count(&db, year), 2);

    // 백업 뒤에 자료를 바꾼다
    student(&db, year, 3, "박세찌");
    db.write(|c| repo::student::delete_all(c, year)).unwrap();
    student(&db, year, 9, "전혀다른학생");
    assert_eq!(student_count(&db, year), 1);

    let report = db.restore(&PathBuf::from(&f.path)).unwrap();
    assert_eq!(report.from_version, migrate::latest_version());
    assert!(!report.migrated, "같은 버전이면 자료구조를 올릴 것이 없다");
    assert!(report.safety_backup.starts_with("before-restore-"));

    // 정확히 두 명, 그때의 이름으로
    assert_eq!(student_count(&db, year), 2);
    let names = db
        .read(|c| repo::student::list(c, year, &Default::default()))
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["김하나", "이두리"]);
}

#[test]
fn 복원_직전에_안전백업이_남는다() {
    let (_dir, db, year) = fixture("safety");
    student(&db, year, 1, "김하나");
    let f = db.backup(BackupKind::Manual).unwrap();
    student(&db, year, 2, "복원하면사라질학생");

    let report = db.restore(&PathBuf::from(&f.path)).unwrap();

    // 안전백업에는 복원 직전 상태(2명)가 들어 있어야 한다
    let safety = db
        .backups()
        .unwrap()
        .into_iter()
        .find(|x| x.name == report.safety_backup)
        .expect("안전백업이 목록에 있어야 한다");
    let info = inspect(&PathBuf::from(&safety.path)).unwrap();
    assert_eq!(info.student_count, 2);
    assert_eq!(student_count(&db, year), 1, "복원은 제대로 되었고");
}

#[test]
fn 복원한_뒤에도_계속_쓸_수_있다() {
    // 연결을 갈아 끼운 뒤에도 읽기·쓰기가 정상인지
    let (_dir, db, year) = fixture("usable");
    student(&db, year, 1, "김하나");
    let f = db.backup(BackupKind::Manual).unwrap();
    db.restore(&PathBuf::from(&f.path)).unwrap();

    student(&db, year, 2, "복원후추가");
    assert_eq!(student_count(&db, year), 2);

    let ws = db
        .write(|c| {
            repo::year::create_workspace(
                c,
                year,
                &WorkspaceInput {
                    name: "4월".into(),
                    start_date: "2026-04-01".into(),
                    end_date: "2026-04-30".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    assert!(ws > 0);
}

// ─────────────────────────────────────────────── 구버전 백업

#[test]
fn 구버전_백업은_자료구조를_올려_복원한다() {
    let (dir, db, year) = fixture("oldver");
    student(&db, year, 1, "지금학생");

    // 001만 적용한 옛 DB를 손으로 만든다
    let old = dir.join("old-v1.db");
    {
        let mut conn = Connection::open(&old).unwrap();
        migrate::run_up_to(&mut conn, 1).unwrap();
        conn.execute(
            "INSERT INTO academic_year (year, name, start_date, end_date, is_current)
             VALUES (2025, '2025학년도', '2025-03-01', '2026-02-28', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO student (year_id, grade, class_no, student_no, name)
             VALUES (1, 3, 1, 1, '옛학생')",
            [],
        )
        .unwrap();
    }

    let info = inspect(&old).unwrap();
    assert_eq!(info.user_version, 1);
    assert!(!info.too_new);
    assert!(info.user_version < migrate::latest_version());

    let report = db.restore(&old).unwrap();
    assert_eq!(report.from_version, 1);
    assert_eq!(report.to_version, migrate::latest_version());
    assert!(report.migrated, "구버전이면 자료구조를 올려야 한다");

    // 옛 자료가 살아 있고, 002·003에서 더한 것도 쓸 수 있다
    let years = db.read(|c| repo::year::list_years(c)).unwrap();
    assert_eq!(years.len(), 1);
    assert_eq!(years[0].year, 2025);
    db.read(|c| {
        let n: i64 = c.query_row("SELECT COUNT(*) FROM settlement_budget", [], |r| r.get(0))?;
        assert_eq!(n, 0);
        let cols: i64 = c.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('change_log') WHERE name = 'student_id'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(cols, 1, "002에서 더한 컬럼이 있어야 한다");
        Ok(())
    })
    .unwrap();
}

#[test]
fn 현재보다_새로운_백업은_막는다() {
    let (dir, db, year) = fixture("toonew");
    student(&db, year, 1, "지켜야할학생");

    // 우리보다 앞선 자료구조를 흉내낸다
    let future = dir.join("future.db");
    {
        let mut conn = Connection::open(&future).unwrap();
        migrate::run_sql_only(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", migrate::latest_version() + 1)
            .unwrap();
    }

    let info = inspect(&future).unwrap();
    assert!(info.too_new);

    let err = db.restore(&future).unwrap_err();
    assert!(
        err.message.contains("현재 프로그램보다 새로운 버전"),
        "{}",
        err.message
    );
    assert!(err.message.contains("업데이트한 후 다시 시도"));
    // 현재 자료는 손대지 않았다
    assert_eq!(student_count(&db, year), 1);
}

// ─────────────────────────────────────────────── 잘못된 파일

#[test]
fn 우리_DB가_아닌_SQLite는_막는다() {
    let (dir, db, year) = fixture("other-sqlite");
    student(&db, year, 1, "지켜야할학생");

    let other = dir.join("other.db");
    {
        let conn = Connection::open(&other).unwrap();
        conn.execute("CREATE TABLE memo(id INTEGER PRIMARY KEY, body TEXT)", [])
            .unwrap();
        conn.execute("INSERT INTO memo (body) VALUES ('남의 자료')", [])
            .unwrap();
    }

    let err = inspect(&other).unwrap_err();
    assert!(err.message.contains("이 프로그램의 백업 파일이 아닙니다"), "{}", err.message);

    let err = db.restore(&other).unwrap_err();
    assert!(err.message.contains("백업 파일이 아닙니다"));
    assert_eq!(student_count(&db, year), 1, "현재 자료는 그대로여야 한다");
}

#[test]
fn SQLite가_아닌_파일은_막는다() {
    let (dir, db, year) = fixture("not-sqlite");
    student(&db, year, 1, "지켜야할학생");

    let junk = dir.join("junk.db");
    std::fs::write(&junk, b"this is not a database at all, just text").unwrap();

    let err = db.restore(&junk).unwrap_err();
    assert!(
        err.message.contains("SQLite 자료 파일이 아닙니다")
            || err.message.contains("손상되어"),
        "{}",
        err.message
    );
    assert_eq!(student_count(&db, year), 1);
}

#[test]
fn 손상된_파일은_막는다() {
    let (dir, db, year) = fixture("corrupt");
    student(&db, year, 1, "지켜야할학생");
    let f = db.backup(BackupKind::Manual).unwrap();

    // 제대로 된 백업의 가운데를 망가뜨린다 (헤더는 남겨 둔다)
    let corrupt = dir.join("corrupt.db");
    let mut bytes = std::fs::read(&f.path).unwrap();
    let len = bytes.len();
    for b in bytes.iter_mut().take(len - 200).skip(len / 2) {
        *b = 0x5a;
    }
    std::fs::write(&corrupt, &bytes).unwrap();

    let err = db.restore(&corrupt).unwrap_err();
    assert!(err.code == "INVALID", "{} / {}", err.code, err.message);
    assert_eq!(student_count(&db, year), 1, "현재 자료는 그대로여야 한다");
}

#[test]
fn 없는_파일은_막는다() {
    let (dir, db, year) = fixture("missing");
    student(&db, year, 1, "지켜야할학생");

    let err = db.restore(&dir.join("없는파일.db")).unwrap_err();
    assert!(err.message.contains("찾지 못했습니다"));
    assert_eq!(student_count(&db, year), 1);
}

// ─────────────────────────────────────────────── 자료 폴더 구조

#[test]
fn 자료_폴더가_용도별로_갈린다() {
    let (dir, db, _year) = fixture("dirs");
    assert_eq!(db.data_dir(), dir);
    assert_eq!(db.backup_dir(), dir.join("backups"));
    assert_eq!(db.export_dir(), dir.join("exports"));
    assert_eq!(db.log_dir(), dir.join("logs"));
    // 백업을 한 번 만들면 폴더가 생긴다
    db.backup(BackupKind::Manual).unwrap();
    assert!(db.backup_dir().is_dir());
}

#[test]
fn 마이그레이션_직전_백업이_남는다() {
    let dir = tmp_dir("migrate-backup");
    let path = dir.join("afterschool.db");

    // 001만 적용한 상태로 만들어 둔다
    {
        let mut conn = Connection::open(&path).unwrap();
        migrate::run_up_to(&mut conn, 1).unwrap();
    }
    // 열면 002·003이 적용되고, 그 직전 백업이 남아야 한다
    let db = Db::open(&path).unwrap();
    let before: Vec<_> = db
        .backups()
        .unwrap()
        .into_iter()
        .filter(|f| f.name.starts_with("before-migrate-"))
        .collect();
    assert_eq!(before.len(), 1, "마이그레이션 전 백업이 하나 있어야 한다");
    assert!(before[0].name.contains("-v1-"), "어느 버전에서 올렸는지 이름에 남는다");
    assert!(!before[0].prunable, "이 백업은 자동으로 지우지 않는다");

    let info = inspect(&PathBuf::from(&before[0].path)).unwrap();
    assert_eq!(info.user_version, 1);
    assert_eq!(
        migrate::current_version(&Connection::open(&path).unwrap()).unwrap(),
        migrate::latest_version()
    );
}

#[test]
fn 같은_초에_백업해도_덮어쓰지_않는다() {
    let (_dir, db, year) = fixture("same-second");
    student(&db, year, 1, "김하나");

    // 사람이 [백업]을 빠르게 두 번 누른 상황
    let a = db.backup(BackupKind::Manual).unwrap();
    let b = db.backup(BackupKind::Manual).unwrap();
    let c = db.backup(BackupKind::Manual).unwrap();

    assert_ne!(a.name, b.name);
    assert_ne!(b.name, c.name);
    for f in [&a, &b, &c] {
        assert!(PathBuf::from(&f.path).exists(), "{} 이(가) 사라졌다", f.name);
        assert!(f.size > 0);
        // 이름이 달라져도 만든 시각은 읽을 수 있어야 한다
        assert!(!f.created_at.is_empty(), "{}", f.name);
    }
    let manual = db
        .backups()
        .unwrap()
        .into_iter()
        .filter(|f| f.kind_label == "수동 백업")
        .count();
    assert_eq!(manual, 3);
}
