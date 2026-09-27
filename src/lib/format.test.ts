import { describe, expect, it } from 'vitest'

import { compareClassNo, compareDays, parseDays } from './format'

describe('parseDays', () => {
  it('구분자가 무엇이든 요일만 뽑는다', () => {
    expect(parseDays('월,수')).toEqual(['월', '수'])
    expect(parseDays('월 수')).toEqual(['월', '수'])
    expect(parseDays('월·수')).toEqual(['월', '수'])
    expect(parseDays('월/수')).toEqual(['월', '수'])
    expect(parseDays('월요일, 수요일')).toEqual(['월', '수'])
  })
  it('요일이 아닌 글은 버린다', () => {
    expect(parseDays('')).toEqual([])
    expect(parseDays('미정')).toEqual([])
    expect(parseDays('격주')).toEqual([])
  })
})

describe('compareDays', () => {
  const 줄세우기 = (xs: string[]) => [...xs].sort(compareDays)

  it('가나다순이 아니라 업무상 요일 차례다', () => {
    expect(줄세우기(['금', '월', '수', '일', '토', '목', '화'])).toEqual([
      '월',
      '화',
      '수',
      '목',
      '금',
      '토',
      '일',
    ])
  })

  it('내림차순은 비교기를 뒤집은 것이다', () => {
    const desc = ['금', '월', '수', '일', '토', '목', '화'].sort((a, b) => -compareDays(a, b))
    expect(desc).toEqual(['일', '토', '금', '목', '수', '화', '월'])
  })

  it('여러 요일은 이른 요일부터 견준다', () => {
    expect(줄세우기(['화', '월,수', '월'])).toEqual(['월', '월,수', '화'])
    expect(줄세우기(['수,금', '월,수,금', '화,목'])).toEqual(['월,수,금', '화,목', '수,금'])
  })

  it('같은 요일을 다르게 적었으면 요일로는 못 가리므로 적힌 글로 갈라 자리를 고정한다', () => {
    expect(줄세우기(['수,월', '월,수'])).toEqual(['수,월', '월,수'])
    expect(줄세우기(['월,수', '수,월'])).toEqual(['수,월', '월,수'])
  })

  it('비어 있거나 요일이 아닌 칸은 오름차순에서 맨 뒤에 둔다', () => {
    expect(줄세우기(['', '일', '월', '미정'])).toEqual(['월', '일', '', '미정'])
  })
})

describe('compareClassNo — 부서 반명에도 같은 규칙을 쓴다', () => {
  it('숫자 반은 자연수 차례다', () => {
    expect(['10', '2', '1'].sort(compareClassNo)).toEqual(['1', '2', '10'])
  })
  it("'01' 과 '1' 은 서로 다른 반으로 남는다", () => {
    expect(compareClassNo('01', '1')).not.toBe(0)
  })
  it('문자 반은 숫자 반 뒤에 온다', () => {
    expect(['A', '1', 'B'].sort(compareClassNo)).toEqual(['1', 'A', 'B'])
  })
})
