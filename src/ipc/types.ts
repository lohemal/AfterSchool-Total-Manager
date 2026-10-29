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

/** `ATTENDING` 재학 · `LEAVING` 전출 예정 · `LEFT` 전출 */
export type Enrolment = 'ATTENDING' | 'LEAVING' | 'LEFT'

export interface Student {
  id: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  note: string
  programs: ProgramCode[]
  /** 전출일. null 이면 재학. 상태는 이 날짜와 오늘로 **판정한다** */
  transferOutDate: string | null
  /** 오늘 기준으로 낸 재학상태 — 저장된 값이 아니다 */
  enrolment: Enrolment
}

export interface TransferCheck {
  studentId: number
  studentLabel: string
  activeCount: number
  activeDepartments: string[]
  pastEnrollments: number
  /** 전출일이 이미 지났는데 수강이 남아 막히는 상태 */
  blocked: boolean
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
  /** 정원. null 이면 **미설정** — 0명이 아니다 */
  capacity: number | null
  /** 수강 가능 학년. **빈 배열이면 미설정** — 전 학년도 전 학년 불가도 아니다 */
  allowedGrades: number[]
  /** 맞춤형 1회당 기본 차시. 0 이면 맞춤형이 아니다 */
  customSessions: number
  enrollmentCount: number
}

export interface DepartmentInput {
  name: string
  className?: string
  teacher?: string
  days?: string
  note?: string
  fees: Fee[]
  capacity: number | null
  allowedGrades: number[]
  /** 맞춤형 1회당 기본 차시. 0 이면 맞춤형이 아니다 */
  customSessions: number
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
  /** 발생 유형 (v0.1.9) */
  source: AdjustmentSource
  /** 부서 일괄로 함께 생긴 기록이면 그 묶음 */
  batchId: number | null
}

/**
 * 발생 유형.
 *
 * `LEGACY` 는 v0.1.9 이전의 기록이다 — '수강취소'라고 단정하지 않는다.
 */
export type AdjustmentSource = 'LEGACY' | 'ENROLL' | 'CANCEL' | 'DEPT_BATCH' | 'INDIVIDUAL'

/** 부서 일괄 묶음 한 건. 합계는 딸린 기록을 더해 낸 값이다. */
export interface BillingBatch {
  id: number
  departmentId: number
  deptLabel: string
  occurredOn: string
  note: string
  createdAt: string
  count: number
  students: number
  refundTotal: number
  additionalTotal: number
}

/**
 * 차액을 기록할지.
 *
 * `register = false` 면 **금액만 고친다**(잘못 넣은 값을 바로잡는 일). 방향은
 * 사람이 고르지 않는다 — 줄었으면 환불, 늘었으면 추가징수다.
 */
export interface DeltaRegister {
  register: boolean
  occurredOn: string | null
  note: string | null
}

