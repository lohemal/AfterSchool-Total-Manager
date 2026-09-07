//! 마이그레이션 러너.
//!
//! `PRAGMA user_version`을 스키마 버전으로 쓰고, 아래 목록을 번호순으로 적용한다.
//! 적용 전에 기존 자료를 `backups/`에 자동 백업한다 — 업데이트 때 자료가
//! 사라지는 일이 없어야 하기 때문이다. 파일 복사가 아니라 SQLite 백업 API를
//! 쓴다(`db::backup::dump_connection`). 이것이야말로 가장 중요한 백업인데,
//! 파일만 복사하면 아직 WAL에만 있는 내용이 빠진다.
//!
//! 새 마이그레이션 추가 방법
//!   1. `migrations/00N_설명.sql` 파일 생성
//!   2. 아래 MIGRATIONS 배열에 한 줄 추가
//! 기존 파일은 절대 수정하지 않는다 (이미 적용된 사용자가 있으므로).

use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
    /// 표를 다시 만드는 마이그레이션인가.
    ///
    /// SQLite 는 컬럼 타입을 못 바꾸므로 `새 표 → 옮기기 → DROP → 이름 바꾸기`로
    /// 갈아 끼워야 한다. 그런데 **외래키를 켠 채 `DROP TABLE` 하면 자식 행이
    /// 함께 지워진다** — student 를 지우면 enrollment 까지 날아간다.
    /// `PRAGMA foreign_keys` 는 트랜잭션 안에서 듣지 않으므로 트랜잭션 바깥에서
    /// 껐다가, 끝난 뒤 `foreign_key_check` 로 확인하고 다시 켠다.
    fk_off: bool,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_init",
        sql: include_str!("../../migrations/001_init.sql"),
        fk_off: false,
    },
    Migration {
        version: 2,
        name: "002_change_log_target",
        sql: include_str!("../../migrations/002_change_log_target.sql"),
        fk_off: false,
    },
    Migration {
        version: 3,
        name: "003_settlement_budget",
        sql: include_str!("../../migrations/003_settlement_budget.sql"),
        fk_off: false,
    },
    Migration {
        version: 4,
        name: "004_class_no_text",
        sql: include_str!("../../migrations/004_class_no_text.sql"),
        fk_off: true,
    },
];

pub fn latest_version() -> i32 {
    MIGRATIONS.iter().map(|m| m.version).max().unwrap_or(0)
}

pub fn current_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

pub fn run(conn: &mut Connection, db_path: &Path) -> AppResult<()> {
    let from = current_version(conn)?;
    let to = latest_version();
    if from >= to {
        return Ok(());
    }

    // 이미 자료가 있는 파일을 손대기 전에 백업을 남긴다.
    // 실패해도 갱신은 진행한다 — 백업을 못 만들었다고 업무를 못 하면 더 손해다.
    if from > 0 && db_path.exists() {
        match backup_before_migrate(conn, db_path, from) {
            Ok(name) => log::info!("마이그레이션 전 백업: {name}"),
            Err(e) => log::warn!("마이그레이션 전 백업 실패: {e}"),
        }
    }

    apply(conn, from)
}

/// 테스트용 — 백업 없이 SQL만 적용한다.
#[cfg(test)]
pub fn run_sql_only(conn: &mut Connection) -> AppResult<()> {
    apply(conn, 0)
}

/// 테스트용 — 특정 버전까지만 적용한다. 구버전 백업 복원을 시험할 때 쓴다.
#[cfg(test)]
pub fn run_up_to(conn: &mut Connection, to: i32) -> AppResult<()> {
    for m in MIGRATIONS.iter().filter(|m| m.version <= to) {
        if m.fk_off {
            apply_one_without_fk(conn, m)?;
        } else {
            apply_one(conn, m)?;
        }
    }
    Ok(())
}

fn apply(conn: &mut Connection, from: i32) -> AppResult<()> {
    for m in MIGRATIONS.iter().filter(|m| m.version > from) {
        if m.fk_off {
            apply_one_without_fk(conn, m)?;
        } else {
            apply_one(conn, m)?;
        }
        log::info!("마이그레이션 적용: {}", m.name);
    }
    Ok(())
}

fn apply_one(conn: &mut Connection, m: &Migration) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(m.sql).map_err(|e| {
        AppError::new("MIGRATE", "자료 구조를 갱신하지 못했습니다.")
            .detail(format!("{} :: {e}", m.name))
    })?;
    if m.fk_off {
        // 커밋하기 **전에** 본다. 커밋한 뒤에 보면 어긋난 것을 찾아도 되돌릴 수 없다.
        fk_check(&tx, m.name)?;
    }
    tx.pragma_update(None, "user_version", m.version)?;
    tx.commit()?;
    Ok(())
}

/// 표를 갈아 끼우는 마이그레이션. 외래키를 끄고 돌린다.
///
/// 외래키를 켠 채 `DROP TABLE student` 를 하면 SQLite 가 먼저 모든 행을 지우고,
/// `ON DELETE CASCADE` 를 타고 수강·지원대상자 자료까지 함께 사라진다.
/// 그래서 잠시 끈다. 대신 **커밋 전에 `foreign_key_check` 로 확인**하므로,
/// 어긋나면 아무것도 적용되지 않고 되돌아간다.
fn apply_one_without_fk(conn: &mut Connection, m: &Migration) -> AppResult<()> {
    // PRAGMA 는 트랜잭션 안에서 듣지 않으므로 바깥에서 끈다.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = apply_one(conn, m);
    // 성공이든 실패든 반드시 되돌린다.
    let restored = conn.pragma_update(None, "foreign_keys", "ON");
    result?;
    restored?;
    Ok(())
}

fn fk_check(conn: &Connection, name: &str) -> AppResult<()> {
    let mut st = conn.prepare("PRAGMA foreign_key_check")?;
    let broken = st.query_map([], |_| Ok(()))?.count();
    if broken > 0 {
        return Err(AppError::new(
            "MIGRATE",
            "자료 구조를 갱신한 뒤 연결이 어긋나 갱신을 되돌렸습니다.",
        )
        .detail(format!("{name} :: foreign_key_check {broken}건")));
    }
    Ok(())
}

/// 마이그레이션 직전 백업. 이미 열려 있는 연결을 그대로 떠 낸다.
fn backup_before_migrate(conn: &Connection, db_path: &Path, from: i32) -> AppResult<String> {
    let dir = db_path
        .parent()
        .ok_or_else(|| AppError::new("IO", "자료 폴더를 찾지 못했습니다."))?
        .join("backups");
    let base = format!(
        "before-migrate-v{from}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let (name, _) = crate::db::backup::dump_connection(conn, &dir, &base)?;
    Ok(name)
}
