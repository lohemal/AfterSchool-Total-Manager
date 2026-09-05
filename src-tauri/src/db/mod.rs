//! SQLite 연결 관리.
//!
//! 1인 사용 데스크톱 앱이므로 커넥션 풀 대신 `Mutex<Connection>` 하나로 충분하다.
//! 모든 DB 접근은 `Db::read` / `Db::write`를 통해서만 이루어진다.

pub mod migrate;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Db {
    /// DB 파일을 열고 필요한 마이그레이션을 적용한다.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let mut conn = Connection::open(path).map_err(|e| {
            AppError::new("DB_OPEN", "자료 파일을 열지 못했습니다. 프로그램을 다시 시작해 주세요.")
                .detail(format!("{} :: {e}", path.display()))
        })?;

        setup_conn(&conn)?;
        migrate::run(&mut conn, path)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// 테스트용 메모리 DB.
    #[cfg(test)]
    pub fn open_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        setup_conn(&conn)?;
        migrate::run_sql_only(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn data_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir().join("backups")
    }

    pub fn export_dir(&self) -> PathBuf {
        self.data_dir().join("exports")
    }

    /// 읽기 전용 작업.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| AppError::new("DB_LOCK", "자료 접근이 잠겨 있습니다."))?;
        f(&guard)
    }

    /// 쓰기 작업. 트랜잭션으로 감싸고, 오류가 나면 통째로 되돌린다.
    pub fn write<T>(&self, f: impl FnOnce(&rusqlite::Transaction) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| AppError::new("DB_LOCK", "자료 접근이 잠겨 있습니다."))?;
        let tx = guard.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

fn setup_conn(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}
