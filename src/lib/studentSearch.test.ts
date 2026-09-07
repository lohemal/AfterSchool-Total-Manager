/**
 * 학생 상세정보 조회 규칙 시험 (최종 QA).
 *
 * 전교생을 넣으면 한 학년이 100명을 넘는다. 여기서 지키는 것은 두 가지다.
 *   · 너무 넓은 조건은 아예 찾지 않는다 (학년만 골랐을 때)
 *   · 조건을 좁히면 20명씩 보여 주고 나머지는 [더 보기]
 */

import { describe, expect, it } from 'vitest'

import { hasCondition, PAGE, pageInfo, search, tooBroad, type Conditions } from './studentSearch'

const 빈조건: Conditions = { grade: '', classNo: '', studentNo: '', name: '' }
const 조건 = (v: Partial<Conditions>): Conditions => ({ ...빈조건, ...v })

interface S {
  grade: number
  classNo: string
  studentNo: number
  name: string
}
const 학생 = (grade: number, classNo: string, studentNo: number, name: string): S => ({
  grade,
  classNo,
  studentNo,
  name,
})

/** 1학년 가반 27명 — [더 보기]를 보려면 20명을 넘겨야 한다. */
const 한학급_27명 = Array.from({ length: 27 }, (_, i) => 학생(1, '가', i + 1, `학생${i + 1}`))

const 여러학급: S[] = [
  학생(1, '가', 1, '김하나'),
  학생(1, '가', 2, '이두리'),
  학생(1, '나', 1, '김세찬'),
  학생(1, '10', 1, '박열'),
  학생(1, '2', 1, '최둘'),
  학생(1, '1', 1, '정하나'),
  학생(2, '가', 1, '김이학년'),
]

describe('조건이 있는가', () => {
  it('아무것도 넣지 않으면 조회하지 않는다', () => {
    expect(hasCondition(빈조건)).toBe(false)
    expect(search(여러학급, 빈조건)).toEqual([])
  })

  it('하나라도 넣으면 조회한다', () => {
    expect(hasCondition(조건({ grade: '1' }))).toBe(true)
    expect(hasCondition(조건({ classNo: '가' }))).toBe(true)
    expect(hasCondition(조건({ studentNo: '5' }))).toBe(true)
    expect(hasCondition(조건({ name: '김' }))).toBe(true)
  })

  it('공백만 넣은 것은 조건이 아니다', () => {
    expect(hasCondition(조건({ name: '   ' }))).toBe(false)
    expect(hasCondition(조건({ studentNo: ' ' }))).toBe(false)
  })
})

describe('너무 넓은 조건', () => {
  it('학년만 고르면 찾지 않고 안내한다', () => {
    const c = 조건({ grade: '1' })
    expect(tooBroad(c)).toBe(true)
    // 빈 목록을 보여 주는 것과 다르다 — 화면이 안내 문구를 띄운다.
    expect(search(한학급_27명, c)).toEqual([])
  })

  it('반을 고르면 넓지 않다', () => {
    expect(tooBroad(조건({ grade: '1', classNo: '가' }))).toBe(false)
  })

  it('번호나 이름을 넣으면 넓지 않다', () => {
    expect(tooBroad(조건({ grade: '1', studentNo: '5' }))).toBe(false)
    expect(tooBroad(조건({ grade: '1', name: '김' }))).toBe(false)
  })

  it('학년이 없으면 넓다고 하지 않는다', () => {
    // 번호나 이름만으로도 찾을 수 있어야 한다 (기존 동작 유지)
    expect(tooBroad(조건({ name: '김' }))).toBe(false)
    expect(tooBroad(조건({ studentNo: '5' }))).toBe(false)
    expect(tooBroad(빈조건)).toBe(false)
  })
})

