//! 백업 · 복원 · 앱 정보 명령 (Phase 5).
//!
//! 업데이트는 Tauri updater 플러그인이 화면 쪽에서 직접 다룬다 — 여기서는
//! 저장소 주소와 현재 버전만 알려 준다. 저장소 주소는 앱 설정에 들어 있고
//! 사용자가 입력하지 않는다 (요구사항 §8).

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::db::backup::{BackupFile, BackupInfo, BackupKind, RestoreReport};
use crate::db::{migrate, Db};
use crate::error::AppResult;

/// 앱과 자료가 어디에 있는지. 화면 상단과 문제 제보에 쓴다.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// 자료구조 버전 — 백업 호환 판단에 쓴다
    pub schema_version: i32,
    pub db_path: String,
    pub data_dir: String,
    pub backup_dir: String,
    pub export_dir: String,
    pub log_dir: String,
    /// 앱 안에 박혀 있는 Release 저장소 주소
    pub release_url: String,
    pub db_size: i64,
}

/// Release 저장소. 화면에 URL 자체를 노출하지 않고 [Release 페이지 열기]로만 쓴다.
pub const RELEASE_URL: &str = "https://github.com/lohemal/AfterSchool-Total-Manager/releases";

#[tauri::command]
pub fn app_info(db: State<'_, Db>) -> AppResult<AppInfo> {
    let path = db.path().to_path_buf();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: migrate::latest_version(),
        db_path: path.to_string_lossy().to_string(),
        data_dir: db.data_dir().to_string_lossy().to_string(),
        backup_dir: db.backup_dir().to_string_lossy().to_string(),
        export_dir: db.export_dir().to_string_lossy().to_string(),
        log_dir: db.log_dir().to_string_lossy().to_string(),
        release_url: RELEASE_URL.to_string(),
        db_size: std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0),
    })
}

// ─────────────────────────────────────────────── 백업

#[tauri::command]
pub fn backup_create(db: State<'_, Db>) -> AppResult<BackupFile> {
    db.backup(BackupKind::Manual)
}

#[tauri::command]
pub fn backup_list(db: State<'_, Db>) -> AppResult<Vec<BackupFile>> {
    db.backups()
}

#[tauri::command]
pub fn backup_delete(db: State<'_, Db>, name: String) -> AppResult<()> {
    db.delete_backup(&name)
}

/// 고른 파일이 복원해도 되는 것인지 미리 살펴본다. **복원하지 않는다.**
#[tauri::command]
pub fn backup_inspect(db: State<'_, Db>, path: String) -> AppResult<BackupInfo> {
    db.inspect_backup(&PathBuf::from(path))
}

/// 백업 폴더 안의 파일로 복원한다.
#[tauri::command]
pub fn backup_restore(db: State<'_, Db>, name: String) -> AppResult<RestoreReport> {
    let path = crate::db::backup::resolve_in_backups(&db.backup_dir(), &name)?;
    db.restore(&path)
}

/// 아무 위치의 파일로 복원한다 (다른 PC에서 가져온 백업).
#[tauri::command]
pub fn backup_restore_file(db: State<'_, Db>, path: String) -> AppResult<RestoreReport> {
    db.restore(&PathBuf::from(path))
}

// ─────────────────────────────────────────────── 위험한 일괄 삭제

/// 대량 삭제 직전에 자동 백업을 남긴다 (요구사항 §7).
///
/// 삭제 자체는 각 화면의 명령이 하고, 이 함수는 그 앞에서 불린다.
pub fn guard_bulk_delete(db: &Db, what: &str) -> AppResult<String> {
    let f = db.backup(BackupKind::BeforeDelete)?;
    log::info!("일괄 삭제 직전 백업: {} ({what})", f.name);
    Ok(f.name)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkDeleteResult {
    pub deleted: i64,
    /// 되돌릴 수 있도록 만들어 둔 백업 파일 이름
    pub backup: String,
}
