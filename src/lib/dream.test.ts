import { describe, expect, it } from 'vitest'

import type { DreamIssue } from '@/ipc/types'

import {
  allocIssueText,
  cellText,
  DREAM_HEADERS,
  issueText,
  matchStateLabel,
  monthCode,
  monthLabel,
  move,
  prioritySourceLabel,
  reasonText,
  statusLabel,
  transferDateCode,
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

describe('방과후드림 배분 (P4)', () => {
  it('이체예정일은 실제 날짜만 YYYYMMDD 로', () => {
    expect(transferDateCode('2026-10-28')).toBe('20261028')
    expect(transferDateCode('2028-02-29')).toBe('20280229')
    expect(transferDateCode('2026-02-30')).toBeNull()
    expect(transferDateCode('2026-13-01')).toBeNull()
    expect(transferDateCode('')).toBeNull()
    expect(transferDateCode('20261028')).toBeNull()
  })

  it('차례는 한 칸씩 옮기고 끝에서는 그대로', () => {
    expect(move([1, 2, 3], 1, -1)).toEqual([2, 1, 3])
    expect(move([1, 2, 3], 1, 1)).toEqual([1, 3, 2])
    expect(move([1, 2, 3], 0, -1)).toEqual([1, 2, 3])
    expect(move([1, 2, 3], 2, 1)).toEqual([1, 2, 3])
    const src = [1, 2]
    move(src, 0, 1)
    expect(src).toEqual([1, 2])
  })

  it('정책이 바뀐 문장 — 강좌명은 참고, 파일에 없는 학생은 오류', () => {
    expect(reasonText({ code: 'COURSE_NAME_DIFFERS', state: 'READY', row: null, detail: '로봇과학 B · 로봇과학B' })).toContain(
      '참고만',
    )
    expect(
      reasonText({ code: 'SETTLEMENT_STUDENT_NOT_IN_FILE', state: 'ERROR', row: null, detail: '1명 · 20000원' }),
    ).toContain('최신 정산에 금액이 남아 있지만 결제파일에 없는 학생')
    expect(allocIssueText({ code: 'ALLOCATION_STALE', row: null, detail: null })).toContain('[배분 계산]')
    expect(allocIssueText({ code: 'ALLOC_TOTAL_MISMATCH', row: 5, detail: null })).toBe(
      'Excel 5행 학생: 학생의 파일 합계가 정산 총액과 다릅니다.',
    )
    expect(allocIssueText({ code: 'X', row: null, detail: null })).toBe('알 수 없는 문제(X)')
    expect(prioritySourceLabel('SETTLEMENT')).toBe('기존 정산 순위(기본값)')
  })
})
