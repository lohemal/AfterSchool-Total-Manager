//! 학년도 지원금 정책 명령. (설정 화면은 Phase 3, 읽기는 지금부터 쓴다)

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::model::{Period, Policy};
use crate::repo::policy;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub policy: Policy,
    /// 기간 한도 합계와 연간 한도가 다를 때의 안내 문구 (막지는 않는다)
    pub notice: Option<String>,
}

#[tauri::command]
pub fn policy_list(db: State<'_, Db>, year_id: i64) -> AppResult<Vec<PolicyView>> {
    db.read(|c| {
        let list = policy::list(c, year_id)?;
        Ok(list
            .into_iter()
            .map(|p| PolicyView {
                notice: policy::limit_notice(&p),
                policy: p,
            })
            .collect())
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn policy_save(
    db: State<'_, Db>,
    year_id: i64,
    program: String,
    annual_limit: i64,
    carryover: bool,
    target_grades: String,
    periods: Vec<Period>,
) -> AppResult<()> {
    db.write(|c| {
        policy::save(
            c,
            year_id,
            &program,
            annual_limit,
            carryover,
            &target_grades,
            &periods,
        )
    })
}
