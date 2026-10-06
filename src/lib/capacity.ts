/**
 * 정원 상태 표기 (v0.1.7).
 *
 * 숫자를 색으로만 가르지 않는다 — **글로도 적는다.** 색각이 다른 사람도,
 * 흑백으로 인쇄한 사람도 같은 것을 읽어야 한다.
 *
 * `정원 초과`는 눈에 띄어야 하지만 경고색을 남용하지 않는다. 방과후는 정원을
 * 꼭 채워야 하는 사업이 아니라서 '부족'이라는 말도 쓰지 않는다.
 */

import type { CapacityStatus, SeatFinding } from '@/ipc/types'

export const STATUS_TEXT: Record<CapacityStatus, string> = {
  UNSET: '정원 미설정',
  OPEN: '모집 가능',
  FULL: '정원 도달',
  OVER: '정원 초과',
}

/** `tag--*` 꼬리표. 넘친 반만 눈에 띄게 하고 나머지는 차분하게 둔다. */
export const STATUS_TONE: Record<CapacityStatus, string> = {
  UNSET: 'plain',
  OPEN: 'free',
  FULL: 'plain',
  OVER: 'warn',
}

/** 정렬용 차례 — 손볼 것부터 위로. */
export const STATUS_ORDER: Record<CapacityStatus, number> = {
  OVER: 0,
  FULL: 1,
  OPEN: 2,
  UNSET: 3,
}

export const FINDING_TEXT: Record<SeatFinding, string> = {
  OPEN: '신청 가능',
  CAPACITY_UNKNOWN: '정원 확인 필요',
  GRADE_UNKNOWN: '대상 학년 확인 필요',
  FULL: '정원 도달',
  OVER: '정원 초과',
}

export const FINDING_TONE: Record<SeatFinding, string> = {
  OPEN: 'free',
  CAPACITY_UNKNOWN: 'warn',
  GRADE_UNKNOWN: 'warn',
  FULL: 'plain',
  OVER: 'warn',
}

/** 사람 수. 금액이 아니므로 `won()` 을 쓰지 않는다. */
export function people(n: number | null): string {
  return n === null ? '—' : n.toLocaleString('ko-KR')
}

export function percent(n: number | null): string {
  return n === null ? '—' : `${n.toFixed(1)}%`
}
