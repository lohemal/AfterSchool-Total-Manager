import { describe, expect, it } from 'vitest'

import { ENROLMENT_ORDER, ENROLMENT_TEXT, enrolmentShort } from './transfer'

describe('재학상태 표기', () => {
  it('세 가지를 모두 우리말로 적는다', () => {
    expect(ENROLMENT_TEXT.ATTENDING).toBe('재학')
    expect(ENROLMENT_TEXT.LEAVING).toBe('전출 예정')
    expect(ENROLMENT_TEXT.LEFT).toBe('전출')
  })

  it('재학은 꼬리말을 붙이지 않는다 — 대부분이 재학이라 글이 시끄러워진다', () => {
    expect(enrolmentShort('ATTENDING', null)).toBe('')
    expect(enrolmentShort('ATTENDING', '2026-10-15')).toBe('')
  })

  it('전출 예정은 날짜를 짧게 붙인다', () => {
    expect(enrolmentShort('LEAVING', '2026-10-15')).toBe('전출 예정 10/15')
    expect(enrolmentShort('LEFT', '2026-09-01')).toBe('전출 09/01')
  })

  it('날짜가 없으면 상태만 적는다', () => {
    expect(enrolmentShort('LEFT', null)).toBe('전출')
  })

  it('정렬 차례는 재학부터다', () => {
    const order = (['LEFT', 'ATTENDING', 'LEAVING'] as const)
      .slice()
      .sort((a, b) => ENROLMENT_ORDER[a] - ENROLMENT_ORDER[b])
    expect(order).toEqual(['ATTENDING', 'LEAVING', 'LEFT'])
  })
})
