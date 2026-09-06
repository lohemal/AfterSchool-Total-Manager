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

  const apply = useMutation({
    mutationFn: () =>
      api.enrollmentApplyFees({
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
      }),
    onSuccess: (r) => {
      toast.ok(
        `${r.enrollments}건의 수강에서 ${r.changed}개 항목을 반영했습니다.` +
          (r.kept > 0 ? ` 학생별 수정 ${r.kept}개는 그대로 두었습니다.` : ''),
      )
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
          <Button
            variant="primary"
            onClick={() => apply.mutate()}
            disabled={apply.isPending || targets.length === 0}
          >
            {apply.isPending ? '반영 중…' : `${targets.length}개 반영`}
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

          <div style={{ marginTop: 12 }}>
            <Field label="변경사유" hint="바뀐 수강마다 변경이력에 남습니다.">
              <Input
                value={reason}
                placeholder="5월 단가 조정 등"
                onChange={(e) => setReason(e.target.value)}
              />
            </Field>
          </div>
        </>
      )}
    </Modal>
  )
}
