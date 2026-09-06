//! 백업 · 복원 (요구사항 §3~§6).
//!
//! ## 파일 복사를 쓰지 않는 이유
//!
//! SQLite는 WAL 방식으로 돌아간다. `.db` 파일만 복사하면 아직 `.db-wal`에만
//! 있는 내용이 빠진다. 그래서 **SQLite 백업 API**를 쓴다 — 쓰는 도중에도
//! 안전하고, 나온 파일은 그 자체로 완결된다.
//!
//! ## 복원이 실패해도 자료를 잃지 않는 순서
//!
//! ```text
//! 1. 고른 파일 검사 (SQLite인가 · 우리 DB인가 · 버전이 미래가 아닌가)
//! 2. 지금 자료를 안전백업으로 남긴다
//! 3. 연결을 닫고 파일을 갈아 끼운다
//! 4. 열면서 마이그레이션 적용
//! ```
//!
//! 3·4에서 어긋나면 2의 안전백업으로 되돌린다. 검사에서 걸리면 3을 아예
//! 시작하지 않으므로 현재 자료는 손대지 않는다.

use std::path::{Path, PathBuf};

use rusqlite::backup::Backup;
use rusqlite::Connection;

use crate::db::{migrate, setup_conn, Db};
use crate::error::{AppError, AppResult};

/// 백업이 만들어진 까닭. 파일 이름 앞머리로 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    /// 사람이 [백업]을 눌러 만든 것 — **자동으로 지우지 않는다**
    Manual,
    /// 앱을 켤 때 만든 것 — 보존 개수를 넘으면 오래된 것부터 지운다
    Startup,
    /// 복원 직전 안전백업
    BeforeRestore,
    /// 위험한 일괄 삭제 직전 안전백업
    BeforeDelete,
    /// 마이그레이션 직전 (`migrate.rs`가 만든다)
    BeforeMigrate,
}

impl BackupKind {
    pub fn prefix(self) -> &'static str {
        match self {
            BackupKind::Manual => "afterschool-backup",
            BackupKind::Startup => "auto-startup",
            BackupKind::BeforeRestore => "before-restore",
            BackupKind::BeforeDelete => "before-delete",
            BackupKind::BeforeMigrate => "before-migrate",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BackupKind::Manual => "수동 백업",
            BackupKind::Startup => "자동 백업 (앱 시작)",
            BackupKind::BeforeRestore => "복원 직전 안전백업",
            BackupKind::BeforeDelete => "삭제 직전 안전백업",
            BackupKind::BeforeMigrate => "자료구조 갱신 직전 안전백업",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        // 긴 접두어를 먼저 본다 (`afterschool-backup`이 `auto-`와 겹치지 않도록)
        for k in [
            BackupKind::Manual,
            BackupKind::Startup,
            BackupKind::BeforeRestore,
            BackupKind::BeforeDelete,
            BackupKind::BeforeMigrate,
        ] {
            if name.starts_with(k.prefix()) {
                return Some(k);
            }
        }
        None
    }

    /// 이 종류는 개수를 지켜 오래된 것을 지우는가.
    pub fn prunable(self) -> bool {
        matches!(self, BackupKind::Startup | BackupKind::BeforeDelete)
    }
}

/// 자동 백업 보존 개수 — 종류마다 이 개수만 남긴다.
pub const KEEP_AUTO: usize = 10;

/// 백업 파일 하나.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub name: String,
    pub path: String,
    /// `수동 백업` 등 사람이 읽는 종류
    pub kind: String,
    pub kind_label: String,
    /// 파일 이름에서 읽은 만든 시각 (`2026-09-06 17:30:00`)
    pub created_at: String,
    pub size: i64,
    /// 자동 정리 대상인가
    pub prunable: bool,
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// 겹치지 않는 백업 파일 이름을 고른다.
///
/// 이름이 초 단위라 같은 초에 두 번 만들면 덮어써진다. 사람이 [백업]을 두 번
/// 빠르게 누를 수도 있고, 삭제 직전 백업과 복원 직전 백업이 같은 초에 겹칠
/// 수도 있다. **백업을 덮어쓰는 것은 백업이 없는 것보다 나쁘다.**
fn unique_from(dir: &Path, base: &str) -> (String, PathBuf) {
    let mut name = format!("{base}.db");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}.db");
        n += 1;
    }
    (name.clone(), dir.join(name))
}

fn unique_path(dir: &Path, kind: BackupKind) -> (String, PathBuf) {
    unique_from(dir, &format!("{}-{}", kind.prefix(), stamp()))
}

