/** Rust `model.rs`와 짝을 이루는 타입. 한쪽을 고치면 다른 쪽도 고친다. */

export type ProgramCode = 'VOUCHER' | 'FREE_VOUCHER'

export interface Year {
  id: number
  year: number
  name: string
  startDate: string
  endDate: string
  dataVersion: number
  isCurrent: boolean
}

export interface Workspace {
  id: number
  yearId: number
  name: string
  startDate: string
  endDate: string
  dataVersion: number
  isCurrent: boolean
  note: string
  departmentCount: number
  enrollmentCount: number
}

export interface WorkspaceInput {
  name: string
  startDate: string
  endDate: string
  note?: string
}

export interface Student {
  id: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  note: string
  programs: ProgramCode[]
}

export interface StudentInput {
  grade: number
  classNo: string
  studentNo: number
  name: string
  note?: string
}

export interface StudentFilter {
  workspaceId?: number | null
  grade?: number | null
  classNo?: string | null
  studentNo?: number | null
  name?: string | null
  /** `VOUCHER` | `FREE_VOUCHER` | `BOTH` | `NONE` */
  program?: string | null
  query?: string | null
}

export interface Eligibility {
  id: number
  studentId: number
  program: ProgramCode
  validFrom: string | null
  validTo: string | null
  source: 'MANUAL' | 'EXCEL'
  note: string
  grade: number
  classNo: string
  studentNo: number
  name: string
  gradeMismatch: boolean
}

export interface EligibilityInput {
  studentId: number
  program: ProgramCode
  validFrom?: string | null
  validTo?: string | null
  note?: string
}

export interface EligibilityView {
  rows: Eligibility[]
  targetGradeText: string
  mismatchCount: number
}

export interface CostItem {
  code: string
  name: string
  sortOrder: number
}

export interface Fee {
  itemCode: string
  amount: number
}

export interface Department {
  id: number
  name: string
  className: string
  teacher: string
  days: string
  note: string
  fees: Fee[]
  total: number
  enrollmentCount: number
}

export interface DepartmentInput {
  name: string
  className?: string
  teacher?: string
  days?: string
  note?: string
  fees: Fee[]
}

export interface Period {
  id?: number | null
  name: string
  startDate: string
  endDate: string
  limitAmount: number
  seq: number
}

export interface Policy {
  id: number
  program: ProgramCode
  annualLimit: number
  carryover: boolean
  targetGrades: string
  labelFund: string
  labelOver: string
  periods: Period[]
}

export interface PolicyView {
  policy: Policy
  notice: string | null
}

export interface Bootstrap {
  years: Year[]
  currentYear: Year | null
  workspaces: Workspace[]
  currentWorkspace: Workspace | null
  costItems: CostItem[]
  dbPath: string
}

export interface RowIssue {
  row: number
  cells: string[]
  reason: string
}

export interface ImportPreview {
  token: string
  kind: string
  fileName: string
  headers: string[]
  total: number
  okCount: number
  errors: RowIssue[]
  warnings: RowIssue[]
  preview: string[][]
}

export interface ImportResult {
  added: number
  updated: number
}

export interface ExportResult {
  path: string
  name: string
  rows: number
}

export type ImportKind = 'students' | 'eligibility' | 'departments' | 'enrollments'

// ─────────────────────────────────────────────── Phase 2 — 수강 관리

export type EnrollStatus = 'ACTIVE' | 'CANCELLED'

export interface Enrollment {
  id: number
  studentId: number
  departmentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  programs: ProgramCode[]
  deptName: string
  deptClassName: string
  /** 화면에 쓰는 부서 표시명 (`로봇과학A반`) */
  deptLabel: string
  fees: Fee[]
  total: number
  /** 학생별로 따로 고친 금액이 하나라도 있는가 */
  hasOverride: boolean
  status: EnrollStatus
  changeReason: string
  updatedAt: string
}

export interface EnrollmentInput {
  studentId: number
  departmentId: number
  /** 비우면 부서 기준 수강료를 그대로 쓴다 */
  fees: Fee[]
  reason?: string
}

export interface EnrollmentFilter {
  departmentId?: number | null
  grade?: number | null
  classNo?: string | null
  program?: string | null
  status?: EnrollStatus | null
  query?: string | null
}

export interface FeeDiff {
  enrollmentId: number
  studentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  departmentId: number
  deptLabel: string
  itemCode: string
  itemName: string
  current: number
  base: number
  isOverridden: boolean
}

