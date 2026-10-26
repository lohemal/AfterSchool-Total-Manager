import { describe, expect, it } from 'vitest'

import type { Fee } from '@/ipc/types'

import { deltaOf, deltaSentence, sumDeltas } from './delta'

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
