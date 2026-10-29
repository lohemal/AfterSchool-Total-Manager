/**
 * 방과후드림 결제관리 — 화면 문장 (P2 검사 · P3 매칭 · P4 배분).
 *
 * 규칙은 Rust(`domain::dream_file`)에 있고, 화면은 그 결과의 **코드**를 사람이 읽는
 * 문장으로 바꾸기만 한다. 같은 코드면 언제나 같은 문장이 나온다.
 *
 * 개인참조번호 · 생년월일은 서버가 이미 가려서 보낸다. 문제(`DreamIssue`)의 `found` 에도
 * 그 값은 들어오지 않는다.
 */

import type {
  DreamAllocation,
  DreamAllocIssue,
  DreamExport,
  DreamIssue,
  DreamMatchState,
  DreamPrioritySource,
  DreamReason,
  DreamStatus,
} from '@/ipc/types'

/** 결제파일 A~R 머리글. 서버의 `HEADERS` 와 같은 차례. */
export const DREAM_HEADERS = [
  '회계년도',
  '기관코드',
  '개인참조번호',
  '학년',
  '반',
  '번호',
  '학생명',
  '생년월일',
  '수강월',
  '강좌명',
  '강좌명코드',
  '항목구분',
  '항목구분코드',
  '정산금액',
  '개인부담금',
  '이용권',
  '자유수강권',
  '이체예정일',
] as const

export const DREAM_LETTERS = 'ABCDEFGHIJKLMNOPQR'.split('')

/** 금액 칸(N · O · P · Q) — 천 단위로 보인다. 수강월(I)은 숫자여도 그대로. */
export const MONEY_COLS = new Set([13, 14, 15, 16])

export function statusLabel(s: DreamStatus): string {
  return s === 'OK' ? '정상' : s === 'REVIEW' ? '확인 필요' : '오류'
}

/** `202609` → `2026년 9월`. 형식이 틀리면 그대로 돌려준다. */
export function monthLabel(v: string | null | undefined): string {
  if (!v) return '미설정'
  const m = /^(\d{4})(\d{2})$/.exec(v)
  if (!m) return v
  return `${m[1]}년 ${Number(m[2])}월`
}

/** (2026, 9) → `202609` */
export function monthCode(year: number, month: number): string {
  return `${year}${String(month).padStart(2, '0')}`
}

function columnLabel(col: string | null): string {
  if (!col) return ''
  const i = DREAM_LETTERS.indexOf(col)
  return i >= 0 ? `${DREAM_HEADERS[i]}(${col})` : `${col}열`
}

