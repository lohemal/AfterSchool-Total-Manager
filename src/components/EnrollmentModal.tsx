/**
 * 수강 추가 / 수정 팝업 (요구사항 §15).
 *
 * 추가는 학생과 부서를 고르고, 부서에 설정된 기준 수강료를 **미리 보여 준 뒤**
 * 저장한다. 수정은 금액만 고친다 — 학생과 부서는 식별정보이므로 바꾸지 않는다
 * (부서를 옮기려면 취소하고 새로 등록한다).
 */

import { useMutation, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

import { api, errorMessage } from '@/ipc/api'
import type { Department, Enrollment, Fee } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { Modal } from './Modal'
import { StudentPicker } from './StudentPicker'
import { useToast } from './Toast'
import { Button, Field, Input, MoneyInput, Notice, Select } from './ui'

export function EnrollmentModal({
  value,
  departments,
  onClose,
  onSaved,
}: {
  /** null이면 새로 추가 */
  value: Enrollment | null
  departments: Department[]
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const items = app.boot.costItems

  const [studentId, setStudentId] = useState<number | null>(value?.studentId ?? null)
  const [departmentId, setDepartmentId] = useState<number | null>(value?.departmentId ?? null)
  const [fees, setFees] = useState<Fee[]>(
    value
      ? items.map((it) => ({
          itemCode: it.code,
          amount: value.fees.find((f) => f.itemCode === it.code)?.amount ?? 0,
        }))
      : items.map((it) => ({ itemCode: it.code, amount: 0 })),
  )
  const [reason, setReason] = useState('')
  const [touched, setTouched] = useState(false)

  // 부서를 고르면 그 부서의 기준 수강료를 불러와 보여 준다.
  const base = useQuery({
    queryKey: ['dept-base-fees', departmentId],
    queryFn: () => api.departmentBaseFees(departmentId!),
    enabled: departmentId !== null,
  })

  useEffect(() => {
    // 새로 추가할 때만 기준금액을 채운다. 손으로 고친 뒤에는 덮지 않는다.
    if (value || !base.data || touched) return
    setFees(
      items.map((it) => ({
        itemCode: it.code,
        amount: base.data.find((f) => f.itemCode === it.code)?.amount ?? 0,
      })),
    )
  }, [base.data, items, value, touched])

  const total = fees.reduce((s, f) => s + f.amount, 0)
  const baseTotal = (base.data ?? []).reduce((s, f) => s + f.amount, 0)
  const differs = departmentId !== null && base.data !== undefined && total !== baseTotal

  const save = useMutation({
    mutationFn: async () => {
      if (value) {
        await api.enrollmentUpdateFees(value.id, fees, reason)
        return
      }
      if (!studentId) throw { code: 'INVALID', message: '학생을 골라 주세요.' }
      if (!departmentId) throw { code: 'INVALID', message: '부서를 골라 주세요.' }
      await api.enrollmentCreate(app.workspaceId!, {
        studentId,
        departmentId,
        fees,
        reason: reason || undefined,
      })
    },
    onSuccess: () => {
      toast.ok(value ? '금액을 저장했습니다.' : '수강을 등록했습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function setFee(code: string, amount: number) {
    setTouched(true)
    setFees((prev) => prev.map((f) => (f.itemCode === code ? { ...f, amount } : f)))
  }

  function resetToBase() {
    if (!base.data) return
    setTouched(true)
    setFees(
      items.map((it) => ({
        itemCode: it.code,
        amount: base.data!.find((f) => f.itemCode === it.code)?.amount ?? 0,
      })),
    )
  }

  return (
    <Modal
      wide
      title={value ? '수강 금액 수정' : '수강 추가'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending}>
            {save.isPending ? '저장 중…' : '저장'}
          </Button>
        </>
      }
    >
      {value ? (
        <Notice tone="info">
          {value.grade}학년 {value.classNo}반 {value.studentNo}번 <b>{value.name}</b> ·{' '}
          <b>{value.deptLabel}</b>
          <div className="hint" style={{ marginTop: 4 }}>
            학생과 부서는 바꿀 수 없습니다. 부서를 옮기려면 이 수강을 취소하고 새로 등록해 주세요.
          </div>
        </Notice>
      ) : (
        <>
          <StudentPicker value={studentId} onChange={(id) => setStudentId(id)} />
          <div style={{ height: 14 }} />
          <Field label="부서" hint="현재 작업공간에 등록된 부서만 나옵니다.">
            <Select
              value={departmentId ?? ''}
              onChange={(e) => {
                setDepartmentId(e.target.value ? Number(e.target.value) : null)
                setTouched(false)
              }}
              style={{ maxWidth: 320 }}
            >
              <option value="">선택</option>
              {departments.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                  {d.className ? ` ${d.className}` : ''} — {won(d.total)}원
                </option>
              ))}
            </Select>
          </Field>
        </>
      )}

      <div style={{ height: 16 }} />
      <div className="card__head" style={{ padding: 0, border: 'none', marginBottom: 8 }}>
        <span className="card__title">금액</span>
        {base.data && (
          <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8 }}>
            <span className="hint">부서 기준 {won(baseTotal)}원</span>
            <Button small onClick={resetToBase} disabled={!differs}>
              기준금액으로
            </Button>
          </div>
        )}
      </div>

      <div className="grid4">
        {items.map((it) => (
          <Field key={it.code} label={it.name}>
            <MoneyInput
              value={fees.find((f) => f.itemCode === it.code)?.amount ?? 0}
              onValue={(n) => setFee(it.code, n)}
            />
          </Field>
        ))}
      </div>

      <div
        style={{
          marginTop: 14,
          display: 'flex',
          justifyContent: 'flex-end',
          alignItems: 'baseline',
          gap: 8,
        }}
      >
        <span className="field__label">합계</span>
        <span
          style={{
            fontSize: 18,
            fontWeight: 700,
            color: 'var(--navy-800)',
            fontVariantNumeric: 'tabular-nums',
          }}
        >
          {won(total)}
        </span>
        <span className="hint">원</span>
      </div>

      {differs && (
        <div style={{ marginTop: 10 }}>
          <Notice tone="warn">
            부서 기준금액({won(baseTotal)}원)과 다릅니다. 이 학생에게만 적용되며 부서정보는
            바뀌지 않습니다.
          </Notice>
        </div>
      )}

      <div style={{ height: 12 }} />
      <Field label="변경사유" hint="변경이력에 남습니다.">
        <Input
          value={reason}
          placeholder={value ? '교재 자체 준비 · 형제 할인 등' : '선택 입력'}
          onChange={(e) => setReason(e.target.value)}
        />
      </Field>
    </Modal>
  )
}
