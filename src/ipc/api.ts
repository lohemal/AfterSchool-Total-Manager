/**
 * Rust 명령 호출 묶음. 화면에서는 `invoke('...')`를 직접 쓰지 않고 여기만 쓴다.
 * 명령 이름의 오타가 화면 코드로 퍼지지 않게 하려는 것이다.
 */

import { invoke } from '@tauri-apps/api/core'

import type {
  ApplyMode,
  ApplyResult,
  Bootstrap,
  ChangeLog,
  Department,
  DepartmentInput,
  Eligibility,
  EligibilityInput,
  EligibilityView,
  Enrollment,
  EnrollmentFilter,
  EnrollmentInput,
  ExportResult,
  Fee,
  FeeDiffView,
  FeePick,
  ImportKind,
  ImportPreview,
  ImportResult,
  Period,
  PolicyView,
  ProgramCode,
  RowIssue,
  Student,
  StudentDetail,
  StudentFeeEdit,
  StudentFilter,
  StudentInput,
  Workspace,
  WorkspaceInput,
  Year,
} from './types'

/** Rust `AppError`. 화면에는 `message`만 보여준다. */
export interface AppError {
  code: string
  message: string
  detail?: string | null
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === 'object' && 'message' in e) {
    return String((e as AppError).message)
  }
  if (e instanceof Error) return e.message
  return '알 수 없는 오류가 발생했습니다.'
}

