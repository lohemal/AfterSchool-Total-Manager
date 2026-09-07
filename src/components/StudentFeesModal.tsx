/**
 * 부서정보 › [학생별 수정] (요구사항 §12).
 *
 * 같은 부서를 들어도 교재비·재료비가 학생마다 다르거나 아예 부과되지 않는 경우가
 * 있다. 여기서 고친 값은 **그 학생의 `charge`만** 바꾸고 부서 기준금액은 건드리지
 * 않는다.
 */

import { useMutation, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

import { api, errorMessage } from '@/ipc/api'
import type { Department, Fee, StudentFeeEdit } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { Modal } from './Modal'
import { useToast } from './Toast'
import { Button, Empty, Field, Input, MoneyInput, Notice } from './ui'

interface Draft {
  enrollmentId: number
  grade: number
  classNo: string
  studentNo: number
  name: string
  hasOverride: boolean
  fees: Record<string, number>
  original: Record<string, number>
}

export function StudentFeesModal({
  department,
  onClose,
  onSaved,
}: {
  department: Department
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const items = app.boot.costItems

  const [drafts, setDrafts] = useState<Draft[]>([])
  const [reason, setReason] = useState('')

  const rows = useQuery({
    queryKey: ['enrollments-by-dept', app.workspaceId, department.id],
    queryFn: () => api.enrollmentByDepartment(app.workspaceId!, department.id),
    enabled: app.workspaceId !== null,
  })

  useEffect(() => {
    if (!rows.data) return
    setDrafts(
      rows.data.map((e) => {
        const map: Record<string, number> = {}
        for (const it of items) {
          map[it.code] = e.fees.find((f) => f.itemCode === it.code)?.amount ?? 0
        }
        return {
          enrollmentId: e.id,
          grade: e.grade,
          classNo: e.classNo,
          studentNo: e.studentNo,
          name: e.name,
          hasOverride: e.hasOverride,
          fees: { ...map },
          original: { ...map },
        }
      }),
    )
  }, [rows.data, items])

  const dirty = drafts.filter((d) => items.some((it) => d.fees[it.code] !== d.original[it.code]))

  const save = useMutation({
    mutationFn: async () => {
      const edits: StudentFeeEdit[] = dirty.map((d) => ({
        enrollmentId: d.enrollmentId,
        fees: items.map<Fee>((it) => ({ itemCode: it.code, amount: d.fees[it.code] ?? 0 })),
      }))
      return api.enrollmentSaveStudentFees(edits, reason)
    },
    onSuccess: (n) => {
      toast.ok(`${n}명의 금액을 저장했습니다.`)
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function setFee(enrollmentId: number, code: string, amount: number) {
    setDrafts((prev) =>
      prev.map((d) =>
        d.enrollmentId === enrollmentId ? { ...d, fees: { ...d.fees, [code]: amount } } : d,
      ),
    )
  }

  const baseOf = (code: string) =>
    department.fees.find((f) => f.itemCode === code)?.amount ?? 0

  return (
    <Modal
      wide
      title={`학생별 금액 수정 — ${department.name}${department.className}`}
      onClose={onClose}
      footer={
        <>
          <span className="hint" style={{ marginRight: 'auto' }}>
            {dirty.length > 0 ? `${dirty.length}명의 금액이 바뀌었습니다.` : '바뀐 금액이 없습니다.'}
          </span>
          <Button onClick={onClose}>취소</Button>
          <Button
            variant="primary"
            onClick={() => save.mutate()}
            disabled={save.isPending || dirty.length === 0}
          >
            {save.isPending ? '저장 중…' : '저장'}
          </Button>
        </>
      }
    >
      <Notice tone="info">
        부서 기준금액{' '}
        {items.map((it) => (
          <span key={it.code} style={{ marginRight: 10 }}>
            {it.name} <b>{won(baseOf(it.code))}</b>
          </span>
        ))}
        <div className="hint" style={{ marginTop: 4 }}>
          여기서 고친 값은 그 학생에게만 적용됩니다. 부서 기준금액은 바뀌지 않습니다.
        </div>
      </Notice>

      {rows.isLoading && <div className="hint">불러오는 중…</div>}

      {rows.data && rows.data.length === 0 && (
        <Empty title="이 부서를 수강하는 학생이 없습니다">
          [수강생 명단]에서 먼저 수강을 등록해 주세요.
        </Empty>
      )}

      {drafts.length > 0 && (
        <div className="tableWrap" style={{ maxHeight: 380, border: '1px solid var(--gray-200)' }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 54 }}>학년</th>
                <th style={{ width: 54 }}>반</th>
                <th style={{ width: 54 }}>번호</th>
                <th style={{ width: 96 }}>이름</th>
                {items.map((it) => (
                  <th key={it.code} className="num" style={{ width: 104 }}>
                    {it.name}
                  </th>
                ))}
                <th className="num" style={{ width: 100 }}>
                  합계
                </th>
              </tr>
            </thead>
            <tbody>
              {drafts.map((d) => {
                const total = items.reduce((s, it) => s + (d.fees[it.code] ?? 0), 0)
                const changed = items.some((it) => d.fees[it.code] !== d.original[it.code])
                return (
                  <tr key={d.enrollmentId} className={changed ? 'on' : undefined}>
                    <td>{d.grade}</td>
                    <td>{d.classNo}</td>
                    <td>{d.studentNo}</td>
                    <td>
                      {d.name}
                      {d.hasOverride && (
                        <span className="tag tag--warn" style={{ marginLeft: 4 }}>
                          수정됨
                        </span>
                      )}
                    </td>
                    {items.map((it) => (
                      <td key={it.code} className="num" style={{ padding: '2px 6px' }}>
                        <MoneyInput
                          value={d.fees[it.code] ?? 0}
                          onValue={(n) => setFee(d.enrollmentId, it.code, n)}
                          style={{ height: 26, width: '100%' }}
                        />
                      </td>
                    ))}
                    <td className="num">
                      <b>{won(total)}</b>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}

      {drafts.length > 0 && (
        <div style={{ marginTop: 12 }}>
          <Field label="변경사유" hint="바뀐 학생마다 변경이력에 남습니다.">
            <Input
              value={reason}
              placeholder="교재 자체 준비 · 재료 지참 등"
              onChange={(e) => setReason(e.target.value)}
            />
          </Field>
        </div>
      )}
    </Modal>
  )
}
