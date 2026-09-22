/**
 * ☐ 추가징수 대상 / ☐ 환불 대상 (v0.1.5).
 *
 * 수강 추가 창과 취소 창이 함께 쓴다. **기본값은 체크 해제다** — 프로그램이
 * 등록일이나 취소 상태만 보고 스스로 정하면 안 된다. 최초 징수 전에 넣는
 * 학생도 있고, 환불할 것이 없는 취소도 있다.
 *
 * 체크했을 때만 발생일과 사유가 열린다.
 */

import type { ReactNode } from 'react'

import type { AdjustmentInput } from '@/ipc/types'

import { Field, Input } from './ui'

/** 오늘 날짜 `YYYY-MM-DD`. 기본 발생일이다. */
export function todayISO(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

/** 체크하지 않았으면 `null` — 그러면 명령이 기록을 만들지 않는다. */
export function adjustmentOf(on: boolean, occurredOn: string, note: string): AdjustmentInput | null {
  return on ? { occurredOn: occurredOn || todayISO(), note: note.trim() || null } : null
}

export function AdjustmentToggle({
  label,
  hint,
  on,
  setOn,
  occurredOn,
  setOccurredOn,
  note,
  setNote,
  children,
}: {
  /** `추가징수 대상` · `환불 대상` */
  label: string
  hint: ReactNode
  on: boolean
  setOn: (v: boolean) => void
  occurredOn: string
  setOccurredOn: (v: string) => void
  note: string
  setNote: (v: string) => void
  /** 체크했을 때 함께 보여 줄 것 (환불액 미리보기 등) */
  children?: ReactNode
}) {
  return (
    <div
      style={{
        marginTop: 12,
        padding: 12,
        border: '1px solid var(--gray-200)',
        borderRadius: 8,
        background: on ? 'var(--blue-50)' : undefined,
      }}
    >
      <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
        <input type="checkbox" checked={on} onChange={(e) => setOn(e.target.checked)} />
        <b>{label}</b>
      </label>
      <div className="hint" style={{ marginTop: 6, lineHeight: 1.7 }}>
        {hint}
      </div>

      {on && (
        <>
          {children}
          {/*
           * 이름표끼리·입력칸끼리 높이를 맞춘다. 설명을 `Field` 안에 넣으면 그
           * 칸만 키가 커져서 줄이 어긋나므로 줄 아래에 따로 둔다.
           */}
          <div className="formRow" style={{ marginTop: 10 }}>
            <Field label="발생일">
              <Input
                type="date"
                value={occurredOn}
                onChange={(e) => setOccurredOn(e.target.value)}
              />
            </Field>
            <Field label="사유 · 메모">
              <Input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="선택 입력"
              />
            </Field>
          </div>
          <div className="hint" style={{ marginTop: 6 }}>
            사유·메모는 추가·취소 관리 상세에 남습니다.
          </div>
        </>
      )}
    </div>
  )
}
