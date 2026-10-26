import { describe, expect, it } from 'vitest'

import type { Fee, RevertPreview, RevertRow } from '@/ipc/types'

import { batchRevertSentence, deltaOf, deltaSentence, revertSentence, revertShift, sumDeltas } from './delta'

const won = (n: number) => n.toLocaleString('ko-KR')
const f = (amount: number, code = 'INSTRUCTOR'): Fee => ({ itemCode: code, amount })
const CODES = ['INSTRUCTOR', 'MATERIAL']

describe('deltaOf — 금액이 줄면 환불, 늘면 추가징수', () => {
  it('35,000 → 30,000 은 환불 5,000', () => {
    const s = deltaOf([f(35_000)], [f(30_000)], CODES)
    expect(s.refund).toBe(5_000)
    expect(s.additional).toBe(0)
    expect(s.changed).toBe(true)
  })

  it('35,000 → 40,000 은 추가징수 5,000', () => {
    const s = deltaOf([f(35_000)], [f(40_000)], CODES)
    expect(s.refund).toBe(0)
    expect(s.additional).toBe(5_000)
  })

  it('같으면 차액이 없다', () => {
    const s = deltaOf([f(35_000)], [f(35_000)], CODES)
    expect(s.changed).toBe(false)
    expect(deltaSentence(s, won)).toBe('금액 차이가 없습니다.')
  })

  it('항목이 엇갈리면 환불과 추가징수가 따로 셈된다 — 서로 상쇄하지 않는다', () => {
    const s = deltaOf(
      [f(35_000), f(10_000, 'MATERIAL')],
      [f(30_000), f(13_000, 'MATERIAL')],
      CODES,
    )
    expect(s.refund).toBe(5_000)
    expect(s.additional).toBe(3_000)
    expect(deltaSentence(s, won)).toBe('5,000원 환불 · 3,000원 추가징수 차액이 생깁니다.')
  })
})

describe('sumDeltas — 부서 일괄 미리보기 합계', () => {
  it('18명 × 5,000 = 90,000, 한 명 빼면 17명 85,000', () => {
    const one = deltaOf([f(35_000)], [f(30_000)], CODES)
    const all = Array.from({ length: 18 }, () => one)
    expect(sumDeltas(all)).toEqual({ refund: 90_000, additional: 0, count: 18 })
    expect(sumDeltas(all.slice(1))).toEqual({ refund: 85_000, additional: 0, count: 17 })
  })
})

const row = (o: Partial<RevertRow>): RevertRow => ({
  adjustmentId: 1,
  kind: 'REFUND',
  source: 'INDIVIDUAL',
  studentLabel: '1-1-1 학생',
  deptLabel: '바둑',
  amount: 5000,
  reverts: true,
  chargeBefore: 30000,
  chargeAfter: 35000,
  blocked: null,
  ...o,
})
const w = (n: number) => n.toLocaleString('ko-KR')

describe('revertSentence — 차액 기록을 지우면 금액도 되돌아간다', () => {
  it('환불 기록', () => {
    expect(revertSentence(row({}), w)).toBe(
      '이 환불 기록을 삭제하면 학생의 금액도 30,000원에서 35,000원으로 되돌아갑니다.',
    )
  })
  it('추가징수 기록', () => {
    expect(
      revertSentence(row({ kind: 'ADDITIONAL', chargeBefore: 40000, chargeAfter: 35000 }), w),
    ).toBe('이 추가징수 기록을 삭제하면 학생의 금액도 40,000원에서 35,000원으로 되돌아갑니다.')
  })
  it('기존 기록은 기록만 지운다', () => {
    const s = revertSentence(row({ source: 'LEGACY', reverts: false, chargeAfter: 30000 }), w)
    expect(s).toContain('기록만 지웁니다')
    expect(s).toContain('30,000원')
  })
})

describe('batchRevertSentence · revertShift — 묶음 삭제 안내', () => {
  const p = (rows: RevertRow[]): RevertPreview => ({
    rows,
    students: rows.length,
    refundTotal: rows.filter((r) => r.kind === 'REFUND').reduce((s, r) => s + r.amount, 0),
    additionalTotal: rows.filter((r) => r.kind !== 'REFUND').reduce((s, r) => s + r.amount, 0),
    blocked: rows.some((r) => r.blocked),
  })
  it('환불 17명', () => {
    const rows = Array.from({ length: 17 }, (_, i) => row({ adjustmentId: i, studentLabel: `s${i}` }))
    expect(batchRevertSentence(p(rows))).toBe(
      '일괄 환불 기록을 삭제하면 대상 학생 17명의 환불 기록이 삭제되고 해당 금액도 함께 원복됩니다.',
    )
    expect(revertShift(p(rows))).toBe(85000)
  })
  it('막힌 줄은 움직임에 넣지 않는다', () => {
    const rows = [row({}), row({ adjustmentId: 2, blocked: '다시 변경됨' })]
    expect(revertShift(p(rows))).toBe(5000)
  })
})
