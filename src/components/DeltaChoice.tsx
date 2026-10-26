/**
 * 금액만 고칠까, 추가징수·환불로 남길까 (v0.1.9).
 *
 * 부서 일괄과 학생 개별이 **같은 고르기**를 쓴다. 환불과 추가징수를 따로
 * 만들지 않는 까닭은, 둘이 같은 일의 두 방향이기 때문이다 — 금액이 줄면 환불,
 * 늘면 추가징수다. 그 방향은 사람이 고르지 않는다.
 *
 * ## 두 고르기는 뜻이 다르다
 *
 * - **금액만 고친다** — 잘못 넣은 값을 바로잡는다. 돈이 오가지 않는다.
 * - **추가징수·환불로 남긴다** — 정상으로 걷은 뒤에 실제로 돈이 오가야 한다.
 *
 * 금액이 줄었다고 늘 환불이 아니다. 그래서 기본값은 '금액만'이고, 남기려면
 * 사람이 직접 고르고 사유를 적어야 한다.
 *
 * 어느 쪽이든 학생이 최종으로 내는 돈은 고친 금액 하나다. 기록은 그 위에 한 번
 * 더 더해지거나 빠지지 않는다.
 */

import { Field, Input } from './ui'

export interface DeltaChoiceValue {
  register: boolean
  occurredOn: string
  note: string
}

export function DeltaChoice({
  value,
  onChange,
  refund,
  additional,
}: {
  value: DeltaChoiceValue
  onChange: (v: DeltaChoiceValue) => void
  /** 미리보기에서 낸 돌려줄 돈 */
  refund: number
  /** 미리보기에서 낸 더 걷을 돈 */
  additional: number
}) {
  const won = (n: number) => n.toLocaleString('ko-KR')
  const 방향 =
    refund > 0 && additional > 0
      ? '추가징수·환불'
      : refund > 0
        ? '환불'
        : additional > 0
          ? '추가징수'
          : '추가징수·환불'

  return (
    <div className="card" style={{ padding: 12, marginTop: 10 }}>
      <div style={{ marginBottom: 8 }}>
        {refund > 0 && (
          <div>
            <b>{won(refund)}원</b> 환불 차액이 생깁니다.
          </div>
        )}
        {additional > 0 && (
          <div>
            <b>{won(additional)}원</b> 추가징수 차액이 생깁니다.
          </div>
        )}
      </div>

      <label className="checkItem" style={{ display: 'flex', marginBottom: 6 }}>
        <input
          type="radio"
          checked={!value.register}
          onChange={() => onChange({ ...value, register: false })}
        />
        <span>
          <b>금액만 수정</b> — 잘못 넣은 값을 바로잡습니다. 추가징수·환불 명단에 오르지 않습니다.
        </span>
      </label>
      <label className="checkItem" style={{ display: 'flex' }}>
        <input
          type="radio"
          checked={value.register}
          onChange={() => onChange({ ...value, register: true })}
        />
        <span>
          <b>{방향} 대상으로 등록</b> — 이미 걷은 뒤에 실제로 돈이 오가야 합니다.
        </span>
      </label>

      {value.register && (
        <div className="grid4" style={{ marginTop: 10 }}>
          <Field label="발생일">
            <Input
              type="date"
              value={value.occurredOn}
              onChange={(e) => onChange({ ...value, occurredOn: e.target.value })}
            />
          </Field>
          <div style={{ gridColumn: 'span 3' }}>
            <Field label={`${방향} 사유 (필수)`}>
              <Input
                value={value.note}
                placeholder="강사 사정으로 1회 휴강 · 재료비 조정 …"
                onChange={(e) => onChange({ ...value, note: e.target.value })}
              />
            </Field>
          </div>
        </div>
      )}

      <div className="hint" style={{ marginTop: 8 }}>
        어느 쪽이든 학생이 최종으로 내는 금액은 <b>고친 금액 하나</b>입니다. 등록한 차액이 그
        위에 한 번 더 더해지거나 빠지지 않습니다.
      </div>
    </div>
  )
}

/** 고르기가 저장해도 되는 상태인가. 등록하려면 사유가 있어야 한다. */
export function deltaChoiceReady(v: DeltaChoiceValue): boolean {
  return !v.register || v.note.trim().length > 0
}