export const api = {
  bootstrap: () => invoke<Bootstrap>('bootstrap'),
  openFolder: (which: 'data' | 'exports' | 'backups') =>
    invoke<void>('open_folder', { which }),
  getSetting: (key: string) => invoke<string | null>('get_setting', { key }),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),

  // 학년도
  yearList: () => invoke<Year[]>('year_list'),
  yearCreate: (yearNo: number, name: string) => invoke<number>('year_create', { yearNo, name }),
  yearUpdate: (id: number, name: string, startDate: string, endDate: string) =>
    invoke<void>('year_update', { id, name, startDate, endDate }),
  yearSetCurrent: (id: number) => invoke<void>('year_set_current', { id }),
  yearDelete: (id: number) => invoke<void>('year_delete', { id }),

  // 작업공간
  workspaceList: (yearId: number) => invoke<Workspace[]>('workspace_list', { yearId }),
  workspaceCreate: (yearId: number, input: WorkspaceInput) =>
    invoke<number>('workspace_create', { yearId, input }),
  workspaceUpdate: (id: number, input: WorkspaceInput) =>
    invoke<void>('workspace_update', { id, input }),
  workspaceDelete: (id: number) => invoke<void>('workspace_delete', { id }),
  workspaceSetCurrent: (id: number) => invoke<void>('workspace_set_current', { id }),
  /** 기간이 겹치는 다른 작업공간. 저장을 막지 않고 확인만 받는다. */
  workspaceOverlaps: (
    yearId: number,
    excludeId: number | null,
    startDate: string,
    endDate: string,
  ) => invoke<Workspace[]>('workspace_overlaps', { yearId, excludeId, startDate, endDate }),

  // 학생정보
  studentList: (yearId: number, filter: StudentFilter) =>
    invoke<Student[]>('student_list', { yearId, filter }),
  studentCreate: (yearId: number, input: StudentInput) =>
    invoke<number>('student_create', { yearId, input }),
  studentUpdate: (id: number, input: StudentInput) => invoke<void>('student_update', { id, input }),
  studentDelete: (ids: number[]) => invoke<number>('student_delete', { ids }),
  studentDeleteAll: (yearId: number) => invoke<number>('student_delete_all', { yearId }),

  // 지원대상자
  eligibilityList: (yearId: number, program: ProgramCode, query?: string) =>
    invoke<EligibilityView>('eligibility_list', { yearId, program, query: query ?? null }),
  eligibilityCreate: (yearId: number, input: EligibilityInput) =>
    invoke<number>('eligibility_create', { yearId, input }),
  eligibilityUpdate: (id: number, input: EligibilityInput) =>
    invoke<void>('eligibility_update', { id, input }),
  eligibilityDelete: (ids: number[]) => invoke<number>('eligibility_delete', { ids }),
  eligibilityDeleteAll: (yearId: number, program: ProgramCode) =>
    invoke<number>('eligibility_delete_all', { yearId, program }),

  // 부서정보
  departmentList: (workspaceId: number, query?: string) =>
    invoke<Department[]>('department_list', { workspaceId, query: query ?? null }),
  departmentCreate: (workspaceId: number, input: DepartmentInput) =>
    invoke<number>('department_create', { workspaceId, input }),
  departmentUpdate: (id: number, input: DepartmentInput) =>
    invoke<void>('department_update', { id, input }),
  departmentDelete: (ids: number[]) => invoke<number>('department_delete', { ids }),
  departmentDeleteAll: (workspaceId: number) =>
    invoke<number>('department_delete_all', { workspaceId }),

  // 지원금 정책 (설정 화면은 Phase 3)
  policyList: (yearId: number) => invoke<PolicyView[]>('policy_list', { yearId }),
  policySave: (
    yearId: number,
    program: ProgramCode,
    annualLimit: number,
    carryover: boolean,
    targetGrades: string,
    periods: Period[],
  ) =>
    invoke<void>('policy_save', {
      yearId,
      program,
      annualLimit,
      carryover,
      targetGrades,
      periods,
    }),

  // 수강 (Phase 2)
  enrollmentList: (workspaceId: number, filter: EnrollmentFilter) =>
    invoke<Enrollment[]>('enrollment_list', { workspaceId, filter }),
  enrollmentGet: (id: number) => invoke<Enrollment>('enrollment_get', { id }),
  enrollmentByDepartment: (workspaceId: number, departmentId: number) =>
    invoke<Enrollment[]>('enrollment_by_department', { workspaceId, departmentId }),
  departmentBaseFees: (departmentId: number) =>
    invoke<Fee[]>('department_base_fees', { departmentId }),
  enrollmentCreate: (workspaceId: number, input: EnrollmentInput) =>
    invoke<number>('enrollment_create', { workspaceId, input }),
  enrollmentUpdateFees: (id: number, fees: Fee[], reason: string) =>
    invoke<void>('enrollment_update_fees', { id, fees, reason }),
  enrollmentCancel: (id: number, reason: string) =>
    invoke<void>('enrollment_cancel', { id, reason }),
  enrollmentRestore: (id: number, reason: string) =>
    invoke<void>('enrollment_restore', { id, reason }),
  enrollmentSaveStudentFees: (edits: StudentFeeEdit[], reason: string) =>
    invoke<number>('enrollment_save_student_fees', { edits, reason }),
  enrollmentFeeDiff: (workspaceId: number, departmentId: number | null) =>
    invoke<FeeDiffView>('enrollment_fee_diff', { workspaceId, departmentId }),
  enrollmentApplyFees: (args: {
    workspaceId: number
    departmentId: number | null
    mode: ApplyMode
    picks: FeePick[]
    reason: string
  }) => invoke<ApplyResult>('enrollment_apply_fees', args),

  // 변경이력 · 학생 상세정보
  changeLogList: (args: {
    yearId: number
    workspaceId?: number | null
    studentId?: number | null
    kind?: string | null
    limit?: number
  }) =>
    invoke<ChangeLog[]>('change_log_list', {
      yearId: args.yearId,
      workspaceId: args.workspaceId ?? null,
      studentId: args.studentId ?? null,
      kind: args.kind ?? null,
      limit: args.limit ?? 500,
    }),
  studentDetail: (yearId: number, studentId: number) =>
    invoke<StudentDetail>('student_detail', { yearId, studentId }),

  // Excel
  excelTemplate: (kind: ImportKind) => invoke<ExportResult>('excel_template', { kind }),
  excelPreview: (args: {
    kind: ImportKind
    path: string
    yearId: number
    workspaceId?: number | null
    program?: ProgramCode | null
  }) =>
    invoke<ImportPreview>('excel_preview', {
      kind: args.kind,
      path: args.path,
      yearId: args.yearId,
      workspaceId: args.workspaceId ?? null,
      program: args.program ?? null,
    }),
  excelCommit: (token: string, yearId: number, workspaceId?: number | null) =>
    invoke<ImportResult>('excel_commit', { token, yearId, workspaceId: workspaceId ?? null }),
  excelExport: (args: {
    kind: ImportKind
    yearId: number
    workspaceId?: number | null
    program?: ProgramCode | null
    filter?: StudentFilter | null
    enrollmentFilter?: EnrollmentFilter | null
  }) =>
    invoke<ExportResult>('excel_export', {
      kind: args.kind,
      yearId: args.yearId,
      workspaceId: args.workspaceId ?? null,
      program: args.program ?? null,
      filter: args.filter ?? null,
      enrollmentFilter: args.enrollmentFilter ?? null,
    }),
  excelExportIssues: (issues: RowIssue[], headers: string[]) =>
    invoke<ExportResult>('excel_export_issues', { issues, headers }),
}

export type { Eligibility, Student, Department, Workspace, Year }
