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
    pub class_no: String,
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
    pub class_no: String,
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
    pub class_no: Option<String>,
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
    pub class_no: String,
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
    pub class_no: String,
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
    pub class_no: Option<String>,
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
    pub class_no: String,
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

// ─────────────────────────────────────────────── Phase 3 — 정산

/// 정산 생성 전 검사 결과 한 줄.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// `ERROR`면 정산을 진행할 수 없다. `WARN`이면 확인 후 진행할 수 있다.
    pub level: String,
    pub code: String,
    pub message: String,
}

/// 저장된 정산이 지금 자료와 견주어 최신인가 (설계안 8장).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementStatus {
    /// `NONE` | `FRESH` | `STALE_DATA` | `STALE_YEAR` | `STALE_PRIOR`
    pub state: String,
    pub message: String,
    pub settlement_id: Option<i64>,
    pub created_at: Option<String>,
    pub program_order: String,
    /// 낡음의 원인이 선행 작업공간이라면 그 이름
    pub prior_name: Option<String>,
    /// 그 선행 작업공간이 이전 지원기간인가
    pub prior_is_earlier_period: bool,
}

impl SettlementStatus {
    pub fn is_fresh(&self) -> bool {
        self.state == "FRESH"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub settlement_id: i64,
    pub created_at: String,
    pub students: i64,
    pub allocs: i64,
    /// 배분한 총액 (= 원본 charge 총액)
    pub total: i64,
    pub warnings: Vec<Issue>,
}

/// 항목 × 재원 요약 한 줄 (설계안 9-1).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRow {
    pub item_code: String,
    pub item_name: String,
    pub self_pay: i64,
    pub voucher: i64,
    pub voucher_over: i64,
    pub free_voucher: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub rows: Vec<SummaryRow>,
    pub total: SummaryRow,
    /// 원본 charge 합계 — 아래 `balanced`가 false면 화면에 붉게 띄운다
    pub charge_total: i64,
    pub balanced: bool,
    pub created_at: String,
}

/// 정산 시점의 학생별 지원금 상태 (`settlement_budget`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetView {
    pub program: String,
    pub annual_limit: i64,
    pub period_name: String,
    pub period_limit: i64,
    pub carryover: bool,
    pub carry_in: i64,
    pub used_prior_periods: i64,
    pub used_in_period_before: i64,
    pub used_all_before: i64,
    pub capped_by_annual: bool,
    /// 이번 작업공간에서 쓸 수 있었던 금액
    pub available: i64,
    /// 이번 작업공간에서 실제로 쓴 금액
    pub used_now: i64,
    /// available − used_now
    pub period_left: i64,
    /// used_all_before + used_now
    pub annual_used: i64,
    /// annual_limit − annual_used
    pub annual_left: i64,
}

/// 수익자 탭 한 줄 — 학생 × 부서 (요구사항 §16).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfPayRow {
    pub student_id: i64,
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    pub department_id: i64,
    pub dept_label: String,
    /// 항목별 학부모 부담액 (SELF_PAY + VOUCHER_OVER)
    pub fees: Vec<Fee>,
    pub total: i64,
    /// 그중 일반 수익자 부담금
    pub self_pay: i64,
    /// 그중 이용권 초과금
    pub voucher_over: i64,
    // 사연별 — 화면에서 구분해 보여 준다
    pub origin_plain: i64,
    pub origin_voucher: i64,
    pub origin_free: i64,
}

/// 이용권 · 자유수강권 탭 한 줄 — 학생 하나 (요구사항 §17·§18).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRow {
    pub student_id: i64,
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    /// 지원받은 금액 (항목별)
    pub used: Vec<Fee>,
    pub used_total: i64,
    /// 초과금 (이용권만 해당. 자유수강권은 비어 있고 수익자 탭으로 간다)
    pub over: Vec<Fee>,
    pub over_total: i64,
    pub budget: Option<BudgetView>,
}

