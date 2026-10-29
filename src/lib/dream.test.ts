import { describe, expect, it } from 'vitest'

import type { DreamCourseMatch, DreamExport, DreamIssue, DreamSuggestion } from '@/ipc/types'

import {
  allocIssueText,
  cellText,
  demoteExport,
  DREAM_HEADERS,
  exportView,
  gradesLabel,
  sameGrades,
  defaultCoursePick,
  coursePicksToSave,
  issueText,
  matchStateLabel,
  monthCode,
  monthLabel,
  move,
  prioritySourceLabel,
  reasonText,
  resultFileName,
  statusLabel,
  transferDateCode,
} from './dream'
import type { MadeExport } from './dream'

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

  it('결과 파일 이름에는 기준월 · 이체예정일만', () => {
    expect(resultFileName('202610', '20261030')).toBe('방과후드림_결제결과_202610_20261030.xlsx')
    expect(resultFileName(null, '20261030')).toBe('방과후드림_결제결과_20261030.xlsx')
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

describe('결과 파일 표시 — 지금 결과인가, 이전 결과인가', () => {
  const made = (fp: string, date: string): MadeExport => ({
    result: { transferDate: date, written: { fileName: '결과.xlsx' } } as unknown as DreamExport,
    fingerprint: fp,
    madeAt: '2026. 10. 29.',
    current: true,
  })
  const done = (fp: string) => ({ status: 'DONE' as const, fingerprint: fp })

  it('A — 배분 완료 뒤 만든 파일은 지금 결과', () => {
    expect(exportView(made('f1', '20261030'), done('f1'), '20261030')).toBe('CURRENT')
    expect(exportView(null, done('f1'), '20261030')).toBe('NONE')
  })

  it('B — 우선순위를 바꿔 배분이 무효가 되면 이전 결과', () => {
    const m = demoteExport(made('f1', '20261030'))
    expect(exportView(m, null, '20261030')).toBe('PREVIOUS')
    // 무효가 된 배분(STALE)이나 지문이 다른 배분이어도
    expect(exportView(made('f1', '20261030'), { status: 'STALE', fingerprint: 'f1' }, '20261030')).toBe('PREVIOUS')
    expect(exportView(made('f1', '20261030'), done('f2'), '20261030')).toBe('PREVIOUS')
  })

  it('C — 다시 계산만 하면(지문이 같아도) 이전 결과가 지금 결과로 돌아오지 않는다', () => {
    let m: MadeExport | null = made('f1', '20261030')
    m = demoteExport(m) // 우선순위 바뀜
    m = demoteExport(m) // 다시 계산
    expect(m?.current).toBe(false)
    expect(exportView(m, done('f1'), '20261030')).toBe('PREVIOUS')
  })

  it('D — 새로 만들면 새 파일이 지금 결과', () => {
    const fresh = made('f2', '20261105')
    expect(exportView(fresh, done('f2'), '20261105')).toBe('CURRENT')
  })

  it('E — 이체예정일만 바꾸면 배분은 그대로지만 그 날짜의 결과는 아직 없다', () => {
    const m = made('f1', '20261030')
    expect(exportView(m, done('f1'), '20261105')).toBe('OTHER_DATE')
    expect(exportView(m, done('f1'), null)).toBe('OTHER_DATE')
    // 날짜를 되돌리면 배분이 그대로이므로 그 파일이 다시 지금 결과다
    expect(exportView(m, done('f1'), '20261030')).toBe('CURRENT')
    expect(demoteExport(null)).toBeNull()
  })
})

describe('적용 학년 · 강좌코드 일괄 연결 (P6)', () => {
  it('학년 표시와 집합 비교', () => {
    expect(gradesLabel([4, 3])).toBe('3학년 · 4학년')
    expect(gradesLabel([])).toBe('미설정')
    expect(sameGrades([3, 4], [4, 3, 3])).toBe(true)
    expect(sameGrades([3], [3, 4])).toBe(false)
  })

  it('적용 학년 문장', () => {
    expect(reasonText({ code: 'APPLIED_GRADES_NOT_SET', state: 'ERROR', row: null, detail: null })).toBe(
      '방과후드림 적용 학년을 1개 이상 선택해 주세요.',
    )
    expect(
      reasonText({ code: 'FILE_STUDENT_OUTSIDE_APPLIED_GRADES', state: 'ERROR', row: null, detail: '4학년 1명' }),
    ).toBe('선택한 적용 학년에 포함되지 않는 학생이 결제파일에 있습니다(4학년 1명). 적용 학년 또는 결제파일을 확인해 주세요.')
    expect(allocIssueText({ code: 'ITEM_ORDER_NOT_CONFIRMED', row: null, detail: null })).toContain('[항목 차례 저장]')
  })

  const course = (o: Partial<DreamCourseMatch>): DreamCourseMatch => ({
    courseCode: 'C1',
    courseNames: ['로봇과학A'],
    departmentId: null,
    deptLabel: null,
    state: 'NEEDS_CONFIRMATION',
    reasons: [],
    suggestions: [],
    previous: null,
    rows: 4,
    ...o,
  })
  const sug = (id: number, why: DreamSuggestion['why'], taken = false): DreamSuggestion => ({
    departmentId: id,
    label: `부서${id}`,
    why,
    taken,
  })

  it('기본 선택은 확실한 추천이 하나일 때만', () => {
    expect(defaultCoursePick(course({ departmentId: 7 }))).toBe(7)
    expect(defaultCoursePick(course({ suggestions: [sug(1, ['SAME_NAME', 'STUDENT_ENROLLED'])] }))).toBe(1)
    expect(defaultCoursePick(course({ suggestions: [sug(2, ['PREVIOUS_WORKSPACE'])] }))).toBe(2)
    // 학생이 듣는 부서라는 근거만으로는 고르지 않는다
    expect(defaultCoursePick(course({ suggestions: [sug(3, ['STUDENT_ENROLLED'])] }))).toBeNull()
    // 둘이면 고르지 않는다, 다른 코드에 이어진 부서도
    expect(defaultCoursePick(course({ suggestions: [sug(1, ['SAME_NAME']), sug(2, ['SAME_NAME'])] }))).toBeNull()
    expect(defaultCoursePick(course({ suggestions: [sug(1, ['SAME_NAME'], true)] }))).toBeNull()
  })

  it('저장할 것은 지금 연결과 다른 행만', () => {
    const cs = [
      course({ courseCode: 'A', suggestions: [sug(1, ['SAME_NAME'])] }),
      course({ courseCode: 'B', departmentId: 2 }),
      course({ courseCode: 'C', suggestions: [sug(3, ['STUDENT_ENROLLED'])] }),
      course({ courseCode: 'D', departmentId: 4 }),
    ]
    // A 는 기본값, B 는 그대로, C 는 직접 고름, D 는 다른 부서로 바꿈
    expect(coursePicksToSave(cs, { C: 3, D: 5 })).toEqual([
      ['A', 1],
      ['C', 3],
      ['D', 5],
    ])
    // A 를 '부서 선택'으로 되돌리면 빠진다
    expect(coursePicksToSave(cs, { A: null })).toEqual([])
  })
})
