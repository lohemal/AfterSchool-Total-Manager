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

// ─────────────────────────────────────────────── Phase 2 — 수강 관리

/// 수강생 명단 한 줄. 금액은 `charge`(실제 청구액)에서 온다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enrollment {
    pub id: i64,
    pub student_id: i64,
    pub department_id: i64,
    // 학생
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    /// 이 작업공간 기간에 유효한 지원제도
    pub programs: Vec<String>,
    // 부서
    pub dept_name: String,
    pub dept_class_name: String,
    /// 화면에 쓰는 부서 표시명 (`로봇과학A반`)
    pub dept_label: String,
    // 금액
    pub fees: Vec<Fee>,
    pub total: i64,
    /// 학생별로 따로 고친 금액이 하나라도 있는가
    pub has_override: bool,
    // 상태
    pub status: String,
    pub change_reason: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentInput {
    pub student_id: i64,
    pub department_id: i64,
    /// 비우면 부서 기준 수강료를 그대로 쓴다
    pub fees: Vec<Fee>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentFilter {
    pub department_id: Option<i64>,
    pub grade: Option<i64>,
    pub class_no: Option<i64>,
    /// `VOUCHER` | `FREE_VOUCHER` | `BOTH` | `NONE`
    pub program: Option<String>,
    /// `ACTIVE` | `CANCELLED` | 비우면 전체
    pub status: Option<String>,
    /// 이름·부서명 통합 검색
    pub query: Option<String>,
}

/// 부서 기준금액과 실제 청구액이 어긋난 한 칸.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeDiff {
    pub enrollment_id: i64,
    pub student_id: i64,
    pub grade: i64,
    pub class_no: i64,
    pub student_no: i64,
    pub name: String,
    pub department_id: i64,
    pub dept_label: String,
    pub item_code: String,
    pub item_name: String,
    /// 지금 이 학생에게 매겨진 금액
    pub current: i64,
    /// 부서에 설정된 기준금액
    pub base: i64,
    /// 학생별로 따로 고쳐 둔 값인가
    pub is_overridden: bool,
}

/// 재반영에서 고른 칸 (SELECTED 방식)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeePick {
    pub enrollment_id: i64,
    pub item_code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    /// 실제로 바뀐 칸 수
    pub changed: i64,
    /// 손대지 않은 학생별 수정 칸 수
    pub kept: i64,
    /// 금액이 바뀐 수강 건수
    pub enrollments: i64,
}

/// 변경이력 한 줄.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLog {
    pub id: i64,
    pub at: String,
    pub workspace_name: String,
    pub kind: String,
    pub kind_label: String,
    pub target: String,
    pub before_value: String,
    pub after_value: String,
    pub reason: String,
}

/// 학생 상세정보 — 조회 전용. **정산 결과를 여기서 계산하지 않는다** (Phase 3).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentDetail {
    pub student: Student,
    /// 지원제도별 자격 (없으면 화면에 '해당없음')
    pub supports: Vec<SupportView>,
    /// 작업공간별 수강 내역 (최신 작업공간이 먼저)
    pub workspaces: Vec<WorkspaceEnrollments>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportView {
    pub program: String,
    pub program_label: String,
    /// 자격 행이 하나도 없으면 false → 화면에 '해당없음'
    pub eligible: bool,
    /// `2026-03-01 ~ 2026-08-31`, 기간이 없으면 `학년도 내내`
    pub periods: Vec<String>,
    /// 대상학년 정책과 어긋나는가
    pub grade_mismatch: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEnrollments {
    pub workspace_id: i64,
    pub workspace_name: String,
    pub start_date: String,
    pub end_date: String,
    pub rows: Vec<Enrollment>,
    /// 이 작업공간에서 수강 중(ACTIVE)인 금액 합계
    pub active_total: i64,
}