export interface FeeDiffView {
  rows: FeeDiff[]
  overridden: number
}

export interface FeePick {
  enrollmentId: number
  itemCode: string
}

export type ApplyMode = 'KEEP_EDITED' | 'ALL' | 'SELECTED'

export interface ApplyResult {
  changed: number
  kept: number
  enrollments: number
}

export interface StudentFeeEdit {
  enrollmentId: number
  fees: Fee[]
}

export interface ChangeLog {
  id: number
  at: string
  workspaceName: string
  kind: string
  kindLabel: string
  target: string
  beforeValue: string
  afterValue: string
  reason: string
}

export interface SupportView {
  program: ProgramCode
  programLabel: string
  /** false면 화면에 '해당없음'을 쓴다 */
  eligible: boolean
  periods: string[]
  gradeMismatch: boolean
}

export interface WorkspaceEnrollments {
  workspaceId: number
  workspaceName: string
  startDate: string
  endDate: string
  rows: Enrollment[]
  activeTotal: number
}

export interface StudentDetail {
  student: Student
  supports: SupportView[]
  workspaces: WorkspaceEnrollments[]
}

// ─────────────────────────────────────────────── Phase 3 — 정산

export interface Issue {
  /** ERROR면 정산할 수 없고, WARN이면 확인 후 진행할 수 있다 */
  level: 'ERROR' | 'WARN'
  code: string
  message: string
}

export type SettleState = 'NONE' | 'FRESH' | 'STALE_DATA' | 'STALE_YEAR' | 'STALE_PRIOR'

export interface SettlementStatus {
  state: SettleState
  message: string
  settlementId: number | null
  createdAt: string | null
  programOrder: string
  priorName: string | null
  priorIsEarlierPeriod: boolean
}

export interface GenerateResult {
  settlementId: number
  createdAt: string
  students: number
  allocs: number
  total: number
  warnings: Issue[]
}

export interface SummaryRow {
  itemCode: string
  itemName: string
  selfPay: number
  voucher: number
  voucherOver: number
  freeVoucher: number
  total: number
}

export interface Summary {
  rows: SummaryRow[]
  total: SummaryRow
  chargeTotal: number
  balanced: boolean
  createdAt: string
}

export interface BudgetView {
  program: ProgramCode
  annualLimit: number
  periodName: string
  periodLimit: number
  carryover: boolean
  carryIn: number
  usedPriorPeriods: number
  usedInPeriodBefore: number
  usedAllBefore: number
  cappedByAnnual: boolean
  available: number
  usedNow: number
  periodLeft: number
  annualUsed: number
  annualLeft: number
}

export interface SelfPayRow {
  studentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  departmentId: number
  deptLabel: string
  fees: Fee[]
  total: number
  selfPay: number
  voucherOver: number
  originPlain: number
  originVoucher: number
  originFree: number
}

export interface ProgramRow {
  studentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  used: Fee[]
  usedTotal: number
  over: Fee[]
  overTotal: number
  budget: BudgetView | null
}

/** NONE=해당없음 · BEFORE=정산 전 · STALE=재정산 필요 · OK=최신 */
export type SupportStateCode = 'NONE' | 'BEFORE' | 'STALE' | 'OK'

export interface SupportState {
  program: ProgramCode
  programLabel: string
  state: SupportStateCode
  budget: BudgetView | null
}

export interface PriorityRow {
  key: string
  label: string
  sortOrder: number
  voucherStudents: number
}

export interface Grant {
  id: number
  studentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  program: ProgramCode
  /** null이면 연간 한도 예외, 값이 있으면 그 지원기간의 한도 예외 */
  periodId: number | null
  periodName: string
  amount: number
  reason: string
}

export interface GrantInput {
  studentId: number
  program: ProgramCode
  periodId: number | null
  amount: number
  reason?: string
}

// ─────────────────────────────────────────────── Phase 4 — 행정자료

export interface ProposalColumn {
  /** 안정된 재원 코드 */
  fund: string
  /** 그 학년도의 표시 이름 (`3학년 지원금`) */
  label: string
}

export interface ProposalRow {
  departmentId: number
  deptLabel: string
  /** columns 차례대로의 금액 */
  amounts: number[]
  total: number
}

