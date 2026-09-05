//! 화면과 주고받는 자료 구조. 필드 이름은 프런트엔드에서 그대로 쓰도록 camelCase.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Year {
    pub id: i64,
    pub year: i64,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub data_version: i64,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: i64,
    pub year_id: i64,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub data_version: i64,
    pub is_current: bool,
    pub note: String,
    /// 이 작업공간의 부서 수 / 수강 수 — 목록에서 한눈에 보기 위한 값
    pub department_count: i64,
    pub enrollment_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInput {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Student {
    pub id: i64,
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    pub note: String,
    /// 이 학생에게 유효한 지원제도 코드 목록 (`VOUCHER`, `FREE_VOUCHER`)
    pub programs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentInput {
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentFilter {
    /// 자격 판정 기준이 되는 작업공간. 없으면 학년도 전체에서 한 번이라도 자격이 있으면 표시
    pub workspace_id: Option<i64>,
    pub grade: Option<i64>,
    pub class_no: Option<i64>,
    pub student_no: Option<i64>,
    pub name: Option<String>,
    /// 지원유형 필터: `VOUCHER` | `FREE_VOUCHER` | `BOTH` | `NONE`
    pub program: Option<String>,
    /// 이름·비고 통합 검색어
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eligibility {
    pub id: i64,
    pub student_id: i64,
    pub program: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub source: String,
    pub note: String,
    // 화면 표시용 학생 정보
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    /// 대상학년 정책과 어긋나는가 (자동으로 고치지 않고 경고만 한다 — 설계안 0-3)
    pub grade_mismatch: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EligibilityInput {
    pub student_id: i64,
    pub program: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostItem {
    pub code: String,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fee {
    pub item_code: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Department {
    pub id: i64,
    pub name: String,
    pub class_name: String,
    pub teacher: String,
    pub days: String,
    pub note: String,
    pub fees: Vec<Fee>,
    pub total: i64,
    /// 이 부서를 수강 중인 학생 수 (ACTIVE)
    pub enrollment_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepartmentInput {
    pub name: String,
    pub class_name: Option<String>,
    pub teacher: Option<String>,
    pub days: Option<String>,
    pub note: Option<String>,
    pub fees: Vec<Fee>,
}

/// 지원정책 (연간한도 · 이월 · 대상학년 + 지원기간 목록)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub id: i64,
    pub program: String,
    pub annual_limit: i64,
    pub carryover: bool,
    pub target_grades: String,
    pub label_fund: String,
    pub label_over: String,
    pub periods: Vec<Period>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub id: Option<i64>,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub limit_amount: i64,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub years: Vec<Year>,
    pub current_year: Option<Year>,
    pub workspaces: Vec<Workspace>,
    pub current_workspace: Option<Workspace>,
    pub cost_items: Vec<CostItem>,
    pub db_path: String,
}
