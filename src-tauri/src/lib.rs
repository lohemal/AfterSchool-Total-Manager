//! 방과후 통합 매니저 — 앱 진입점.
//!
//! 계층은 아래와 같이 나뉜다 (설계안 1장).
//!   `commands`  얇은 IPC 껍데기 (로직 없음)
//!   `repo`      SQL
//!   `domain`    업무 규칙 (DB 의존 없음, 테스트 대상)
//!   `excel`     Excel 읽기/쓰기
//!
//! 업무 자료는 실행 파일과 떨어진 곳(`%APPDATA%\kr.school.afterschool`)에 둔다.
//! 그래야 프로그램을 업데이트해도 자료가 남는다.

pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod excel;
pub mod logging;
pub mod model;
pub mod repo;

use tauri::Manager;

use crate::db::backup::BackupKind;
use crate::db::Db;
use crate::excel::Stage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;

            // 로그를 먼저 붙인다 — 자료 파일을 여는 과정도 기록에 남아야 한다.
            logging::init(&dir.join("logs"));
            log::info!(
                "방과후 통합 매니저 {} 시작 · 자료 폴더 {}",
                env!("CARGO_PKG_VERSION"),
                dir.display()
            );

            let db = Db::open(&dir.join("afterschool.db")).map_err(|e| {
                // 자료 파일을 열지 못하면 더 진행할 수 없다. 이유를 그대로 남긴다.
                log::error!("자료 파일 열기 실패: {} ({:?})", e.message, e.detail);
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("{} ({})", e.message, e.detail.unwrap_or_default()),
                )
            })?;

            // 앱을 켤 때 자동 백업을 남긴다. 실패해도 앱은 계속 뜬다 —
            // 백업을 못 만든 것 때문에 업무를 못 하면 더 손해다.
            match db.backup(BackupKind::Startup) {
                Ok(f) => log::info!("시작 자동백업: {}", f.name),
                Err(e) => log::warn!("시작 자동백업 실패(계속 진행): {}", e.message),
            }

            app.manage(db);
            app.manage(Stage::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::bootstrap,
            commands::app::open_folder,
            commands::app::get_setting,
            commands::app::set_setting,
            commands::system::app_info,
            commands::system::backup_create,
            commands::system::backup_list,
            commands::system::backup_delete,
            commands::system::backup_inspect,
            commands::system::backup_restore,
            commands::system::backup_restore_file,
            commands::year::year_list,
            commands::year::year_create,
            commands::year::year_update,
            commands::year::year_set_current,
            commands::year::year_delete,
            commands::year::workspace_list,
            commands::year::workspace_create,
            commands::year::workspace_update,
            commands::year::workspace_delete,
            commands::year::workspace_set_current,
            commands::year::workspace_overlaps,
            commands::student::student_list,
            commands::student::student_create,
            commands::student::student_update,
            commands::student::student_delete,
            commands::student::student_delete_all,
            commands::eligibility::eligibility_list,
            commands::eligibility::eligibility_create,
            commands::eligibility::eligibility_update,
            commands::eligibility::eligibility_delete,
            commands::eligibility::eligibility_delete_all,
            commands::department::department_list,
            commands::department::department_create,
            commands::department::department_update,
            commands::department::department_delete,
            commands::department::department_delete_all,
            commands::enrollment::enrollment_list,
            commands::enrollment::enrollment_get,
            commands::enrollment::enrollment_by_department,
            commands::enrollment::department_base_fees,
            commands::enrollment::enrollment_create,
            commands::enrollment::enrollment_update_fees,
            commands::enrollment::enrollment_cancel,
            commands::enrollment::enrollment_restore,
            commands::enrollment::enrollment_save_student_fees,
            commands::enrollment::enrollment_fee_diff,
            commands::enrollment::enrollment_apply_fees,
            commands::enrollment::change_log_list,
            commands::enrollment::student_detail,
            commands::enrollment::enrollment_delete_all,
            commands::enrollment::fee_report,
            commands::policy::policy_list,
            commands::policy::policy_save,
            commands::settle::settlement_status,
            commands::settle::settlement_validate,
            commands::settle::settlement_generate,
            commands::settle::settlement_summary,
            commands::settle::settlement_self_pay,
            commands::settle::settlement_program,
            commands::settle::settlement_student_supports,
            commands::settle::priority_dept_list,
            commands::settle::priority_dept_save,
            commands::settle::priority_item_list,
            commands::settle::priority_item_save,
            commands::settle::grant_list,
            commands::settle::grant_save,
            commands::settle::grant_delete,
            commands::settle::proposal_kinds,
            commands::settle::proposal_preview,
            commands::settle::proposal_export,
            commands::settle::settlement_student_allocs,
            commands::settle::settlement_export,
            commands::excel::excel_template,
            commands::excel::excel_preview,
            commands::excel::excel_commit,
            commands::excel::excel_export,
            commands::excel::excel_export_issues,
            commands::excel::fee_report_export,
        ])
        .run(tauri::generate_context!())
        .expect("앱을 시작하지 못했습니다");
}

#[cfg(test)]
#[path = "version_tests.rs"]
mod version_tests;
