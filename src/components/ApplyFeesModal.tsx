/**
 * 부서 기준금액을 기존 수강생에게 반영한다 (요구사항 §11).
 *
 * 자동으로 덮어쓰지 않는 이유는 학생마다 따로 조정해 둔 금액이 있기 때문이다.
 * 그래서 이 창은 **무엇이 어떻게 바뀌는지 먼저 보여 주고**, 방식을 고르게 한다.
 *
 *   · 학생별 수정값 유지 (권장) — 손으로 고친 칸은 그대로 둔다
 *   · 전체 반영              — 학생별 수정까지 기준금액으로 맞춘다
 *   · 고른 항목만 반영        — 아래 표에서 체크한 칸만 바꾼다
 *
 * 목록에는 **실제로 바뀔 칸만** 나온다. 같은 값은 애초에 보이지 않는다.
 */

import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'

import { api, errorMessage } from '@/ipc/api'
import type { ApplyMode, Department, FeeDiff } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { todayISO } from './AdjustmentToggle'
import { DeltaChoice, deltaChoiceReady, type DeltaChoiceValue } from './DeltaChoice'
import { Modal } from './Modal'
import { useToast } from './Toast'
import { Button, Empty, Field, Input, Notice } from './ui'

const MODES: { code: ApplyMode; label: string; desc: string }[] = [
  {
    code: 'KEEP_EDITED',
    label: '학생별 수정값 유지 (권장)',
    desc: '손으로 고쳐 둔 금액은 그대로 두고 나머지만 기준금액으로 맞춥니다.',
  },
  {
    code: 'ALL',
    label: '전체 반영',
    desc: '학생별로 고친 금액까지 모두 부서 기준금액으로 맞춥니다.',
  },
  {
    code: 'SELECTED',
    label: '고른 항목만 반영',
    desc: '아래 표에서 체크한 칸만 바꿉니다.',
  },
]

function key(d: FeeDiff) {
  return `${d.enrollmentId}:${d.itemCode}`
}

