/**
 * 전출 처리 (v0.1.8).
 *
 * ## 이 창은 수강이나 금액을 건드리지 않는다
 *
 * 전출은 **학생이 학교를 떠났다는 사실**일 뿐이다. 수강료를 얼마로 확정할지는
 * 담당자가 실제 상황을 보고 [추가·취소 관리]에서 정해야 한다. 그래서 전출일이
 * 이미 지났는데 수강이 남아 있으면 여기서 막고, 자동으로 취소하지 않는다.
 *
 * 전출 **예정**은 수강이 있어도 등록된다 — 그날까지는 정상으로 수업을 듣는다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'

import { api, errorMessage } from '@/ipc/api'
import type { Student } from '@/ipc/types'
import { ENROLMENT_TEXT } from '@/lib/transfer'

import { Modal } from './Modal'
import { useToast } from './Toast'
import { Button, Field, Input, Notice } from './ui'

export function TransferModal({
  student,
  onClose,
  onSaved,
}: {
  student: Student
  onClose: () => void
  onSaved: () => void
}) {
  const toast = useToast()
  const qc = useQueryClient()
  const nav = useNavigate()
  const [date, setDate] = useState(student.transferOutDate ?? '')

  // 무엇이 걸려 있는지 미리 보여 준다. 읽기만 하는 조회다.
  const check = useQuery({
    queryKey: ['transfer-check', student.id, date],
    queryFn: () => api.studentTransferCheck(student.id, date || null),
  })

  const save = useMutation({
    mutationFn: () => api.studentTransferSet(student.id, date.trim() || null),
    onSuccess: () => {
      toast.ok(
        date.trim()
          ? `전출일을 ${date.trim()} 로 저장했습니다. 지난 수강·금액·정산 기록은 그대로 남습니다.`
          : '전출 정보를 지웠습니다. 다시 재학 상태입니다.',
      )
      void qc.invalidateQueries()
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const c = check.data
  const 막힘 = c?.blocked ?? false

  return (
    <Modal
      title="전출 처리"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          {student.transferOutDate && (
            <Button
              onClick={() => {
                setDate('')
                save.mutate()
              }}
              disabled={save.isPending}
            >
              전출 정보 해제
            </Button>
          )}
          <Button
            variant="primary"
            onClick={() => save.mutate()}
            disabled={save.isPending || 막힘 || !date.trim()}
          >
            {save.isPending ? '저장 중…' : '저장'}
          </Button>
        </>
      }
    >
      <div className="formRow">
        <label className="field__label">학생</label>
        <div>
          <b>
            {student.grade}학년 {student.classNo}반 {student.studentNo}번 {student.name}
          </b>
          <span className="muted"> · 지금 {ENROLMENT_TEXT[student.enrolment]}</span>
        </div>
      </div>

      <Field label="전출일">
        <Input
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          style={{ width: 170 }}
        />
      </Field>
      <div className="hint" style={{ marginBottom: 10 }}>
        앞날로 적으면 <b>전출 예정</b>이 되고, 그날까지는 재학생으로 다룹니다. 오늘이거나 지난
        날짜면 <b>전출</b>입니다.
      </div>

      {c && c.activeCount > 0 && (
        <Notice tone={막힘 ? 'warn' : 'info'}>
          {막힘 ? (
            <>
              <b>전출일이 지났는데 수강 중인 프로그램이 {c.activeCount}건 있습니다.</b>
              <div style={{ marginTop: 4 }}>{c.activeDepartments.join(' · ')}</div>
              <div style={{ marginTop: 6 }}>
                전출 처리 전에 수강 취소를 완료하여 <b>최종 수강료를 확정</b>해 주세요. 수강료는
                자동으로 정해지지 않습니다.
              </div>
              <div style={{ marginTop: 8 }}>
                <Button
                  small
                  onClick={() => {
                    onClose()
                    nav('/adjustments')
                  }}
                >
                  수강 취소하러 가기
                </Button>
              </div>
            </>
          ) : (
            <>
              지금 수강 중인 프로그램이 {c.activeCount}건 있습니다 (
              {c.activeDepartments.join(' · ')}). 전출일까지는 정상 수강으로 두고, 마지막 수강
              시점에 [추가·취소 관리]에서 취소하여 금액을 확정해 주세요.
            </>
          )}
        </Notice>
      )}

      {c && (c.pastEnrollments > 0 || c.activeCount > 0) && (
        <div className="hint" style={{ marginTop: 8 }}>
          전출해도 지난 수강 {c.pastEnrollments + c.activeCount}건과 금액·정산 기록은 <b>그대로
          남습니다.</b> 기록을 지우려고 학생을 삭제하지 마세요.
        </div>
      )}
    </Modal>
  )
}