export interface Proposal {
  yearName: string
  workspaceName: string
  itemLabel: string
  itemCodes: string[]
  columns: ProposalColumn[]
  rows: ProposalRow[]
  total: ProposalRow
  settlementTotal: number
  balanced: boolean
  settledAt: string
}

export interface ProposalKind {
  key: string
  label: string
  itemCodes: string[]
}

export interface StudentAllocRow {
  departmentId: number
  deptLabel: string
  itemCode: string
  itemName: string
  fund: string
  fundLabel: string
  origin: string
  originLabel: string
  amount: number
}

/** 정산 결과 Excel 종류 */
export type SettleExportKind = 'self_pay' | 'voucher' | 'free_voucher'

// ─────────────────────────────────────────────── Phase 5 — 배포 · 백업

export interface AppInfo {
  version: string
  /** 자료구조 버전 — 백업 호환 판단에 쓴다 */
  schemaVersion: number
  dbPath: string
  dataDir: string
  backupDir: string
  exportDir: string
  logDir: string
  releaseUrl: string
  dbSize: number
}

export interface BackupFile {
  name: string
  path: string
  kind: string
  /** `수동 백업` 등 사람이 읽는 종류 */
  kindLabel: string
  createdAt: string
  size: number
  /** 자동 정리 대상인가 */
  prunable: boolean
}

export interface BackupInfo {
  userVersion: number
  appVersion: number
  tables: number
  tooNew: boolean
  yearCount: number
  studentCount: number
  enrollmentCount: number
  settlementCount: number
}

export interface RestoreReport {
  fromVersion: number
  toVersion: number
  /** 구버전 백업이라 자료구조를 올렸는가 */
  migrated: boolean
  safetyBackup: string
}

/** 대량 삭제 결과 — 되돌릴 수 있는 백업 이름을 함께 준다 */
export interface BulkDeleteResult {
  deleted: number
  backup: string
}

/**
 * 학생 한 명의 항목별 합계 — 목록 한 줄 (v0.1.4).
 *
 * 네 화면(학생별 징수 내역 · 수익자 · 방과후 이용권 · 자유수강권)이 같은 모양을
 * 쓴다. 다만 **어디서 나온 돈인지는 화면마다 다르다** — 징수 내역은 원본
 * `charge`, 나머지 셋은 정산 스냅샷의 배분액이다.
 */
export interface StudentSumRow {
  studentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  programs: string[]
  /** 항목별 합계 — 이 학생이 수강하는 모든 부서를 더한 값 */
  fees: Fee[]
  total: number
  /** 이 학생의 상세 줄 수 */
  details: number
}

/**
 * 학생별 징수 내역 (행정자료).
 *
 * **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`) 기준이므로
 * 정산이 없거나 낡아도 조회된다.
 *
 * 필터는 **학생을 찾는 조건**이고, 금액은 언제나 찾은 학생의 **전체** 합계다.
 * 그래서 `details`에는 걸러진 부서만이 아니라 찾은 학생의 모든 수강 건이 있다.
 */
export interface FeeReport {
  /** 학생당 한 줄 */
  rows: StudentSumRow[]
  /** 부서별 상세 — 학생을 눌렀을 때와 Excel 둘째 장 */
  details: Enrollment[]
  students: number
  enrollments: number
  /** 항목별 총합 — 화면 요약이 이 값을 쓴다 */
  fees: Fee[]
  total: number
}

export interface SelfPayReport {
  rows: StudentSumRow[]
  details: SelfPayRow[]
  fees: Fee[]
  total: number
}

/**
 * 부서별 금액 Excel 수정 (v0.1.4).
 *
 * 파일이 곧바로 DB를 바꾸지 않는다. 먼저 무엇이 어떻게 바뀌는지 보여 주고,
 * 사람이 [변경사항 반영]을 눌렀을 때만 한 트랜잭션으로 쓴다.
 */
export interface FeeChange {
  enrollmentId: number
  studentLabel: string
  itemCode: string
  itemName: string
  before: number
  after: number
}

export interface FeeRowIssue {
  line: number
  label: string
  message: string
}

export interface FeePreview {
  /** 반영에 쓸 열쇠. 오류가 있거나 바뀔 것이 없으면 빈 문자열이다 */
  token: string
  fileName: string
  deptLabel: string
  total: number
  students: number
  cells: number
  unchanged: number
  unmatched: FeeRowIssue[]
  errors: FeeRowIssue[]
  changes: FeeChange[]
}

