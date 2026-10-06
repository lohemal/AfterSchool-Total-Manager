import { describe, expect, it } from 'vitest'

import { gradeText, toggleGrade } from './grades'

describe('gradeText', () => {
  it('빈 배열은 미설정이다 — 전 학년도 전 학년 불가도 아니다', () => {
    expect(gradeText([])).toBe('미설정')
  })

  it('띄엄띄엄한 학년을 범위로 줄이지 않는다', () => {
    expect(gradeText([1, 2, 4])).toBe('1·2·4학년')
    expect(gradeText([1, 2, 4])).not.toContain('~')
  })

  it('차례와 중복을 바로잡는다', () => {
    expect(gradeText([4, 1, 2, 1])).toBe('1·2·4학년')
  })

  it('여섯 학년이 모두 있으면 전 학년이다', () => {
    expect(gradeText([1, 2, 3, 4, 5, 6])).toBe('전 학년')
    expect(gradeText([1, 2, 3, 4, 5])).toBe('1·2·3·4·5학년')
  })

  it('한 학년만도 된다', () => {
    expect(gradeText([3])).toBe('3학년')
  })
})

describe('toggleGrade', () => {
  it('켜고 끄며 차례를 지킨다', () => {
    expect(toggleGrade([], 3)).toEqual([3])
    expect(toggleGrade([4, 1], 2)).toEqual([1, 2, 4])
    expect(toggleGrade([1, 2, 4], 2)).toEqual([1, 4])
  })

  it('모두 끄면 빈 배열 — 곧 미설정이다', () => {
    expect(toggleGrade([3], 3)).toEqual([])
    expect(gradeText(toggleGrade([3], 3))).toBe('미설정')
  })
})
