/**
 * 방과후드림 결제관리 — 화면 문장 (P2).
 *
 * 규칙은 Rust(`domain::dream_file`)에 있고, 화면은 그 결과의 **코드**를 사람이 읽는
 * 문장으로 바꾸기만 한다. 같은 코드면 언제나 같은 문장이 나온다.
 *
 * 개인참조번호 · 생년월일은 서버가 이미 가려서 보낸다. 문제(`DreamIssue`)의 `found` 에도
 * 그 값은 들어오지 않는다.
 */

import type { DreamIssue, DreamStatus } from '@/ipc/types'

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
