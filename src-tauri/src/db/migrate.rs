//! 마이그레이션 러너.
//!
//! `PRAGMA user_version`을 스키마 버전으로 쓰고, 아래 목록을 번호순으로 적용한다.
//! 적용 전에 기존 DB 파일을 `backups/`에 자동 복사한다 — 업데이트 때 자료가
//! 사라지는 일이 없어야 하기 때문이다.
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
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_init",
        sql: include_str!("../../migrations/001_init.sql"),
    },
    Migration {
        version: 2,
        name: "002_change_log_target",
        sql: include_str!("../../migrations/002_change_log_target.sql"),
    },
    Migration {
        version: 3,
        name: "003_settlement_budget",
        sql: include_str!("../../migrations/003_settlement_budget.sql"),
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

    // 이미 자료가 있는 파일을 손대기 전에 복사본을 남긴다.
    if from > 0 && db_path.exists() {
        if let Err(e) = backup_before_migrate(db_path, from) {
            log::warn!("마이그레이션 전 백업 실패: {e}");
        }
    }

    apply(conn, from)
}

/// 테스트용 — 백업 없이 SQL만 적용한다.
#[cfg(test)]
pub fn run_sql_only(conn: &mut Connection) -> AppResult<()> {
    apply(conn, 0)
}

fn apply(conn: &mut Connection, from: i32) -> AppResult<()> {
    for m in MIGRATIONS.iter().filter(|m| m.version > from) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql).map_err(|e| {
            AppError::new("MIGRATE", "자료 구조를 갱신하지 못했습니다.")
                .detail(format!("{} :: {e}", m.name))
        })?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
        log::info!("마이그레이션 적용: {}", m.name);
    }
    Ok(())
}

fn backup_before_migrate(db_path: &Path, from: i32) -> AppResult<()> {
    let dir = db_path
        .parent()
        .ok_or_else(|| AppError::new("IO", "자료 폴더를 찾지 못했습니다."))?
        .join("backups");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("before-migrate-v{from}-{stamp}.db"));
    std::fs::copy(db_path, dest)?;
    Ok(())
}
