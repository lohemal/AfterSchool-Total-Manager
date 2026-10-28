import { describe, expect, it } from 'vitest'

import type { DreamIssue } from '@/ipc/types'

import {
  cellText,
  DREAM_HEADERS,
  issueText,
  matchStateLabel,
  monthCode,
  monthLabel,
  reasonText,
  statusLabel,
} from './dream'

const issue = (o: Partial<DreamIssue>): DreamIssue => ({
  code: 'INVALID_CELL_TYPE',
  severity: 'ERROR',
  row: null,
  column: null,
  expected: null,
  found: null,
  ...o,
})

describe('방과후드림 문장', () => {
  it('행 · 열 · 내용을 한 줄로', () => {
    expect(issueText(issue({ row: 4, column: 'M', expected: 'TEXT' }))).toBe(
      'Excel 4행 · 항목구분코드(M): 글자로 저장되어야 하는데 숫자 등 다른 형식입니다(앞자리 0 이 사라졌을 수 있습니다).',
    )
    expect(issueText(issue({ code: 'EMPTY_DATA' }))).toBe(
      '처리할 결제내역이 없습니다. 머리글 아래에 결제행이 하나도 없습니다.',
    )
  })

  it('작업공간과 다른 값은 기대값과 함께', () => {
    expect(
      issueText(issue({ code: 'BILLING_MONTH_MISMATCH', expected: '202610', found: '202609' })),
    ).toBe('파일의 수강월(2026년 9월)이 이 작업공간의 결제 기준월(2026년 10월)과 다릅니다.')
    expect(issueText(issue({ code: 'ORG_CODE_MISMATCH', expected: '0024000001', found: '2024000001' }))).toContain(
      '0024000001',
    )
  })

  it('모르는 코드도 감추지 않는다', () => {
    expect(issueText(issue({ code: 'SOMETHING_NEW' }))).toBe('알 수 없는 문제(SOMETHING_NEW)')
  })

  it('머리글은 18열', () => {
    expect(DREAM_HEADERS).toHaveLength(18)
    expect(DREAM_HEADERS[14]).toBe('개인부담금')
    expect(DREAM_HEADERS[15]).toBe('이용권')
    expect(DREAM_HEADERS[16]).toBe('자유수강권')
  })

  it('월과 금액 표시', () => {
    expect(monthLabel('202609')).toBe('2026년 9월')
    expect(monthLabel(null)).toBe('미설정')
    expect(monthCode(2026, 9)).toBe('202609')
    expect(cellText(13, '35000')).toBe('35,000')
    expect(cellText(8, '202609')).toBe('202609') // 수강월은 숫자여도 그대로
    expect(cellText(12, '01')).toBe('01')
    expect(statusLabel('REVIEW')).toBe('확인 필요')
  })
})

describe('방과후드림 매칭 문장', () => {
  it('행 번호와 함께', () => {
    expect(
      reasonText({ code: 'CHARGE_AMOUNT_MISMATCH', state: 'ERROR', row: 2, detail: '파일 30000 · 정산 35000' }),
    ).toBe('Excel 2행: 정산금액이 통합 매니저 금액과 다릅니다(파일 30000 · 정산 35000).')
    expect(reasonText({ code: 'STUDENT_AMBIGUOUS', state: 'NEEDS_CONFIRMATION', row: null, detail: null })).toBe(
      '후보 학생이 여럿입니다. 어느 학생인지 골라 주세요.',
    )
    expect(reasonText({ code: 'NEW_CODE', state: 'ERROR', row: null, detail: null })).toBe('알 수 없는 문제(NEW_CODE)')
    expect(matchStateLabel('NEEDS_CONFIRMATION')).toBe('확인 필요')
  })
})