const TEXT: Record<string, (i: DreamIssue) => string> = {
  // 파일 수준
  FILE_NOT_XLSX: () =>
    '방과후드림에서 내려받은 .xlsx 파일만 받습니다(xls · csv · ods 는 받지 않습니다).',
  FILE_TOO_LARGE: () => '파일이 너무 큽니다(20MB 초과). 방과후드림 결제파일이 맞는지 확인해 주세요.',
  FILE_READ_FAILED: () => '파일을 읽지 못했습니다. 다른 프로그램에서 열려 있지 않은지 확인해 주세요.',
  WORKBOOK_CORRUPT: () => 'Excel 파일로 열 수 없습니다. 파일이 손상되었거나 다른 형식입니다.',
  SHEET_MISSING: () => `'정산내역' 시트가 없습니다. 방과후드림 결제파일이 맞는지 확인해 주세요.`,
  EXTRA_SHEET: (i) =>
    `'정산내역' 말고 다른 시트(${i.found ?? ''})가 있습니다. 방과후드림에서 내려받은 그대로의 파일을 골라 주세요.`,
  HEADER_MISMATCH: (i) =>
    `1행 머리글이 다릅니다 — '${i.expected ?? ''}' 자리에 '${i.found || '(빈 칸)'}'. 열 차례를 바꾸거나 이름을 고친 파일은 받지 않습니다.`,
  EXTRA_DATA_COLUMN: (i) => `R열 오른쪽(${i.found ?? ''}열)에 값이 있습니다. 결제파일은 A~R 18열입니다.`,
  EMPTY_DATA: () => '처리할 결제내역이 없습니다. 머리글 아래에 결제행이 하나도 없습니다.',
  INTERNAL_EMPTY_ROW: () => '결제행 사이에 빈 행이 있습니다. 빈 행을 건너뛰어 이어 붙이지 않습니다.',
  MIXED_ACCOUNTING_YEAR: (i) => `회계년도가 줄마다 다릅니다(${i.found ?? ''}). 한 파일은 한 회계년도여야 합니다.`,
  MIXED_ORG_CODE: (i) => `기관코드가 줄마다 다릅니다(${i.found ?? ''}).`,
  MIXED_BILLING_MONTH: (i) => `수강월이 줄마다 다릅니다(${i.found ?? ''}). 한 파일은 한 달이어야 합니다.`,
  ACCOUNTING_YEAR_MISMATCH: (i) =>
    `회계년도 ${i.found ?? ''}는 이 작업공간의 학년도(${i.expected ?? ''})와 다릅니다.`,
  ORG_CODE_NOT_SET: () => '기관코드를 먼저 저장해 주세요(기본정보).',
  ORG_CODE_MISMATCH: (i) => `파일의 기관코드 ${i.found ?? ''}가 저장된 기관코드 ${i.expected ?? ''}와 다릅니다.`,
  BILLING_MONTH_NOT_SET: () => '이 작업공간의 결제 기준월을 먼저 저장해 주세요(기본정보).',
  BILLING_MONTH_MISMATCH: (i) =>
    `파일의 수강월(${monthLabel(i.found)})이 이 작업공간의 결제 기준월(${monthLabel(i.expected)})과 다릅니다.`,
  // 행 수준
  REQUIRED_VALUE_MISSING: () => '비어 있습니다.',
  INVALID_CELL_TYPE: (i) =>
    i.expected === 'TEXT'
      ? '글자로 저장되어야 하는데 숫자 등 다른 형식입니다(앞자리 0 이 사라졌을 수 있습니다).'
      : '숫자로 저장되어야 하는데 글자 등 다른 형식입니다.',
  INVALID_ACCOUNTING_YEAR: () => '네 자리 연도(예: 2026)가 아닙니다.',
  INVALID_ORG_CODE: () => '영문 · 숫자로 된 기관코드가 아닙니다.',
  INVALID_PERSONAL_REF: () => '영문 · 숫자로 된 개인참조번호가 아닙니다.',
  INVALID_GRADE: () => "학년을 알 수 없습니다(예: '3학년').",
  INVALID_CLASS: () => '반 이름이 너무 깁니다.',
  INVALID_STUDENT_NO: () => '숫자로 된 번호가 아닙니다.',
  INVALID_BIRTHDATE: () => 'YYMMDD 여섯 자리의 실제 날짜가 아닙니다.',
  INVALID_BILLING_MONTH: (i) => `YYYYMM 여섯 자리 수강월이 아닙니다(${i.found ?? ''}).`,
  INVALID_COURSE_CODE: () => '영문 · 숫자로 된 강좌명코드가 아닙니다.',
  INVALID_ITEM_PAIR: (i) =>
    `항목구분과 항목구분코드가 맞지 않습니다(${i.found ?? ''}). 강사료/01 · 수용비/02 · 교재비/03 · 재료비/04 만 받습니다.`,
  INVALID_AMOUNT: (i) => `0 이상의 정수 원이어야 합니다(${i.found ?? ''}).`,
  INVALID_EXISTING_ALLOCATION: (i) =>
    i.column === 'N'
      ? '정산금액이 0원인데 개인부담금 · 이용권 · 자유수강권에 금액이 있습니다.'
      : `0 이상의 정수 원이어야 합니다(${i.found ?? ''}).`,
  INVALID_TRANSFER_DATE: (i) => `YYYYMMDD 여덟 자리의 실제 날짜가 아닙니다(${i.found ?? ''}).`,
  // 참고
  EXISTING_SUM_MISMATCH: () =>
    '원본의 개인부담금 + 이용권 + 자유수강권이 정산금액과 다릅니다(참고 — 나중에 새로 계산합니다).',
}