/// 학생 상세정보의 지원제도 칸 (요구사항 §19).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportState {
    pub program: String,
    pub program_label: String,
    /// `NONE`(해당없음) | `BEFORE`(정산 전) | `STALE`(재정산 필요) | `OK`
    pub state: String,
    pub budget: Option<BudgetView>,
}

/// 차감 우선순위 편집용 한 줄.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityRow {
    pub key: String,
    pub label: String,
    pub sort_order: i64,
    /// 부서 우선순위에서만 쓴다 — 이용권 대상자가 실제 수강 중인 인원
    pub voucher_students: i64,
}

/// 학생별 예외 한도 (`support_grant`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub id: i64,
    pub student_id: i64,
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    pub program: String,
    /// null이면 연간 한도 예외, 값이 있으면 그 지원기간의 한도 예외
    pub period_id: Option<i64>,
    pub period_name: String,
    pub amount: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantInput {
    pub student_id: i64,
    pub program: String,
    pub period_id: Option<i64>,
    pub amount: i64,
    pub reason: Option<String>,
}

// ─────────────────────────────────────────────── Phase 4 — 행정자료

/// 품의 한 열. `fund`는 안정된 코드이고 `label`은 그 학년도의 표시 이름이다.
///
/// `3학년 지원금` 같은 문구는 여기서만 만들어진다 — DB와 정산 엔진에는 학년
/// 개념이 들어가지 않는다 (설계안 3장).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalColumn {
    pub fund: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalRow {
    pub department_id: i64,
    pub dept_label: String,
    /// `columns` 차례대로의 금액
    pub amounts: Vec<i64>,
    pub total: i64,
}

/// 품의 집계 결과. **Excel writer는 이것을 받아 쓰기만 한다** (요구사항 §14).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub year_name: String,
    pub workspace_name: String,
    /// `강사료`, `교재·재료비` 등 사용자가 고른 품의 종류
    pub item_label: String,
    pub item_codes: Vec<String>,
    pub columns: Vec<ProposalColumn>,
    pub rows: Vec<ProposalRow>,
    /// 마지막 전체 합계 행
    pub total: ProposalRow,
    /// 고른 비용항목의 정산 총액 — 교차검증에 쓴다
    pub settlement_total: i64,
    pub balanced: bool,
    pub settled_at: String,
}

/// 품의로 뽑을 수 있는 종류 (요구사항 §10).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalKind {
    pub key: String,
    pub label: String,
    pub item_codes: Vec<String>,
}

/// 학생 한 명의 정산 내역 — 부서별 상세 팝업에 쓴다 (요구사항 §3·§4).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentAllocRow {
    pub department_id: i64,
    pub dept_label: String,
    pub item_code: String,
    pub item_name: String,
    pub fund: String,
    pub fund_label: String,
    pub origin: String,
    pub origin_label: String,
    pub amount: i64,
}

// ─────────────────────────────────────────────── 학생 단위 집계 (v0.1.4)
//
// 네 화면(학생별 징수 내역 · 수익자 · 방과후 이용권 · 자유수강권)이 같은 모양을
// 쓴다 — **목록에서 학생이 얼마인지 보고, 눌러서 왜 그 금액인지 본다.**
//
// 다만 **어디서 나온 돈인지는 화면마다 다르다.** 하나의 SQL로 억지로 합치지
// 않는 까닭이다.
//
//   학생별 징수 내역 → 원본 `charge` 전체
//   수익자          → 정산 배분 중 학부모 부담(SELF_PAY + VOUCHER_OVER)
//   방과후 이용권    → 정산 배분 중 이용권
//   자유수강권      → 정산 배분 중 자유수강권

/// 학생 한 명의 항목별 합계 — 목록 한 줄.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentSumRow {
    pub student_id: i64,
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    /// 이 작업공간 기간에 유효한 지원제도
    pub programs: Vec<String>,
    /// 항목별 합계 — 이 학생이 수강하는 모든 부서를 더한 값
    pub fees: Vec<Fee>,
    pub total: i64,
    /// 이 학생의 상세 줄 수. 누르면 이만큼 나온다
    pub details: i64,
}