/// 연결 하나를 그대로 파일로 떠 낸다.
///
/// `Db`가 없는 곳(마이그레이션 도중)에서도 **파일 복사가 아니라 백업 API**를
/// 쓰기 위한 함수다. 파일만 복사하면 아직 WAL에만 있는 내용이 빠진다.
pub(crate) fn dump_connection(
    conn: &Connection,
    dir: &Path,
    base: &str,
) -> AppResult<(String, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let (name, path) = unique_from(dir, base);
    let mut dest = Connection::open(&path).map_err(|e| {
        AppError::new("BACKUP", "백업 파일을 만들지 못했습니다.").detail(e.to_string())
    })?;
    let result = (|| {
        let b = Backup::new(conn, &mut dest).map_err(|e| {
            AppError::new("BACKUP", "백업을 시작하지 못했습니다.").detail(e.to_string())
        })?;
        b.run_to_completion(64, std::time::Duration::from_millis(0), None)
            .map_err(|e| AppError::new("BACKUP", "백업을 끝내지 못했습니다.").detail(e.to_string()))
    })();
    drop(dest);
    if let Err(e) = result {
        // 반쯤 만들어진 파일은 남기지 않는다. 있으면 온전한 백업으로 오해한다.
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok((name, path))
}

/// `20260906-173000` → `2026-09-06 17:30:00`
fn read_stamp(name: &str) -> String {
    let digits: String = name
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    for part in digits.split('-').collect::<Vec<_>>().windows(2) {
        if part[0].len() == 8 && part[1].len() == 6 {
            let (d, t) = (part[0], part[1]);
            return format!(
                "{}-{}-{} {}:{}:{}",
                &d[0..4],
                &d[4..6],
                &d[6..8],
                &t[0..2],
                &t[2..4],
                &t[4..6]
            );
        }
    }
    String::new()
}

impl Db {
    /// 백업을 만든다. SQLite 백업 API를 쓰므로 쓰는 도중에도 안전하다.
    pub fn backup(&self, kind: BackupKind) -> AppResult<BackupFile> {
        let dir = self.backup_dir();
        std::fs::create_dir_all(&dir)?;
        let (name, path) = unique_path(&dir, kind);

        self.read(|src| {
            let mut dest = Connection::open(&path).map_err(|e| {
                AppError::new("BACKUP", "백업 파일을 만들지 못했습니다.").detail(e.to_string())
            })?;
            let b = Backup::new(src, &mut dest).map_err(|e| {
                AppError::new("BACKUP", "백업을 시작하지 못했습니다.").detail(e.to_string())
            })?;
            b.run_to_completion(64, std::time::Duration::from_millis(0), None)
                .map_err(|e| {
                    AppError::new("BACKUP", "백업을 끝내지 못했습니다.").detail(e.to_string())
                })?;
            Ok(())
        })
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&path);
        })?;

        if kind.prunable() {
            prune(&dir, kind);
        }
        log::info!("백업 생성: {name} ({})", kind.label());
        Ok(describe(&path)?)
    }

    /// 백업 파일 목록. 최근 것이 먼저.
    pub fn backups(&self) -> AppResult<Vec<BackupFile>> {
        let dir = self.backup_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir)?.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("db") {
                continue;
            }
            if let Ok(f) = describe(&path) {
                out.push(f);
            }
        }
        out.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(out)
    }

    /// 백업 파일 하나를 지운다. 자료 파일 자체를 지우는 실수를 막으려고
    /// **백업 폴더 안의 파일만** 받는다.
    pub fn delete_backup(&self, name: &str) -> AppResult<()> {
        let path = resolve_in_backups(&self.backup_dir(), name)?;
        std::fs::remove_file(&path)?;
        log::info!("백업 삭제: {name}");
        Ok(())
    }

    /// 백업 파일을 열어 우리 DB인지 살펴본다. **복원하지는 않는다.**
    pub fn inspect_backup(&self, path: &Path) -> AppResult<BackupInfo> {
        inspect(path)
    }

    /// 복원. 실패하면 지금 자료를 그대로 둔다 (모듈 머리말의 순서를 지킨다).
    pub fn restore(&self, src: &Path) -> AppResult<RestoreReport> {
        // 1) 검사 — 여기서 걸리면 현재 자료를 건드리지 않는다
        let info = inspect(src)?;
        if info.user_version > migrate::latest_version() {
            return Err(AppError::invalid(format!(
                "이 백업파일은 현재 프로그램보다 새로운 버전에서 생성되었습니다. \
                 프로그램을 업데이트한 후 다시 시도해주세요. \
                 (백업 자료구조 {} / 현재 프로그램 {})",
                info.user_version,
                migrate::latest_version()
            )));
        }

        // 2) 지금 자료를 안전백업으로 남긴다
        let safety = self.backup(BackupKind::BeforeRestore)?;
        let safety_path = PathBuf::from(&safety.path);

        // 3~4) 연결을 닫고 갈아 끼운 뒤 마이그레이션까지
        let result = self.swap_in(src);
        match result {
            Ok(applied) => {
                log::info!(
                    "복원 완료: {} (자료구조 {} → {})",
                    src.file_name().unwrap_or_default().to_string_lossy(),
                    info.user_version,
                    migrate::latest_version()
                );
                Ok(RestoreReport {
                    from_version: info.user_version,
                    to_version: migrate::latest_version(),
                    migrated: applied,
                    safety_backup: safety.name,
                })
            }
            Err(e) => {
                log::error!("복원 실패 — 안전백업으로 되돌립니다: {}", e.message);
                // 되돌리기. 이것마저 실패하면 사람에게 파일 위치를 알려 준다.
                if let Err(back) = self.swap_in(&safety_path) {
                    return Err(AppError::new(
                        "RESTORE_FATAL",
                        format!(
                            "복원에 실패했고 되돌리기도 실패했습니다. \
                             백업 폴더의 {} 파일을 afterschool.db 로 직접 바꿔 주세요.",
                            safety.name
                        ),
                    )
                    .detail(format!("{} / {}", e.message, back.message)));
                }
                Err(e)
            }
        }
    }

    /// 연결을 닫고 `src`의 내용으로 자료 파일을 새로 만든 뒤 다시 연다.
    fn swap_in(&self, src: &Path) -> AppResult<bool> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| AppError::new("DB_LOCK", "자료 접근이 잠겨 있습니다."))?;

        // 잠시 메모리 연결로 바꿔 두면 기존 연결이 닫히고 파일이 풀린다.
        *guard = Connection::open_in_memory().map_err(|e| {
            AppError::new("RESTORE", "복원을 준비하지 못했습니다.").detail(e.to_string())
        })?;

        let path = self.path();
        let _ = std::fs::remove_file(path);
        remove_sidecars(path);

        let mut dest = Connection::open(path).map_err(|e| {
            AppError::new("RESTORE", "자료 파일을 새로 만들지 못했습니다.").detail(e.to_string())
        })?;
        {
            let source = Connection::open_with_flags(
                src,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| {
                AppError::new("RESTORE", "백업 파일을 열지 못했습니다.").detail(e.to_string())
            })?;
            let b = Backup::new(&source, &mut dest).map_err(|e| {
                AppError::new("RESTORE", "복원을 시작하지 못했습니다.").detail(e.to_string())
            })?;
            b.run_to_completion(64, std::time::Duration::from_millis(0), None)
                .map_err(|e| {
                    AppError::new("RESTORE", "복원을 끝내지 못했습니다.").detail(e.to_string())
                })?;
        }

        setup_conn(&dest)?;
        let before = migrate::current_version(&dest)?;
        migrate::run(&mut dest, path)?;
        let after = migrate::current_version(&dest)?;

        *guard = dest;
        Ok(after > before)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub from_version: i32,
    pub to_version: i32,
    /// 자료구조를 올렸는가 (구버전 백업을 복원한 경우)
    pub migrated: bool,
    pub safety_backup: String,
}

