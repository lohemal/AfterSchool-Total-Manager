/**
 * 차감 우선순위 편집 — 위/아래 버튼으로 순서를 바꾼다 (요구사항 §7·§8).
 *
 * 부서와 비용항목이 같은 모양이라 하나로 쓴다.
 */

import { useEffect, useState } from 'react'

import type { PriorityRow } from '@/ipc/types'

import { Button, Empty } from './ui'

export function PriorityEditor({
  rows,
  showCount,
  emptyTitle,
  emptyHint,
  onChange,
}: {
  rows: PriorityRow[]
  /** 부서 목록에서만 '대상자 N명'을 보여 준다 */
  showCount?: boolean
  emptyTitle: string
  emptyHint: string
  onChange: (keys: string[]) => void
}) {
  const [order, setOrder] = useState<PriorityRow[]>(rows)
  const [picked, setPicked] = useState<string | null>(null)

  useEffect(() => setOrder(rows), [rows])

  function move(delta: -1 | 1) {
    if (!picked) return
    const i = order.findIndex((r) => r.key === picked)
    const j = i + delta
    if (i < 0 || j < 0 || j >= order.length) return
    const next = [...order]
    ;[next[i], next[j]] = [next[j], next[i]]
    setOrder(next)
    onChange(next.map((r) => r.key))
  }

  if (order.length === 0) {
    return <Empty title={emptyTitle}>{emptyHint}</Empty>
  }

  const i = picked ? order.findIndex((r) => r.key === picked) : -1

  return (
    <div style={{ display: 'flex', gap: 10, alignItems: 'flex-start' }}>
      <div
        style={{
          flex: 1,
          border: '1px solid var(--gray-200)',
          borderRadius: 'var(--radius)',
          overflow: 'hidden',
          maxHeight: 300,
          overflowY: 'auto',
        }}
      >
        {order.map((r, idx) => (
          <div
            key={r.key}
            onClick={() => setPicked(r.key)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              height: 30,
              padding: '0 10px',
              cursor: 'pointer',
              background: picked === r.key ? 'var(--blue-100)' : 'transparent',
              borderBottom: '1px solid var(--gray-100)',
              boxShadow: picked === r.key ? 'inset 3px 0 0 var(--navy-500)' : undefined,
            }}
          >
            <span
              style={{
                width: 20,
                textAlign: 'right',
                color: 'var(--gray-500)',
                fontVariantNumeric: 'tabular-nums',
              }}
            >
              {idx + 1}
            </span>
            <span style={{ fontWeight: picked === r.key ? 600 : 400 }}>{r.label}</span>
            {showCount && (
              <span className="hint" style={{ marginLeft: 'auto' }}>
                대상자 {r.voucherStudents}명
              </span>
            )}
          </div>
        ))}
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        <Button small disabled={i <= 0} onClick={() => move(-1)}>
          ↑ 위로
        </Button>
        <Button small disabled={i < 0 || i >= order.length - 1} onClick={() => move(1)}>
          ↓ 아래로
        </Button>
      </div>
    </div>
  )
}