export function ApplyFeesModal({
  department,
  onClose,
  onApplied,
}: {
  /** null이면 이 작업공간의 모든 부서 */
  department: Department | null
  onClose: () => void
  onApplied: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [mode, setMode] = useState<ApplyMode>('KEEP_EDITED')
  const [picked, setPicked] = useState<string[]>([])
  const [reason, setReason] = useState('')

  const diff = useQuery({
    queryKey: ['fee-diff', app.workspaceId, department?.id ?? null],
    queryFn: () => api.enrollmentFeeDiff(app.workspaceId!, department?.id ?? null),
    enabled: app.workspaceId !== null,
  })

  const rows = diff.data?.rows ?? []
  const overridden = diff.data?.overridden ?? 0

  const targets = rows.filter((d) => {
    if (mode === 'ALL') return true
    if (mode === 'KEEP_EDITED') return !d.isOverridden
    return picked.includes(key(d))
  })

  // 차액 추가징수·환불 (v0.1.9). **한 부서**를 고른 때만 묻는다 — 묶음은 부서
  // 하나에 딸린다. 반영하면 그 칸은 기준금액이 되므로 차액은 `기준 - 지금` 이다.
  const [delta, setDelta] = useState<DeltaChoiceValue>({
    register: false,
    occurredOn: todayISO(),
    note: '',
  })
  const canBatch = department !== null
  const refund = targets.filter((d) => d.base < d.current).reduce((s, d) => s + d.current - d.base, 0)
  const additional = targets
    .filter((d) => d.base > d.current)
    .reduce((s, d) => s + d.base - d.current, 0)
  const students = new Set(targets.map((d) => d.enrollmentId)).size
  const registering = canBatch && delta.register

  const apply = useMutation({
    mutationFn: async () => {
      if (registering) {
        // 금액 반영과 차액 기록이 한 트랜잭션이다. 고른 칸을 그대로 넘긴다 —
        // 서버가 지금 실제로 차이가 있는 칸(수강중)만 다시 골라 쓴다.
        const r = await api.adjustmentBatchCreate(
          app.workspaceId!,
          department!.id,
          targets.map((d) => ({ enrollmentId: d.enrollmentId, itemCode: d.itemCode })),
          { register: true, occurredOn: delta.occurredOn || null, note: delta.note },
        )
        return { kind: 'batch' as const, r }
      }
      const r = await api.enrollmentApplyFees({
        workspaceId: app.workspaceId!,
        departmentId: department?.id ?? null,
        mode,
        picks:
          mode === 'SELECTED'
            ? rows
                .filter((d) => picked.includes(key(d)))
                .map((d) => ({ enrollmentId: d.enrollmentId, itemCode: d.itemCode }))
            : [],
        reason,
      })
      return { kind: 'apply' as const, r }
    },
    onSuccess: (out) => {
      if (out.kind === 'batch') {
        const r = out.r
        const parts: string[] = []
        if (r.refunds > 0) parts.push(`환불 ${r.refunds}건 ${won(r.refundTotal)}원`)
        if (r.additionals > 0) parts.push(`추가징수 ${r.additionals}건 ${won(r.additionalTotal)}원`)
        toast.ok(
          `${r.changed}개 항목을 반영하고 ${parts.join(' · ') || '차액'}을 한 묶음으로 등록했습니다.`,
        )
      } else {
        const r = out.r
        toast.ok(
          `${r.enrollments}건의 수강에서 ${r.changed}개 항목을 반영했습니다.` +
            (r.kept > 0 ? ` 학생별 수정 ${r.kept}개는 그대로 두었습니다.` : ''),
        )
      }
      onApplied()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const title = department
    ? `부서금액 반영 — ${department.name}${department.className}`
    : '부서금액 반영 — 전체 부서'

  return (
    <Modal
      wide
      title={title}
      onClose={onClose}
      footer={
        <>
          <span className="hint" style={{ marginRight: 'auto' }}>
            {targets.length > 0
              ? `${targets.length}개 항목이 바뀝니다.`
              : '바뀌는 항목이 없습니다.'}
          </span>
          <Button onClick={onClose}>취소</Button>
          {/* 반영 중에는 잠근다. 서버도 지금 값을 기준으로 다시 셈하므로, 같은
              요청이 두 번 가도 두 번째에는 고칠 칸이 없어 묶음이 하나만 생긴다. */}
          <Button
            variant="primary"
            onClick={() => apply.mutate()}
            disabled={
              apply.isPending ||
              targets.length === 0 ||
              (registering && !deltaChoiceReady(delta))
            }
          >
            {apply.isPending
              ? '반영 중…'
              : registering
                ? `${students}명 반영하고 등록`
                : `${targets.length}개 반영`}
          </Button>
        </>
      }
    >
      {diff.isLoading && <div className="hint">확인하는 중…</div>}

      {diff.data && rows.length === 0 && (
        <Empty title="반영할 것이 없습니다">
          수강생의 금액이 이미 부서 기준금액과 같습니다.
        </Empty>
      )}

      {rows.length > 0 && (
        <>
          <Notice tone="warn">
            아래 <b>{rows.length}개</b> 항목이 부서 기준금액과 다릅니다.
            {overridden > 0 && (
              <>
                {' '}
                이 가운데 <b>{overridden}개</b>는 학생별로 따로 고쳐 둔 금액입니다.
              </>
            )}
          </Notice>

          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, marginBottom: 12 }}>
            {MODES.map((m) => (
              <label
                key={m.code}
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 8,
                  padding: '8px 10px',
                  border: '1px solid',
                  borderColor: mode === m.code ? 'var(--navy-500)' : 'var(--gray-200)',
                  background: mode === m.code ? 'var(--blue-50)' : 'var(--white)',
                  borderRadius: 'var(--radius)',
                  cursor: 'pointer',
                }}
              >
                <input
                  type="radio"
                  name="apply-mode"
                  checked={mode === m.code}
                  onChange={() => setMode(m.code)}
                  style={{ marginTop: 2, accentColor: 'var(--navy-500)' }}
                />
                <span>
                  <b>{m.label}</b>
                  <div className="hint">{m.desc}</div>
                </span>
              </label>
            ))}
          </div>

          <div className="tableWrap" style={{ maxHeight: 300, border: '1px solid var(--gray-200)' }}>
            <table className="table">
              <thead>
                <tr>
                  {mode === 'SELECTED' && (
                    <th className="check">
                      <input
                        type="checkbox"
                        aria-label="전체 선택"
                        checked={picked.length === rows.length && rows.length > 0}
                        onChange={(e) => setPicked(e.target.checked ? rows.map(key) : [])}
                      />
                    </th>
                  )}
                  <th style={{ width: 130 }}>부서</th>
                  <th style={{ width: 150 }}>학생</th>
                  <th style={{ width: 80 }}>항목</th>
                  <th className="num" style={{ width: 92 }}>
                    지금
                  </th>
                  <th className="num" style={{ width: 92 }}>
                    기준금액
                  </th>
                  <th className="num" style={{ width: 92 }}>
                    차액
                  </th>
                  <th style={{ width: 80 }}>구분</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((d) => {
                  const included = targets.includes(d)
                  return (
                    <tr
                      key={key(d)}
                      style={{ opacity: included ? 1 : 0.45 }}
                      className={included ? undefined : undefined}
                    >
                      {mode === 'SELECTED' && (
                        <td className="check">
                          <input
                            type="checkbox"
                            aria-label="선택"
                            checked={picked.includes(key(d))}
                            onChange={(e) =>
                              setPicked((prev) =>
                                e.target.checked
                                  ? [...prev, key(d)]
                                  : prev.filter((k) => k !== key(d)),
                              )
                            }
                          />
                        </td>
                      )}
                      <td>{d.deptLabel}</td>
                      <td>
                        {d.grade}-{d.classNo}-{d.studentNo} {d.name}
                      </td>
                      <td>{d.itemName}</td>
                      <td className="num">{won(d.current)}</td>
                      <td className="num">
                        <b style={{ color: included ? 'var(--navy-700)' : undefined }}>
                          {won(d.base)}
                        </b>
                      </td>
                      {/* 반영하면 이 칸은 기준금액이 된다. 줄면 환불, 늘면 추가징수. */}
                      <td className="num">
                        {d.base - d.current > 0 ? '+' : ''}
                        {won(d.base - d.current)}
                      </td>
                      <td>
                        {d.isOverridden ? (
                          <span className="tag tag--warn">학생별 수정</span>
                        ) : (
                          <span className="muted">기본</span>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>

          {canBatch && targets.length > 0 && (
            <>
              <div className="hint" style={{ marginTop: 10 }}>
                반영 대상 <b>{students}명</b> · 수강중인 학생만입니다. 취소한 학생은 들어가지
                않습니다.
                {!registering && ' 추가징수·환불로 등록하면 학생별로 뺄 수 있습니다.'}
              </div>
              <DeltaChoice
                value={delta}
                onChange={(v) => {
                  // 등록으로 바꾸면 지금 대상을 모두 골라 둔 채 '골라서 반영'으로 옮긴다.
                  // 그러면 사람이 뺄 학생만 체크를 풀면 된다.
                  if (v.register && !delta.register) {
                    setPicked(targets.map(key))
                    setMode('SELECTED')
                  }
                  setDelta(v)
                }}
                refund={refund}
                additional={additional}
              />
            </>
          )}

          {!registering && (
            <div style={{ marginTop: 12 }}>
              <Field label="변경사유" hint="바뀐 수강마다 변경이력에 남습니다.">
                <Input
                  value={reason}
                  placeholder="5월 단가 조정 등"
                  onChange={(e) => setReason(e.target.value)}
                />
              </Field>
            </div>
          )}
        </>
      )}
    </Modal>
  )
}