/// 백업 파일을 살펴본 결과.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub user_version: i32,
    pub app_version: i32,
    /// 열어 본 표 가운데 우리 DB의 핵심 표가 몇 개 있었는가
    pub tables: i64,
    pub too_new: bool,
    pub year_count: i64,
    pub student_count: i64,
    pub enrollment_count: i64,
    pub settlement_count: i64,
}

/// 복원해도 되는 파일인지 본다 (요구사항 §5).
///
/// 확장자만 보지 않는다 — 실제로 열어서 SQLite인지, 우리 프로그램의 표가
/// 있는지, 자료구조 버전이 미래가 아닌지 확인한다.
pub fn inspect(path: &Path) -> AppResult<BackupInfo> {
    if !path.exists() {
        return Err(AppError::invalid("선택한 파일을 찾지 못했습니다."));
    }

    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| {
            AppError::invalid("SQLite 자료 파일이 아닙니다. 이 프로그램에서 만든 백업 파일을 골라 주세요.")
                .detail(e.to_string())
        })?;

    // 진짜 SQLite인지 — 헤더만 맞고 내용이 깨진 파일을 걸러낸다
    let ok: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| {
            AppError::invalid(
                "파일이 손상되어 읽을 수 없습니다. 다른 백업 파일을 골라 주세요.",
            )
            .detail(e.to_string())
        })?;
    if ok != "ok" {
        return Err(AppError::invalid(
            "파일이 손상되어 읽을 수 없습니다. 다른 백업 파일을 골라 주세요.",
        )
        .detail(ok));
    }

    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);

    // 우리 프로그램의 DB인지 — 핵심 표가 다 있어야 한다
    const CORE: [&str; 6] = [
        "academic_year",
        "workspace",
        "student",
        "department",
        "enrollment",
        "charge",
    ];
    let mut found = 0i64;
    for t in CORE {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [t],
                |r| r.get(0),
            )
            .unwrap_or(0);
        found += n;
    }
    if found < CORE.len() as i64 {
        return Err(AppError::invalid(
            "이 프로그램의 백업 파일이 아닙니다. 방과후 통합 매니저에서 만든 백업을 골라 주세요.",
        )
        .detail(format!("핵심 표 {found}/{}", CORE.len())));
    }
    if user_version == 0 {
        return Err(AppError::invalid(
            "자료구조 버전을 읽을 수 없는 파일입니다. 이 프로그램에서 만든 백업 파일을 골라 주세요.",
        ));
    }

    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };

    Ok(BackupInfo {
        user_version,
        app_version: migrate::latest_version(),
        tables: found,
        too_new: user_version > migrate::latest_version(),
        year_count: count("SELECT COUNT(*) FROM academic_year"),
        student_count: count("SELECT COUNT(*) FROM student"),
        enrollment_count: count("SELECT COUNT(*) FROM enrollment"),
        // settlement 표는 001에 있으므로 버전을 가리지 않고 세도 된다
        settlement_count: count("SELECT COUNT(*) FROM settlement"),
    })
}