/// 학생별 징수 내역 (행정자료).
///
/// **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`) 기준이므로
/// 정산이 없거나 낡아도 조회된다 — 정산 전에 금액을 대조하는 자료다.
///
/// 필터는 **학생을 찾는 조건**이고, 금액은 언제나 찾은 학생의 **전체** 합계다.
/// `2학년`으로 걸러도 그 학생의 모든 부서가 더해진다. 그래서 `details`에는
/// 걸러진 부서만이 아니라 찾은 학생의 모든 수강 건이 들어간다 —
/// 그러지 않으면 목록 합계와 상세 합계가 어긋난다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeReport {
    /// 학생당 한 줄
    pub rows: Vec<StudentSumRow>,
    /// 부서별 상세 — 학생을 눌렀을 때와 Excel 둘째 장에 쓴다
    pub details: Vec<Enrollment>,
    /// 학생 수 (= `rows.len()`)
    pub students: i64,
    /// 수강 건수 (= `details.len()`)
    pub enrollments: i64,
    /// 항목별 총합 — 화면 요약과 Excel 합계 줄이 이 값을 그대로 쓴다
    pub fees: Vec<Fee>,
    pub total: i64,
}

/// 수익자 탭 (정산 결과). 목록은 학생별 합계, 상세는 부서별이다.
///
/// **원본 `charge`가 아니라 정산 스냅샷의 배분액이다.** 이용권으로 정상
/// 지원된 금액은 여기 들어오지 않는다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfPayReport {
    /// 학생당 한 줄
    pub rows: Vec<StudentSumRow>,
    /// 학생 × 부서 상세 — 지금까지 쓰던 줄 그대로다
    pub details: Vec<SelfPayRow>,
    /// 항목별 총합
    pub fees: Vec<Fee>,
    pub total: i64,
}

// ─────────────────────────────────────────────── 부서별 금액 Excel 수정 (v0.1.4)
//
// 부서 하나를 고른 뒤 그 부서 수강생의 금액을 Excel로 한꺼번에 고친다.
//
// **파일이 곧바로 DB를 바꾸지 않는다.** 먼저 무엇이 어떻게 바뀌는지 보여 주고,
// 사람이 [변경사항 반영]을 눌렀을 때만 한 트랜잭션으로 쓴다.

/// 실제로 바뀌는 칸 하나. 값이 같은 칸은 여기 들어오지 않는다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeChange {
    pub enrollment_id: i64,
    /// `1학년 가람반 3번 홍길동`
    pub student_label: String,
    pub item_code: String,
    pub item_name: String,
    pub before: i64,
    pub after: i64,
}

/// 반영하지 않는 줄과 그 까닭.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRowIssue {
    /// Excel 화면의 행 번호
    pub line: usize,
    /// 파일에 적혀 있던 대로의 학생 표시
    pub label: String,
    pub message: String,
}

/// 반영 전 미리보기.
///
/// 검증은 **파일 전체**를 먼저 본다. 오류가 있으면 `token`이 비어 있고 아무것도
/// 반영할 수 없다 — 절반만 들어가는 일이 없어야 하기 때문이다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeePreview {
    /// 반영에 쓸 열쇠. 오류가 있으면 빈 문자열이다.
    pub token: String,
    pub file_name: String,
    /// `로봇과학A반`
    pub dept_label: String,
    /// 파일에서 읽은 자료 줄 수
    pub total: i64,
    /// 금액이 실제로 바뀌는 학생 수
    pub students: i64,
    /// 바뀌는 칸 수
    pub cells: i64,
    /// 찾았지만 값이 같아 바뀌지 않는 학생 수
    pub unchanged: i64,
    /// 이 부서에서 찾지 못한 줄
    pub unmatched: Vec<FeeRowIssue>,
    /// 형식이 잘못된 줄
    pub errors: Vec<FeeRowIssue>,
    pub changes: Vec<FeeChange>,
}

