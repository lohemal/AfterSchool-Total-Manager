/**
 * 금액 차액 셈 (v0.1.9) — 부서 일괄과 학생 개별이 함께 쓴다.
 *
 * 환불과 추가징수를 따로 만들지 않는다. **금액이 줄면 환불, 늘면 추가징수**이고,
 * 그 방향은 사람이 고르지 않는다. 사람이 고르는 것은 '이 차액을 추가징수·환불로
 * 남길지(돈이 오간다)' 아니면 '금액만 고칠지(잘못 넣은 값을 바로잡는다)' 하나다.
 *
 * 여기 값은 **미리보기**다. 실제 기록은 서버가 지금 DB 값을 기준으로 다시 셈해
 * 만든다 — 화면이 보낸 '이전 값'을 믿지 않으므로 같은 요청이 두 번 와도 겹치지
 * 않는다.
 */

import type { Fee } from '@/ipc/types'

export interface ItemDelta {
  itemCode: string
  before: number
  after: number
  /** 늘면 양수, 줄면 음수 */
  diff: number
}

export interface DeltaSummary {
  items: ItemDelta[]
  /** 돌려줄 돈 (줄어든 항목의 합, 양수) */
  refund: number
  /** 더 걷을 돈 (늘어난 항목의 합, 양수) */
  additional: number
  /** 차액이 하나라도 있는가 */
  changed: boolean
}

function amountOf(fees: Fee[], code: string): number {
  return fees.find((f) => f.itemCode === code)?.amount ?? 0
}

/** 항목별 차액. 한 학생 안에서 강사료는 줄고 재료비는 늘 수 있다. */
export function deltaOf(before: Fee[], after: Fee[], codes: string[]): DeltaSummary {
  const items = codes.map((code) => {
    const b = amountOf(before, code)
    const a = amountOf(after, code)
    return { itemCode: code, before: b, after: a, diff: a - b }
  })
  const refund = items.filter((d) => d.diff < 0).reduce((s, d) => s - d.diff, 0)
  const additional = items.filter((d) => d.diff > 0).reduce((s, d) => s + d.diff, 0)
  return { items, refund, additional, changed: refund > 0 || additional > 0 }
}

/** `5,000원의 환불 차액이 생깁니다` 처럼 사람에게 보일 한 줄. */
export function deltaSentence(s: DeltaSummary, won: (n: number) => string): string {
  if (!s.changed) return '금액 차이가 없습니다.'
  const parts: string[] = []
  if (s.refund > 0) parts.push(`${won(s.refund)}원 환불`)
  if (s.additional > 0) parts.push(`${won(s.additional)}원 추가징수`)
  return `${parts.join(' · ')} 차액이 생깁니다.`
}

/** 여러 학생 차액을 더한다 — 부서 일괄 미리보기의 합계 줄. */
export function sumDeltas(list: DeltaSummary[]): { refund: number; additional: number; count: number } {
  return list.reduce(
    (acc, s) => ({
      refund: acc.refund + s.refund,
      additional: acc.additional + s.additional,
      count: acc.count + (s.changed ? 1 : 0),
    }),
    { refund: 0, additional: 0, count: 0 },
  )
}

/** 발생 유형을 사람이 읽는 말로. 서버 `source_label` 과 같은 말을 쓴다. */
export const SOURCE_TEXT: Record<string, string> = {
  LEGACY: '기존 기록',
  ENROLL: '중도추가',
  CANCEL: '수강취소',
  DEPT_BATCH: '부서일괄',
  INDIVIDUAL: '개별조정',
}