/** `Excel 4행 · 항목구분코드(M): …` 꼴의 한 줄. */
export function issueText(i: DreamIssue): string {
  const where = [i.row ? `Excel ${i.row}행` : '', columnLabel(i.column)].filter(Boolean).join(' · ')
  const body = TEXT[i.code]?.(i) ?? `알 수 없는 문제(${i.code})`
  return where ? `${where}: ${body}` : body
}

/** 미리보기 칸 글자 — 금액은 천 단위, 나머지는 원래대로. */
export function cellText(col: number, v: string): string {
  if (MONEY_COLS.has(col) && /^\d+$/.test(v)) return Number(v).toLocaleString('ko-KR')
  return v
}

// ─────────────────────────────────────────────── 매칭 · 검증 (P3)

export function matchStateLabel(s: DreamMatchState): string {
  return s === 'READY' ? '준비 완료' : s === 'NEEDS_CONFIRMATION' ? '확인 필요' : '오류'
}

const REASON: Record<string, (r: DreamReason) => string> = {
  SETTLEMENT_NOT_FRESH: (r) => `정산이 최신이 아닙니다. ${r.detail ?? ''}`,
  BUDGET_CROSSCHECK_FAILED: () => '정산 결과와 지원금 기록이 맞지 않습니다. 정산을 다시 만들어 주세요.',
  SETTLEMENT_STUDENT_NOT_IN_FILE: (r) =>
    `최신 정산에 금액이 남아 있지만 결제파일에 없는 학생이 있습니다(${r.detail ?? ''}). 취소한 학생도 남은 금액이 있으면 파일에 있어야 합니다.`,
  FILE_TOTAL_MISMATCH: (r) => `파일 전체 정산금액 합계가 최신 정산 전체와 다릅니다(${r.detail ?? ''}).`,
  SETTLEMENT_CHARGE_TOTAL_MISMATCH: (r) =>
    `정산 전체 금액이 Charge 합계와 다릅니다(${r.detail ?? ''}). 정산을 다시 만들어 주세요.`,
  STUDENT_INFO_INCONSISTENT: () => '같은 개인참조번호의 줄들끼리 학년 · 반 · 번호 · 이름 · 생년월일이 서로 다릅니다.',
  STUDENT_REF_NOT_REGISTERED: () => '개인참조번호가 아직 등록되지 않았습니다. 후보 학생이 맞는지 확인해 주세요.',
  STUDENT_REF_CONFLICT: () => '이 학생에게는 이미 다른 개인참조번호가 등록되어 있습니다. 자동으로 옮기지 않습니다.',
  STUDENT_NOT_FOUND: () => '학년 · 반 · 번호 · 이름이 모두 같은 학생이 없습니다.',
  STUDENT_AMBIGUOUS: () => '후보 학생이 여럿입니다. 어느 학생인지 골라 주세요.',
  STUDENT_DUPLICATE_IN_FILE: () => '같은 학생이 파일에 다른 개인참조번호로 두 번 나옵니다.',
  STUDENT_GRADE_MISMATCH: () => '개인참조번호로 찾은 학생과 학년이 다릅니다.',
  STUDENT_CLASS_MISMATCH: () => '개인참조번호로 찾은 학생과 반이 다릅니다.',
  STUDENT_NO_MISMATCH: () => '개인참조번호로 찾은 학생과 번호가 다릅니다.',
  STUDENT_NAME_MISMATCH: () => '개인참조번호로 찾은 학생과 이름이 다릅니다.',
  STUDENT_BIRTH_MISMATCH: () => '저장된 생년월일과 파일의 생년월일이 다릅니다.',
  STUDENT_BIRTH_NOT_REGISTERED: () => '생년월일이 아직 등록되지 않았습니다. 확인 후 등록해 주세요.',
  STUDENT_NOT_CONFIRMED: () => '학생이 정해지지 않아 금액을 맞대어 보지 못했습니다.',
  COURSE_MAP_MISSING: () => '강좌명코드가 아직 부서에 연결되지 않았습니다.',
  COURSE_NAME_DIFFERS: (r) => `강좌명 표기가 연결된 부서 이름과 다릅니다(${r.detail ?? ''}). 참고만 — 확정한 강좌코드 연결을 따릅니다.`,
  COURSE_NAME_INCONSISTENT: (r) => `같은 강좌명코드에 강좌명이 여럿입니다(${r.detail ?? ''}).`,
  ENROLLMENT_NOT_FOUND: () => '이 학생은 이 부서를 수강하지 않습니다.',
  ENROLLMENT_AMBIGUOUS: () => '이 부서에 정산된 수강이 여럿이라 하나를 고를 수 없습니다.',
  ITEM_UNKNOWN: () => '알 수 없는 비용항목입니다.',
  DUPLICATE_PAYMENT_ROW: (r) => `같은 학생 · 부서 · 항목이 두 줄입니다(${r.detail ?? ''}). 더하지 않습니다.`,
  CHARGE_MISSING: (r) => `정산에 이 항목 금액이 없습니다(${r.detail ?? ''}).`,
  CHARGE_AMOUNT_MISMATCH: (r) => `정산금액이 통합 매니저 금액과 다릅니다(${r.detail ?? ''}).`,
  DB_CHARGE_NOT_IN_FILE: (r) => `정산에 있는 금액이 파일에 없습니다(${r.detail ?? ''}).`,
  STUDENT_TOTAL_MISMATCH: (r) => `학생의 파일 합계가 정산 총액과 다릅니다(${r.detail ?? ''}).`,
  CHARGE_SETTLEMENT_MISMATCH: (r) => `Charge 합계가 정산 총액과 다릅니다(${r.detail ?? ''}). 정산을 다시 만들어 주세요.`,
}