/// 반영 결과.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeApplyResult {
    /// 금액이 바뀐 학생 수
    pub students: i64,
    /// 바뀐 칸 수
    pub cells: i64,
}

// ─────────────────────────────────────────────── 추가징수 · 환불 (v0.1.5)
//
// 최초 징수 이후에 생긴 행정처리 기록이다. 수강생 명단(지금 얼마를 내는가)과
// 다른 자료이므로 금액을 만든 시점의 값으로 굳혀 둔다.

/// 조정 한 건의 항목별 금액.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentFee {
    pub item_code: String,
    pub item_name: String,
    /// 이 조정의 금액 — 추가징수면 더 걷을 돈, 환불이면 돌려줄 돈
    pub amount: i64,
    /// 환불 기준이 된 취소 직전 금액 (추가징수는 0)
    pub base_amount: i64,
    /// 마지막으로 사람이 확인한 그때의 원본 charge
    pub checked_charge: i64,
    /// 지금 수강생 명단의 charge
    pub current_charge: i64,
}

/// 조정 한 건 (= 수강 건 하나에 대한 추가징수 또는 환불).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    pub id: i64,
    pub kind: String,
    pub student_id: i64,
    pub enrollment_id: i64,
    pub department_id: i64,
    // 지금 정보 — 목록·필터·정렬이 쓴다
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    pub dept_label: String,
    /// 이 수강 건이 지금 수강중인가 취소인가
    pub enrollment_status: String,
    // 만든 때의 표시 — 반이나 부서명이 바뀌어도 그때를 알 수 있게 병기한다
    pub student_label_at: String,
    pub dept_label_at: String,
    pub occurred_on: String,
    pub note: String,
    pub fees: Vec<AdjustmentFee>,
    pub total: i64,
    /// 원본 금액이 달라져 사람이 확인해야 하는가
    pub needs_check: bool,
    pub created_at: String,
}

/// 추가·취소 관리 한 탭의 결과 — 학생별 합계 + 부서별 상세.
///
/// 학생별 징수 내역과 같은 모양이다. 다만 원본이 `charge` 가 아니라 이
/// 조정 기록이다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentReport {
    /// 학생당 한 줄
    pub rows: Vec<StudentSumRow>,
    /// 조정 건별 상세
    pub details: Vec<Adjustment>,
    pub students: i64,
    /// 조정 건수
    pub count: i64,
    /// 항목별 총합
    pub fees: Vec<Fee>,
    pub total: i64,
    /// 이 가운데 확인이 필요한 건수
    pub needs_check: i64,
}

/// 추가·취소 관리 화면이 한 번에 받는 것 — 두 탭 + 확인 필요 건수.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentView {
    pub additional: AdjustmentReport,
    pub refund: AdjustmentReport,
    /// 작업공간 전체(필터와 무관)의 확인 필요 건수 — 사이드바 배지에 쓴다
    pub needs_check_all: i64,
}

/// 추가·취소 관리 목록 조건.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentFilter {
    /// 발생일 기간 'YYYY-MM-DD'
    pub from: Option<String>,
    pub to: Option<String>,
    pub grade: Option<i64>,
    pub class_no: Option<String>,
    /// 부서는 **학생을 찾는 조건**이다 (학생별 징수 내역과 같은 원칙)
    pub department_id: Option<i64>,
    pub query: Option<String>,
}

/// 추가징수·환불을 만들 때 함께 받는 것.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentInput {
    /// 발생일. 비우면 오늘
    pub occurred_on: Option<String>,
    pub note: Option<String>,
}

/// 원본이 달라진 칸 하나 — 변경내역 확인 화면이 보여 준다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentDiff {
    pub adjustment_id: i64,
    pub kind: String,
    pub student_label: String,
    pub dept_label: String,
    pub occurred_on: String,
    pub item_code: String,
    pub item_name: String,
    /// 지금 기록에 들어 있는 조정 금액
    pub saved: i64,
    /// 지금 기준으로 다시 계산한 조정 금액
    pub suggested: i64,
    /// 마지막으로 확인한 그때의 원본 charge
    pub checked_charge: i64,
    /// 지금 원본 charge
    pub current_charge: i64,
    /// 환불 기준(취소 직전 금액). 추가징수는 0
    pub base_amount: i64,
    /// 지금 기준으로 다시 계산하면 음수가 되는가 — 그러면 반영할 수 없다
    pub negative: bool,
}

