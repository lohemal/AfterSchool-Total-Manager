//! 앱 시작 정보와 공용 기능.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::Bootstrap;
use crate::repo;

/// 화면이 처음 뜰 때 한 번 부른다. 학년도·작업공간·비용항목을 한꺼번에 준다.
#[tauri::command]
pub fn bootstrap(db: State<'_, Db>) -> AppResult<Bootstrap> {
    db.read(|c| {
        let years = repo::year::list_years(c)?;
        let current_year = repo::year::current_year(c)?;
        let (workspaces, current_workspace) = match &current_year {
            Some(y) => (
                repo::year::list_workspaces(c, y.id)?,
                repo::year::current_workspace(c, y.id)?,
            ),
            None => (Vec::new(), None),
        };
        Ok(Bootstrap {
            years,
            current_year,
            workspaces,
            current_workspace,
            cost_items: repo::cost_items(c)?,
            db_path: db.path().to_string_lossy().to_string(),
        })
    })
}

/// 자료 폴더(또는 그 안의 `exports`)를 탐색기로 연다.
#[tauri::command]
pub fn open_folder(db: State<'_, Db>, which: String) -> AppResult<()> {
    let dir = match which.as_str() {
        "exports" => db.export_dir(),
        "logs" => db.log_dir(),
        "backups" => db.backup_dir(),
        _ => db.data_dir(),
    };
    std::fs::create_dir_all(&dir)?;

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn get_setting(db: State<'_, Db>, key: String) -> AppResult<Option<String>> {
    db.read(|c| repo::setting::get(c, &key))
}

#[tauri::command]
pub fn set_setting(db: State<'_, Db>, key: String, value: String) -> AppResult<()> {
    db.write(|c| repo::setting::set(c, &key, &value))
}