export interface BatchResult {
  changed: number
  batchId: number | null
  refunds: number
  additionals: number
  refundTotal: number
  additionalTotal: number
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
  /** 발생 유형 (v0.1.9). 비우면 모두 */
  source?: AdjustmentSource | null
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

// ─────────────────────────────────────────────── 부서별 수강현황 (v0.1.7)

export type CapacityStatus = 'UNSET' | 'OPEN' | 'FULL' | 'OVER'

export interface DeptCapacityRow {
  departmentId: number
  name: string
  className: string
  teacher: string
  days: string
  /** 빈 배열이면 대상 학년 미설정 */
  allowedGrades: number[]
  capacity: number | null
  currentCount: number
  /** 정원 미설정이면 null. 넘쳤으면 음수 그대로 */
  remaining: number | null
  fillRate: number | null
  status: CapacityStatus
}

export interface CapacitySummary {
  classes: number
  classesWithCapacity: number
  classesWithoutCapacity: number
  totalCapacity: number
  countedCurrent: number
  totalEnrollments: number
  totalStudents: number
  /** sum(max(정원 - 현재, 0)) — 실제로 더 받을 수 있는 자리 */
  openSeats: number
  avgFillRate: number | null
  overClasses: number
}

export interface GradeStatRow {
  grade: number
  students: number
  enrollments: number
  totalStudents: number
  joinRate: number | null
}

export interface WeekdayStatRow {
  day: string
  classes: number
  enrollments: number
  classesWithCapacity: number
  totalCapacity: number
  openSeats: number
}

export interface CapacityStats {
  summary: CapacitySummary
  rows: DeptCapacityRow[]
  grades: GradeStatRow[]
  weekdays: WeekdayStatRow[]
}

export type SeatFinding = 'OPEN' | 'CAPACITY_UNKNOWN' | 'GRADE_UNKNOWN' | 'FULL' | 'OVER'

export interface SeatRow {
  departmentId: number
  name: string
  className: string
  teacher: string
  days: string
  allowedGrades: number[]
  capacity: number | null
  currentCount: number
  remaining: number | null
  finding: SeatFinding
}

export interface SeatQuery {
  grade: number
  studentId: number | null
  includeClosed: boolean
}

export interface SeatDayGroup {
  day: string
  rows: SeatRow[]
}

export interface SeatResult {
  byDay: SeatDayGroup[]
  /** 고른 학생이 이미 듣고 있어 뺀 반 */
  excluded: string[]
}

// ─────────────────────────────────────────────── 맞춤형 강사료 (v0.1.8)

export type SessionKind = 'NORMAL' | 'CLOSED' | 'EXTRA'

export interface SessionDay {
  date: string
  weekday: string
  kind: SessionKind
  sessions: number
  reason: string
  /** 사람이 손대어 기본값과 달라진 날 */
  overridden: boolean
  /** 공통 휴업일 때문에 쉬는 날 */
  fromClosure: boolean
}

export interface CustomFeeRow {
  departmentId: number
  name: string
  className: string
  teacher: string
  days: string
  baseSessions: number
  plannedDays: number
  closedDays: number
  extraDays: number
  countedDays: number
  totalSessions: number
  rate: number
  fee: number
  sessionsList: SessionDay[]
}

export interface Closure {
  id: number
  date: string
  weekday: string
  reason: string
}

export interface OrphanSession {
  departmentId: number
  departmentLabel: string
  date: string
  sessions: number
  reason: string
  why: string
}

export interface CustomFeeSummary {
  classes: number
  plannedDays: number
  closedDays: number
  extraDays: number
  countedDays: number
  totalSessions: number
  totalFee: number
}

export interface CustomFeeReport {
  yearName: string
  workspaceName: string
  startDate: string
  endDate: string
  rate: number
  summary: CustomFeeSummary
  rows: CustomFeeRow[]
  closures: Closure[]
  /** 기간 밖이거나 효력 없는 기록 — 지우지 않고 알린다 */
  orphans: OrphanSession[]
}

/** 기록을 지우면 학생 금액이 어떻게 되는가 (v0.1.9) */
export interface RevertRow {
  adjustmentId: number
  kind: string
  source: AdjustmentSource
  studentLabel: string
  deptLabel: string
  amount: number
  /** 차액 기록이면 true — 지우면 금액도 되돌린다 */
  reverts: boolean
  chargeBefore: number
  chargeAfter: number
  /** 되돌릴 수 없어 막히는 까닭 */
  blocked: string | null
}

export interface RevertPreview {
  rows: RevertRow[]
  students: number
  refundTotal: number
  additionalTotal: number
  blocked: boolean
}

// ─────────────────────────────────────────────── 방과후드림 결제관리 (P2)

/** 개인정보 암호 키 상태 */
export type DreamKeyStatus = 'READY' | 'NOT_INITIALIZED' | 'KEY_UNAVAILABLE'

export interface DreamEnter {
  keyStatus: DreamKeyStatus
  /** 이번에 정리한 30일 지난 처리 이력 수 */
  cleaned: number
  cleanupError: string | null
}

export interface DreamSettings {
  year: number
  yearName: string
  workspaceName: string
  orgCode: string | null
  /** YYYYMM */
  billingMonth: string | null
  /** 방과후드림 적용 학년(비어 있으면 미설정) */
  appliedGrades: number[]
  /** 고를 수 있는 학년 */
  gradeChoices: number[]
  /** 제안 — 앞선 작업공간의 적용 학년 [작업공간 이름, 학년]. 저장된 것이 아니다 */
  suggestedGrades: [string, number[]] | null
}

export interface DreamGradesSaved {
  grades: number[]
  changed: boolean
  closedJobs: number
}

export type DreamSeverity = 'INFO' | 'REVIEW' | 'ERROR'
export type DreamStatus = 'OK' | 'REVIEW' | 'ERROR'

/** 문제 하나. 문장은 화면이 만든다(`lib/dream`). `found` 에는 개인정보가 없다. */
export interface DreamIssue {
  code: string
  severity: DreamSeverity
  /** Excel 행 번호 */
  row: number | null
  /** 열 문자 A~R */
  column: string | null
  expected: string | null
  found: string | null
}

export interface DreamPreviewRow {
  sourceRow: number
  /** A~R 18칸 — 개인참조번호(C) · 생년월일(H)은 가려져 온다 */
  cells: string[]
  status: DreamStatus
  issues: DreamIssue[]
}

export interface DreamFileCheck {
  fileName: string
  fileSize: number
  sha256: string
  sheetName: string | null
  dataRows: number
  status: DreamStatus
  fileIssues: DreamIssue[]
  rowErrorCount: number
  rowInfoCount: number
  accountingYear: string | null
  orgCode: string | null
  billingMonth: string | null
  expectedYear: number
  expectedOrgCode: string | null
  expectedBillingMonth: string | null
  rows: DreamPreviewRow[]
}

export interface DreamReset {
  result: { removedIdentities: number; removedKeys: number }
  /** 초기화 직전 자동백업 이름 */
  backup: string
}

// ─────────────────────────────────────────────── 방과후드림 매칭 · 검증 (P3)

export type DreamMatchState = 'READY' | 'NEEDS_CONFIRMATION' | 'ERROR'

/** 까닭 하나. `detail` 에는 부서 · 항목 · 금액 · 코드만(개인정보 없음) */
export interface DreamReason {
  code: string
  state: DreamMatchState
  row: number | null
  detail: string | null
}

export interface DreamSuggestion {
  departmentId: number
  label: string
  why: ('SAME_NAME' | 'PREVIOUS_WORKSPACE' | 'STUDENT_ENROLLED')[]
  /** 다른 강좌코드에 이미 연결된 부서 */
  taken: boolean
}

export interface DreamCourseMatch {
  courseCode: string
  courseNames: string[]
  departmentId: number | null
  deptLabel: string | null
  state: DreamMatchState
  reasons: DreamReason[]
  suggestions: DreamSuggestion[]
  /** [작업공간 이름, 부서 이름] */
  previous: [string, string] | null
  rows: number
}

export interface DreamLineMatch {
  sourceRow: number
  courseCode: string
  departmentId: number | null
  deptLabel: string | null
  itemCode: string | null
  itemName: string
  amountN: number
  /** 정산에 쓰인 Charge(행이 없으면 0). 견줄 수 없었으면 null */
  charge: number | null
  enrollmentId: number | null
  state: DreamMatchState
  reasons: DreamReason[]
}

export interface DreamStudentAmount {
  studentId: number
  total: number
  voucher: number
  freeVoucher: number
  personal: number
  selfPay: number
  voucherOver: number
}

export interface DreamStudentMatch {
  /** 첫 행 번호 — 확인 요청에 쓰는 열쇠 */
  key: number
  rows: number[]
  fileName: string
  fileGrade: string
  fileClass: string
  fileNo: string
  refMasked: string
  birthMasked: string
  studentId: number | null
  studentLabel: string | null
  refState: 'MATCHED' | 'NOT_REGISTERED' | 'CONFLICT' | 'UNAVAILABLE'
  birthState: 'MATCHED' | 'NOT_REGISTERED' | 'MISMATCH' | 'UNCHECKED'
  candidates: { studentId: number; label: string }[]
  lines: DreamLineMatch[]
  missing: { departmentId: number; deptLabel: string; itemName: string; amount: number }[]
  sumN: number
  /** 원본 [O, P, Q] 합 */
  before: [number, number, number]
  amount: DreamStudentAmount | null
  state: DreamMatchState
  reasons: DreamReason[]
}

export interface DreamMatching {
  settlementId: number | null
  fileReasons: DreamReason[]
  courses: DreamCourseMatch[]
  students: DreamStudentMatch[]
  departments: { id: number; label: string }[]
  ready: number
  needsConfirmation: number
  errors: number
  /** 최신 정산에 금액이 남았는데 파일에 없는 학생(취소 포함) */
  absent: DreamAbsentStudent[]
  /** 파일 ΣN */
  fileTotal: number
  /** 적용 학년 정산 T 합(학교 전체가 아니다) */
  settlementTotal: number
  /** 방과후드림 적용 학년(비어 있으면 미설정) */
  appliedGrades: number[]
  /** 검증 대상 학생 수 — 적용 학년에서 정산 금액이 남은 학생 */
  universeStudents: number
  /** 적용 학년이 아닌데 파일에 있는 학생 [학년, 학생 수] */
  outside: [number, number][]
  p4Ready: boolean
}

export interface DreamAbsentStudent {
  studentId: number
  label: string
  total: number
  cancelled: boolean
  depts: string[]
}

export interface DreamSession {
  jobId: number
  /** OPEN(확인 필요) · BLOCKED(오류) · READY(모두 준비 완료) · FAILED */
  jobStatus: string
  matching: DreamMatching
}

// ─────────────────────────────────────────────── 방과후드림 P4 — 우선순위 · 배분

/** DREAM(방과후드림 순위) · SETTLEMENT(기존 정산 순위를 기본값으로) · DEFAULT(정해진 차례) */
/**
 * DREAM(방과후드림 순위) · LEGACY(v0.1.11 부서별 차례가 모두 같아 그대로) · CONFLICT(부서마다
 * 달라 공통 차례를 저장해야 배분) · SETTLEMENT(기존 정산 순위를 기본값으로) · DEFAULT
 */
export type DreamPrioritySource = 'DREAM' | 'LEGACY' | 'CONFLICT' | 'SETTLEMENT' | 'DEFAULT'

export interface DreamPriorityDept {
  departmentId: number
  label: string
  inUse: boolean
}

export interface DreamPriorityView {
  deptSource: DreamPrioritySource
  /** 배분할 차례대로 */
  departments: DreamPriorityDept[]
  /** 공통 항목 차례 — 모든 부서에 같다 */
  items: { code: string; name: string }[]
  itemSource: DreamPrioritySource
  digest: string
}

/** n = 정산금액(T) · o = 개인부담 · p = 이용권 · q = 자유수강권 */
export interface DreamTotals {
  n: number
  o: number
  p: number
  q: number
}

export interface DreamAllocLine {
  sourceRow: number
  seq: number
  departmentId: number
  deptLabel: string
  itemCode: string
  itemName: string
  amountN: number
  /** 원본 O · P · Q */
  before: [number, number, number]
  /** 새 O · P · Q */
  after: [number, number, number]
}

export interface DreamAllocStudent {
  key: number
  studentId: number
  studentLabel: string
  amount: DreamStudentAmount
  before: DreamTotals
  after: DreamTotals
  lines: DreamAllocLine[]
  order: { label: string; items: string[] }[]
}

export interface DreamAllocIssue {
  code: string
  row: number | null
  detail: string | null
}

export interface DreamAllocation {
  jobId: number
  jobStatus: string
  /** DONE(배분 검증 완료) · NOT_READY · FAILED · STALE(다시 계산 필요) */
  status: 'DONE' | 'NOT_READY' | 'FAILED' | 'STALE'
  issues: DreamAllocIssue[]
  session: DreamSession | null
  students: DreamAllocStudent[]
  before: DreamTotals
  after: DreamTotals
  settlement: DreamTotals
  /** 이 결과가 다룬 방과후드림 적용 학년 */
  appliedGrades: number[]
  priorityDigest: string
  fingerprint: string | null
}

// ─────────────────────────────────────────────── 방과후드림 P5 — 결과 Excel

export interface DreamExport {
  written: {
    /** 저장한 파일 이름(경로 없음) */
    fileName: string
    /** [폴더 열기] 용 */
    path: string
    sha256: string
    patch: { sheetPart: string; entries: number; dataRows: number; changedCells: number; insertedCells: number }
    verify: {
      entries: number
      nonTargetSame: number
      dataRows: number
      sheetRows: number
      targetCellsChanged: number
      aToNChanged: number
    }
  }
  rows: number
  students: number
  totals: DreamTotals
  transferDate: string
  appliedGrades: number[]
  historySaved: boolean
  historyError: string | null
}