fn describe(path: &Path) -> AppResult<BackupFile> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let kind = BackupKind::from_name(&name).unwrap_or(BackupKind::Manual);
    let size = std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);
    Ok(BackupFile {
        created_at: read_stamp(&name),
        kind: format!("{kind:?}"),
        kind_label: kind.label().to_string(),
        prunable: kind.prunable(),
        name,
        path: path.to_string_lossy().to_string(),
        size,
    })
}

/// 그 종류의 백업을 최신 [`KEEP_AUTO`]개만 남긴다.
/// **수동 백업과 마이그레이션·복원 안전백업은 손대지 않는다.**
fn prune(dir: &Path, kind: BackupKind) {
    if !kind.prunable() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut mine: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(kind.prefix()) && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();
    if mine.len() <= KEEP_AUTO {
        return;
    }
    mine.sort();
    for old in &mine[..mine.len() - KEEP_AUTO] {
        if std::fs::remove_file(old).is_ok() {
            log::info!(
                "오래된 자동백업 삭제: {}",
                old.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }
}

/// 백업 폴더 안의 파일 이름만 받는다 — 경로를 거슬러 올라가는 것을 막는다.
pub fn resolve_in_backups(dir: &Path, name: &str) -> AppResult<PathBuf> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::invalid("백업 파일 이름이 올바르지 않습니다."));
    }
    let path = dir.join(name);
    if !path.exists() {
        return Err(AppError::not_found("백업 파일을 찾지 못했습니다."));
    }
    Ok(path)
}

pub fn remove_sidecars(db_path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{}", db_path.display(), suffix));
        let _ = std::fs::remove_file(side);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 파일_이름에서_종류를_읽는다() {
        assert_eq!(
            BackupKind::from_name("afterschool-backup-20260906-173000.db"),
            Some(BackupKind::Manual)
        );
        assert_eq!(
            BackupKind::from_name("auto-startup-20260906-173000.db"),
            Some(BackupKind::Startup)
        );
        assert_eq!(
            BackupKind::from_name("before-restore-20260906-173000.db"),
            Some(BackupKind::BeforeRestore)
        );
        assert_eq!(BackupKind::from_name("something-else.db"), None);
    }

    #[test]
    fn 수동_백업은_자동으로_지우지_않는다() {
        assert!(!BackupKind::Manual.prunable());
        assert!(!BackupKind::BeforeMigrate.prunable());
        assert!(!BackupKind::BeforeRestore.prunable());
        assert!(BackupKind::Startup.prunable());
    }

    #[test]
    fn 파일_이름에서_시각을_읽는다() {
        assert_eq!(
            read_stamp("afterschool-backup-20260906-173000.db"),
            "2026-09-06 17:30:00"
        );
        assert_eq!(read_stamp("before-migrate-v2-20260906-084500.db"), "2026-09-06 08:45:00");
        assert_eq!(read_stamp("이상한이름.db"), "");
    }

    #[test]
    fn 백업_폴더_밖의_경로는_받지_않는다() {
        let dir = std::env::temp_dir();
        assert!(resolve_in_backups(&dir, "../secret.db").is_err());
        assert!(resolve_in_backups(&dir, "sub/other.db").is_err());
    }
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod file_tests;