export function reasonText(r: DreamReason): string {
  const body = REASON[r.code]?.(r) ?? `알 수 없는 문제(${r.code})`
  return r.row ? `Excel ${r.row}행: ${body}` : body
}

// ─────────────────────────────────────────────── 배분 (P4)

const ALLOC: Record<string, (i: DreamAllocIssue) => string> = {
  P3_NOT_READY: () => '매칭 및 검증이 모두 준비 완료가 아니어서 배분하지 않았습니다. 위의 문제를 먼저 해결해 주세요.',
  ALLOCATION_STALE: () => '저장된 배분 결과가 지금 자료 · 우선순위와 맞지 않습니다. [배분 계산]을 다시 해 주세요.',
  ALLOC_INTERNAL_INCONSISTENT: () => '검증을 마친 자료에 배분에 필요한 값이 없습니다(내부 오류).',
  ALLOC_SETTLEMENT_STUDENT_MISSING: (i) => `정산에 금액이 남은 학생 ${i.detail ?? ''}명이 배분 대상에 없습니다.`,
  ALLOC_NEGATIVE_AMOUNT: () => '음수 금액이 있어 배분하지 않았습니다.',
  ALLOC_TARGET_INCONSISTENT: () => '정산 총액이 이용권 + 자유수강권 + 개인부담과 다릅니다.',
  ALLOC_TOTAL_MISMATCH: () => '학생의 파일 합계가 정산 총액과 다릅니다.',
  ALLOC_DUPLICATE_ROW: () => '같은 행이 두 번 들어 있습니다.',
  ALLOC_VOUCHER_LEFT: () => '이용권 지원액을 파일 금액 안에 모두 담지 못했습니다.',
  ALLOC_FREE_LEFT: () => '자유수강권 지원액을 파일 금액 안에 모두 담지 못했습니다.',
  ALLOC_ROW_SET_MISMATCH: () => '배분 결과의 줄이 파일 줄과 다릅니다.',
  ALLOC_ROW_SUM_MISMATCH: () => '어떤 줄에서 정산금액 ≠ 개인부담 + 이용권 + 자유수강권입니다.',
  ALLOC_VOUCHER_SUM_MISMATCH: () => '이용권 합계가 정산 이용권과 다릅니다.',
  ALLOC_FREE_SUM_MISMATCH: () => '자유수강권 합계가 정산 자유수강권과 다릅니다.',
  ALLOC_PERSONAL_SUM_MISMATCH: () => '개인부담 합계가 정산 개인부담과 다릅니다.',
  ALLOC_FILE_SUM_MISMATCH: (i) => `파일 전체 배분 합계가 정산 전체와 다릅니다(${i.detail ?? ''}).`,
}

