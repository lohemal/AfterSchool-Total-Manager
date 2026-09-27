/**
 * 다중 정렬 규칙 시험 (v0.1.6).
 *
 * 화면과 떨어져 있으므로 DOM 없이 규칙만 본다.
 */

import { describe, expect, it } from 'vitest'

import {
  applySort,
  dirOf,
  MAX_SORT,
  priorityOf,
  sortMark,
  toggleSort,
  type SortSpec,
} from './sortSpec'

/** 누른 차례대로 돌린다. */
function 누르기(keys: string[], start: SortSpec[] = []): SortSpec[] {
  let sort = start
  for (const k of keys) sort = toggleSort(sort, k).sort
  return sort
}

const 키 = (s: SortSpec[]) => s.map((x) => `${x.key}${x.dir === 'ASC' ? '↑' : '↓'}`)

describe('한 열을 누르면 없음 → 오름 → 내림 → 없음', () => {
  it('세 번 누르면 처음으로 돌아온다', () => {
    let s: SortSpec[] = []
    s = toggleSort(s, 'grade').sort
    expect(s).toEqual([{ key: 'grade', dir: 'ASC' }])
    s = toggleSort(s, 'grade').sort
    expect(s).toEqual([{ key: 'grade', dir: 'DESC' }])
    s = toggleSort(s, 'grade').sort
    expect(s).toEqual([])
  })
})

describe('여러 열', () => {
  it('누른 차례가 우선순위가 된다', () => {
    const s = 누르기(['grade', 'classNo', 'studentNo'])
    expect(키(s)).toEqual(['grade↑', 'classNo↑', 'studentNo↑'])
    expect(priorityOf(s, 'grade')).toBe(1)
    expect(priorityOf(s, 'classNo')).toBe(2)
    expect(priorityOf(s, 'studentNo')).toBe(3)
    expect(priorityOf(s, 'name')).toBeNull()
  })

  it('방향을 바꿔도 자리는 그대로다', () => {
    // 2순위의 방향만 뒤집었는데 1순위로 튀어 오르면 차례를 다시 잡아야 한다
    let s = 누르기(['grade', 'classNo', 'studentNo'])
    s = toggleSort(s, 'classNo').sort
    expect(키(s)).toEqual(['grade↑', 'classNo↓', 'studentNo↑'])
    expect(priorityOf(s, 'classNo')).toBe(2)
  })

  it('중간을 풀면 뒤가 한 칸씩 당겨진다', () => {
    // 요구사항의 예시 그대로
    let s = 누르기(['grade', 'classNo', 'studentNo', 'name', 'sex'])
    expect(priorityOf(s, 'studentNo')).toBe(3)
    // 번호를 두 번 더 눌러 푼다 (오름 → 내림 → 없음)
    s = toggleSort(s, 'studentNo').sort
    s = toggleSort(s, 'studentNo').sort
    expect(키(s)).toEqual(['grade↑', 'classNo↑', 'name↑', 'sex↑'])
    expect(priorityOf(s, 'name')).toBe(3)
    expect(priorityOf(s, 'sex')).toBe(4)
  })
})

describe('최대 다섯 개', () => {
  it('여섯 번째는 막고 기존 조건을 그대로 둔다', () => {
    const s = 누르기(['a', 'b', 'c', 'd', 'e'])
    expect(s).toHaveLength(MAX_SORT)

    const r = toggleSort(s, 'f')
    expect(r.sort).toEqual(s)
    expect(r.message).toContain('최대 5개')
    expect(키(r.sort)).toEqual(['a↑', 'b↑', 'c↑', 'd↑', 'e↑'])
  })

  it('다섯 개가 차 있어도 기존 열의 방향 변경과 해제는 된다', () => {
    let s = 누르기(['a', 'b', 'c', 'd', 'e'])
    const r1 = toggleSort(s, 'c')
    expect(r1.message).toBeNull()
    expect(dirOf(r1.sort, 'c')).toBe('DESC')

    s = r1.sort
    const r2 = toggleSort(s, 'c')
    expect(r2.sort).toHaveLength(4)
    // 자리가 빈 뒤에는 새 열을 받을 수 있다
    const r3 = toggleSort(r2.sort, 'f')
    expect(r3.message).toBeNull()
    expect(priorityOf(r3.sort, 'f')).toBe(5)
  })
})

