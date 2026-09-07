/**
 * 학생 상세정보 조회 규칙 (최종 QA).
 *
 * 화면에서 떼어 낸 순수 함수다. 전교생을 넣으면 한 학년이 100명을 넘는데,
 * 그걸 한 번에 다 그리면 화면이 쓸 수 없게 된다. 그래서 두 가지를 둔다.
 *
 * 1. **너무 넓은 조건은 아예 찾지 않는다** — 학년만 골랐을 때.
 *    빈 목록을 보여 주는 것과 다르다. 무엇을 더 넣어야 하는지 알려 준다.
 * 2. **20명씩 보여 준다** — 나머지는 [N명 더 보기]로 이어 붙인다.
 */

import { compareClassNo } from './format'

/** 한 번에 보여 주는 사람 수. */
export const PAGE = 20

export interface Conditions {
  /** `''` = 전체 */
  grade: string
  /** `''` = 전체 */
  classNo: string
  studentNo: string
  name: string
}

export interface Searchable {
  grade: number
  classNo: string
  studentNo: number
  name: string
}

/** 넣은 조건이 하나라도 있는가. 하나도 없으면 조회하지 않는다. */
export function hasCondition(c: Conditions): boolean {
  return c.grade !== '' || c.classNo !== '' || c.studentNo.trim() !== '' || c.name.trim() !== ''
}

/**
 * 학년 말고는 아무것도 안 넣었는가.
 *
 * 이 경우만 막는다. 반·번호·이름 가운데 하나라도 있으면 범위가 충분히 좁혀지고,
 * 그래도 많이 나오면 [더 보기]가 받아 준다.
 */
export function tooBroad(c: Conditions): boolean {
  const narrowed = c.classNo !== '' || c.studentNo.trim() !== '' || c.name.trim() !== ''
  return c.grade !== '' && !narrowed
}

/** 번호 칸에 적은 것에서 숫자만 뽑는다. 숫자가 없으면 조건으로 쓰지 않는다. */
function askedNo(raw: string): number | null {
  const digits = raw.replace(/[^0-9]/g, '')
  return digits === '' ? null : Number(digits)
}

/**
 * 넣은 조건을 **모두** 만족하는 학생 (AND).
 *
 * 차례는 `학년 → 반 자연정렬 → 번호 → 이름`이다. 숫자 반은 `1, 2, 10`,
 * 문자 반은 정해진 문자열 차례를 지킨다 (`compareClassNo`).
 */
export function search<T extends Searchable>(rows: T[], c: Conditions): T[] {
  if (!hasCondition(c) || tooBroad(c)) return []

  const grade = c.grade === '' ? null : Number(c.grade)
  const no = askedNo(c.studentNo)
  const name = c.name.trim()

  return rows
    .filter((s) => {
      if (grade !== null && s.grade !== grade) return false
      if (c.classNo !== '' && s.classNo !== c.classNo) return false
      if (no !== null && s.studentNo !== no) return false
      // 이름은 부분검색을 그대로 둔다 — '김'으로 김씨를 다 찾을 수 있어야 한다.
      if (name !== '' && !s.name.includes(name)) return false
      return true
    })
    .sort(
      (a, b) =>
        a.grade - b.grade ||
        compareClassNo(a.classNo, b.classNo) ||
        a.studentNo - b.studentNo ||
        a.name.localeCompare(b.name, 'ko'),
    )
}

/** 결과 줄에 적을 문구와 [더 보기] 버튼에 적을 인원. */
export function pageInfo(total: number, shown: number): { label: string; more: number } {
  const more = Math.max(0, total - shown)
  return {
    label: more > 0 ? `${total}명 검색됨 · ${shown}명 표시 중` : `${total}명 검색됨`,
    more,
  }
}
