/**
 * 재학상태 표기 (v0.1.8).
 *
 * 상태는 저장된 값이 아니라 **전출일과 오늘로 판정한 값**이다. 그래서 날짜를
 * 고치면 상태가 바로 따라 바뀌고, 전출 예정일이 지나면 아무도 손대지 않아도
 * 전출로 넘어간다.
 */

import type { Enrolment } from '@/ipc/types'

export const ENROLMENT_TEXT: Record<Enrolment, string> = {
  ATTENDING: '재학',
  LEAVING: '전출 예정',
  LEFT: '전출',
}

/** 꼬리표 색. 전출은 눈에 띄되 경고로 보이지는 않게 둔다. */
export const ENROLMENT_TONE: Record<Enrolment, string> = {
  ATTENDING: 'free',
  LEAVING: 'warn',
  LEFT: 'plain',
}

/** 목록 정렬용 — 재학 → 전출 예정 → 전출 */
export const ENROLMENT_ORDER: Record<Enrolment, number> = {
  ATTENDING: 0,
  LEAVING: 1,
  LEFT: 2,
}

/** `전출 예정 10/15` 처럼 짧게. 학생 고르는 칸에서 쓴다. */
export function enrolmentShort(e: Enrolment, date: string | null): string {
  if (e === 'ATTENDING') return ''
  const md = date ? date.slice(5).replace('-', '/') : ''
  return md ? `${ENROLMENT_TEXT[e]} ${md}` : ENROLMENT_TEXT[e]
}