export function allocIssueText(i: DreamAllocIssue): string {
  const body = ALLOC[i.code]?.(i) ?? `알 수 없는 문제(${i.code})`
  return i.row ? `Excel ${i.row}행 학생: ${body}` : body
}

export function prioritySourceLabel(s: DreamPrioritySource): string {
  return s === 'DREAM' ? '방과후드림 순위' : s === 'SETTLEMENT' ? '기존 정산 순위(기본값)' : '정해진 차례(기본값)'
}

/** 목록에서 한 칸 위(-1) · 아래(+1)로. 끝이면 그대로. */
export function move<T>(list: T[], index: number, by: -1 | 1): T[] {
  const to = index + by
  if (to < 0 || to >= list.length) return list
  const out = list.slice()
  ;[out[index], out[to]] = [out[to], out[index]]
  return out
}

/**
 * 이 화면에서 만든 결과 파일. `current` 는 만든 뒤로 배분이 무효가 되거나 다시 계산된 적이
 * 없는가 — 한 번 내려가면 다시 올라가지 않는다(배분을 다시 계산했다고 예전 파일이 지금 결과가
 * 되지 않는다). 처리 이력(DB)은 건드리지 않는 화면 상태다.
 */
export interface MadeExport {
  result: DreamExport
  /** 만들 때의 배분 지문 */
  fingerprint: string | null
  madeAt: string
  current: boolean
}

/** `CURRENT` 지금 결과 · `PREVIOUS` 배분이 바뀐 뒤의 이전 결과 · `OTHER_DATE` 이체예정일만 다름 */
export type ExportView = 'NONE' | 'CURRENT' | 'PREVIOUS' | 'OTHER_DATE'

/** 배분이 무효가 되거나 다시 계산되면 — 이전 결과 파일을 '이전 생성 결과'로 내린다. */
export function demoteExport(m: MadeExport | null): MadeExport | null {
  return m && m.current ? { ...m, current: false } : m
}

export function exportView(
  m: MadeExport | null,
  alloc: Pick<DreamAllocation, 'status' | 'fingerprint'> | null,
  transferDate: string | null,
): ExportView {
  if (!m) return 'NONE'
  if (!m.current || alloc?.status !== 'DONE' || alloc.fingerprint !== m.fingerprint) return 'PREVIOUS'
  if (transferDate !== m.result.transferDate) return 'OTHER_DATE'
  return 'CURRENT'
}

/**
 * 결과 파일 기본 이름 — 결제 기준월 · 이체예정일만(학생 정보 없음).
 * 예: `방과후드림_결제결과_202610_20261030.xlsx`
 */
export function resultFileName(billingMonth: string | null, transferDate: string): string {
  return ['방과후드림_결제결과', billingMonth, transferDate].filter(Boolean).join('_') + '.xlsx'
}

/** 날짜 선택기 값(`YYYY-MM-DD`) → 결과 파일 R열 값(`YYYYMMDD`). 실제 날짜가 아니면 null. */
export function transferDateCode(v: string): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v)
  if (!m) return null
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])]
  const dt = new Date(Date.UTC(y, mo - 1, d))
  if (dt.getUTCFullYear() !== y || dt.getUTCMonth() !== mo - 1 || dt.getUTCDate() !== d) return null
  return `${m[1]}${m[2]}${m[3]}`
}