describe('머리글 표시', () => {
  it('방향과 순번을 함께 적는다', () => {
    let s = 누르기(['grade', 'classNo'])
    s = toggleSort(s, 'classNo').sort
    expect(sortMark(s, 'grade')).toBe('↑¹')
    expect(sortMark(s, 'classNo')).toBe('↓²')
    expect(sortMark(s, 'name')).toBe('')
  })

  it('다섯 번째까지 위첨자가 있다', () => {
    const s = 누르기(['a', 'b', 'c', 'd', 'e'])
    expect(sortMark(s, 'e')).toBe('↑⁵')
  })
})

describe('정렬 초기화', () => {
  it('빈 배열이면 기본 차례를 쓴다', () => {
    const rows = [3, 1, 2]
    expect(applySort(rows, [], { v: (a, b) => a - b }, () => 0)).toEqual([3, 1, 2])
  })
})

describe('화면 정렬', () => {
  interface 학생 {
    id: number
    grade: number
    classNo: string
    name: string
  }
  const 사람들: 학생[] = [
    { id: 1, grade: 2, classNo: '1', name: '나' },
    { id: 2, grade: 1, classNo: '10', name: '가' },
    { id: 3, grade: 1, classNo: '2', name: '다' },
    { id: 4, grade: 1, classNo: '2', name: '가' },
  ]
  const 비교 = {
    grade: (a: 학생, b: 학생) => a.grade - b.grade,
    classNo: (a: 학생, b: 학생) => Number(a.classNo) - Number(b.classNo),
    name: (a: 학생, b: 학생) => a.name.localeCompare(b.name, 'ko'),
  }
  const 마지막 = (a: 학생, b: 학생) => a.id - b.id

  it('우선순위대로 걸린다', () => {
    const s: SortSpec[] = [
      { key: 'grade', dir: 'ASC' },
      { key: 'classNo', dir: 'ASC' },
      { key: 'name', dir: 'ASC' },
    ]
    expect(applySort(사람들, s, 비교, 마지막).map((x) => x.id)).toEqual([4, 3, 2, 1])
  })

  it('내림차순', () => {
    const s: SortSpec[] = [{ key: 'grade', dir: 'DESC' }]
    expect(applySort(사람들, s, 비교, 마지막)[0].grade).toBe(2)
  })

  it('모두 같으면 마지막 규칙이 차례를 정한다', () => {
    const 같은값: 학생[] = [
      { id: 9, grade: 1, classNo: '1', name: '같음' },
      { id: 3, grade: 1, classNo: '1', name: '같음' },
      { id: 7, grade: 1, classNo: '1', name: '같음' },
    ]
    const s: SortSpec[] = [{ key: 'name', dir: 'ASC' }]
    for (let i = 0; i < 5; i++) {
      expect(applySort(같은값, s, 비교, 마지막).map((x) => x.id)).toEqual([3, 7, 9])
    }
  })

  it('모르는 열은 건너뛴다', () => {
    const s: SortSpec[] = [
      { key: '없는열', dir: 'ASC' },
      { key: 'grade', dir: 'ASC' },
    ]
    expect(applySort(사람들, s, 비교, 마지막).map((x) => x.grade)).toEqual([1, 1, 1, 2])
  })

  it('원본 배열을 건드리지 않는다', () => {
    const 앞 = 사람들.map((x) => x.id)
    applySort(사람들, [{ key: 'grade', dir: 'DESC' }], 비교, 마지막)
    expect(사람들.map((x) => x.id)).toEqual(앞)
  })
})