describe('조건은 AND 로 걸린다', () => {
  it('학년 + 반', () => {
    const r = search(여러학급, 조건({ grade: '1', classNo: '가' }))
    expect(r.map((s) => s.name)).toEqual(['김하나', '이두리'])
  })

  it('학년 + 반 + 번호', () => {
    const r = search(여러학급, 조건({ grade: '1', classNo: '가', studentNo: '2' }))
    expect(r.map((s) => s.name)).toEqual(['이두리'])
  })

  it('학년 + 이름 (부분검색)', () => {
    const r = search(여러학급, 조건({ grade: '1', name: '김' }))
    expect(r.map((s) => s.name)).toEqual(['김하나', '김세찬'])
  })

  it('반 + 이름 — 학년이 없어도 걸린다', () => {
    const r = search(여러학급, 조건({ classNo: '가', name: '김' }))
    expect(r.map((s) => s.name)).toEqual(['김하나', '김이학년'])
  })

  it('이름만으로 학년을 넘어 찾는다', () => {
    const r = search(여러학급, 조건({ name: '하나' }))
    expect(r.map((s) => s.name)).toEqual(['정하나', '김하나'])
  })

  it('번호 칸에 글자가 섞여도 숫자만 쓴다', () => {
    const r = search(여러학급, 조건({ classNo: '가', studentNo: '2번' }))
    expect(r.map((s) => s.name)).toEqual(['이두리'])
  })

  it('맞는 학생이 없으면 빈 결과다', () => {
    expect(search(여러학급, 조건({ grade: '1', classNo: '가', studentNo: '99' }))).toEqual([])
  })
})

describe('정렬 — 학년 → 반 자연정렬 → 번호 → 이름', () => {
  it('숫자 반은 1, 2, 10 이고 문자 반은 그 뒤다', () => {
    // 1번 학생만 남겨 반 차례만 본다. 사전순이면 1, 10, 2 가 된다.
    const r = search(여러학급, 조건({ studentNo: '1' }))
    expect(r.map((s) => `${s.grade}-${s.classNo}`)).toEqual([
      '1-1',
      '1-2',
      '1-10',
      '1-가',
      '1-나',
      '2-가',
    ])
  })

  it('넣는 차례가 달라도 결과 차례는 같다', () => {
    const 뒤집힘 = [...여러학급].reverse()
    const a = search(여러학급, 조건({ studentNo: '1' })).map((s) => `${s.grade}-${s.classNo}`)
    const b = search(뒤집힘, 조건({ studentNo: '1' })).map((s) => `${s.grade}-${s.classNo}`)
    expect(b).toEqual(a)
  })

  it('같은 반 안에서는 번호 차례다', () => {
    const r = search(한학급_27명, 조건({ grade: '1', classNo: '가' }))
    expect(r.map((s) => s.studentNo).slice(0, 5)).toEqual([1, 2, 3, 4, 5])
    expect(r[r.length - 1].studentNo).toBe(27)
  })

  it('원본 배열을 건드리지 않는다', () => {
    const 원본 = [...여러학급]
    search(여러학급, 조건({ studentNo: '1' }))
    expect(여러학급).toEqual(원본)
  })
})

describe('더 보기', () => {
  it('20명까지는 더 보기가 없다', () => {
    expect(pageInfo(20, 20)).toEqual({ label: '20명 검색됨', more: 0 })
    expect(pageInfo(7, 7)).toEqual({ label: '7명 검색됨', more: 0 })
  })

  it('27명이면 20명을 보여 주고 7명 더 보기', () => {
    const p = pageInfo(27, PAGE)
    expect(p.label).toBe('27명 검색됨 · 20명 표시 중')
    expect(p.more).toBe(7)
  })

  it('더 보기를 누르면 남은 인원이 없어진다', () => {
    const p = pageInfo(27, PAGE + PAGE)
    expect(p.label).toBe('27명 검색됨')
    expect(p.more).toBe(0)
  })

  it('한 학급 27명이 실제로 20명씩 나뉜다', () => {
    const r = search(한학급_27명, 조건({ grade: '1', classNo: '가' }))
    expect(r.length).toBe(27)
    expect(r.slice(0, PAGE).length).toBe(20)
    expect(pageInfo(r.length, PAGE).more).toBe(7)
    // 더 보기를 한 번 누르면 전부 보인다
    expect(r.slice(0, PAGE * 2).length).toBe(27)
  })

  it('보여 준 수가 전체를 넘어도 음수가 나오지 않는다', () => {
    expect(pageInfo(3, 20).more).toBe(0)
  })
})