// ─────────────────────────────────────────── 작업공간 자료 가져오기 (v0.1.6)
//
// 새 기수를 시작할 때마다 부서와 수강생을 처음부터 다시 넣지 않도록, 같은
// 학년도의 다른 작업공간에서 **운영자료**를 옮겨 온다.
//
// 학생정보와 지원대상자는 학년도 소속이라 이미 공유된다 — 복제하지 않는다.
// 금액(charge·override)·추가징수·환불·정산은 지난 기간의 결과이므로 넘기지
// 않는다. 새 기간은 새 징수명단을 만드는 자리다.

/// 무엇을 가져올지.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCopyInput {
    pub source_workspace_id: i64,
    pub departments: bool,
    /// 수강생을 가져오려면 부서도 함께 가져와야 한다 — 수강은 부서를 가리키기
    /// 때문이다. 명령 계층이 이 규칙을 검사한다.
    pub enrollments: bool,
}

/// 만들기 전에 보여 주는 요약 — 무엇이 넘어오고 무엇이 넘어오지 않는지.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCopyPreview {
    pub source_workspace_id: i64,
    pub source_name: String,
    pub departments: i64,
    /// 가져올 수강 (수강중)
    pub active_enrollments: i64,
    /// 가져오지 않는 수강 (취소)
    pub cancelled_enrollments: i64,
    /// 가져오지 않는 것들 — 화면에 그대로 적는다
    pub adjustments: i64,
    pub settlements: i64,
    /// 학생별로 따로 고쳐 둔 금액 칸 수. 새 기간에는 넘기지 않는다
    pub overridden_cells: i64,
    /// 부서 차감 우선순위가 정해져 있는가
    pub has_priority: bool,
    /// 눈에 띄는 문제 — 조용히 건너뛰지 않고 여기 적는다
    pub warnings: Vec<String>,
}

/// 가져오기 결과.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCopyResult {
    pub workspace_id: i64,
    pub departments: i64,
    pub enrollments: i64,
    pub charges: i64,
}

// ─────────────────────────────────────────── 다중 정렬 · 페이지 (v0.1.6)

/// 정렬 한 칸. `key` 는 **미리 정해 둔 이름**이라야 한다 — 화면이 보낸 글자를
/// SQL 에 그대로 넣지 않는다.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub key: String,
    /// `ASC` | `DESC`
    pub dir: String,
}

/// 한 쪽 분량의 수강생 명단.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentPage {
    /// 이 쪽에 보일 줄만
    pub rows: Vec<Enrollment>,
    /// 필터에 걸린 전체 건수 (쪽 나누기 전)
    pub total: i64,
    /// 그 가운데 수강중
    pub active_total: i64,
    /// **필터에 걸린 수강중 전체**의 항목별 합계. 쪽을 나눠도 화면 아래 합계가
    /// 이 쪽만의 합이 되면 안 되므로 서버에서 센다.
    pub fees: Vec<Fee>,
    /// 위 합계의 총액
    pub amount_total: i64,
    /// 1부터
    pub page: i64,
    pub page_size: i64,
    pub page_count: i64,
}

/// 필터 드롭다운을 채울 값들.
///
/// 예전에는 전체 명단을 한 번 더 읽어 여기서 뽑았다. 쪽 나누기를 넣은 뒤에도
/// 그러면 무거운 조회가 그대로 남으므로, 서로 다른 값만 따로 읽는다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentFilterOptions {
    pub grades: Vec<i64>,
    /// 학년별 반 — `[["1", …], …]` 가 아니라 (학년, 반) 쌍으로 준다
    pub classes: Vec<GradeClass>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeClass {
    pub grade: i64,
    pub class_no: String,
}
