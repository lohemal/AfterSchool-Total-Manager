/**
 * 수강 가능 학년 표기 (v0.1.7).
 *
 * **빈 배열은 '미설정'이다.** '전 학년'도 '전 학년 불가'도 아니다.
 * 지원정책의 대상학년(`support_policy.target_grades`)은 빈 값이 '전 학년'이라
 * 뜻이 반대이므로, 그쪽 표기 함수를 여기에 그대로 쓰면 안 된다.
 */

/** 초등학교 기준. */
export const GRADES = [1, 2, 3, 4, 5, 6] as const

/**
 * `1·2·4학년` · `전 학년` · `미설정`.
 *
 * 띄엄띄엄한 학년을 `1~4학년` 처럼 범위로 줄이지 않는다 — 3학년이 대상인 줄
 * 알고 잘못 안내하게 된다.
 */
export function gradeText(grades: number[]): string {
  if (grades.length === 0) return '미설정'
  const g = [...new Set(grades)].sort((a, b) => a - b)
  if (g.length === 6 && GRADES.every((x) => g.includes(x))) return '전 학년'
  return `${g.join('·')}학년`
}

/** 켜고 끄기. 차례는 늘 오름차순으로 둔다. */
export function toggleGrade(grades: number[], grade: number): number[] {
  const has = grades.includes(grade)
  const next = has ? grades.filter((g) => g !== grade) : [...grades, grade]
  return next.sort((a, b) => a - b)
}