export interface FeeApplyResult {
  students: number
  cells: number
}

// ─────────────────────────────── 추가징수 · 환불 (v0.1.5)
//
// 수강생 명단(지금 얼마를 내는가)과 다른 자료다. 최초 징수 이후에 생긴
// 추가징수·환불이라는 행정처리 기록이고, 금액은 만든 시점에서 굳는다.

export type AdjustmentKind = 'ADDITIONAL_CHARGE' | 'REFUND'

export interface AdjustmentFee {
  itemCode: string
  itemName: string
  /** 이 조정의 금액 — 추가징수면 더 걷을 돈, 환불이면 돌려줄 돈 */
  amount: number
  /** 환불 기준이 된 취소 직전 금액 (추가징수는 0) */
  baseAmount: number
  /** 마지막으로 사람이 확인한 그때의 원본 charge */
  checkedCharge: number
  /** 지금 수강생 명단의 charge */
  currentCharge: number
}

export interface Adjustment {
  id: number
  kind: AdjustmentKind
  studentId: number
  enrollmentId: number
  departmentId: number
  // 지금 정보 — 목록·필터·정렬이 쓴다
  grade: number
  classNo: string
  studentNo: number
  name: string
  deptLabel: string
  enrollmentStatus: EnrollStatus
  // 만든 때의 표시 — 반이나 부서명이 바뀌어도 그때를 알 수 있다
  studentLabelAt: string
  deptLabelAt: string
  occurredOn: string
  note: string
  fees: AdjustmentFee[]
  total: number
  needsCheck: boolean
  createdAt: string
}

export interface AdjustmentReport {
  rows: StudentSumRow[]
  details: Adjustment[]
  students: number
  count: number
  fees: Fee[]
  total: number
  needsCheck: number
}

export interface AdjustmentView {
  additional: AdjustmentReport
  refund: AdjustmentReport
  /** 화면 필터와 무관한 전체 확인 필요 건수 — 사이드바 배지 */
  needsCheckAll: number
}

export interface AdjustmentFilter {
  from?: string | null
  to?: string | null
  grade?: number | null
  classNo?: string | null
  /** 부서는 **학생을 찾는 조건**이다 */
  departmentId?: number | null
  query?: string | null
}

/** 추가징수·환불을 만들 때 함께 보내는 것. 보내지 않으면 기록을 만들지 않는다. */
export interface AdjustmentInput {
  /** 비우면 오늘 */
  occurredOn?: string | null
  note?: string | null
}

export interface AdjustmentDiff {
  adjustmentId: number
  kind: AdjustmentKind
  studentLabel: string
  deptLabel: string
  occurredOn: string
  itemCode: string
  itemName: string
  /** 지금 기록에 들어 있는 조정 금액 */
  saved: number
  /** 지금 기준으로 다시 계산한 조정 금액 */
  suggested: number
  checkedCharge: number
  currentCharge: number
  baseAmount: number
  /** 다시 계산하면 음수가 되는가 — 그러면 반영할 수 없다 */
  negative: boolean
}

// ─────────────────────────────── 작업공간 자료 가져오기 (v0.1.6)

export interface WorkspaceCopyInput {
  sourceWorkspaceId: number
  departments: boolean
  /** 수강생을 가져오려면 부서도 함께 가져와야 한다 */
  enrollments: boolean
}

export interface WorkspaceCopyPreview {
  sourceWorkspaceId: number
  sourceName: string
  departments: number
  activeEnrollments: number
  cancelledEnrollments: number
  adjustments: number
  settlements: number
  overriddenCells: number
  hasPriority: boolean
  warnings: string[]
}

export interface WorkspaceCopyResult {
  workspaceId: number
  departments: number
  enrollments: number
  charges: number
}

// ─────────────────────────────── 쪽 나누기 (v0.1.6)

export interface EnrollmentPage {
  rows: Enrollment[]
  /** 필터에 걸린 전체 건수 (쪽 나누기 전) */
  total: number
  activeTotal: number
  /** 필터에 걸린 수강중 전체의 항목별 합계 — 쪽만의 합이 아니다 */
  fees: Fee[]
  amountTotal: number
  page: number
  pageSize: number
  pageCount: number
}

export interface GradeClass {
  grade: number
  classNo: string
}

export interface EnrollmentFilterOptions {
  grades: number[]
  classes: GradeClass[]
}
