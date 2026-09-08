/**
 * 부서별 학생 금액 Excel 일괄 수정 (v0.1.4).
 *
 * [금액 수정 양식 받기] → 파일에서 금액 수정 → [금액 수정 파일 불러오기] →
 * 미리보기 확인 → [변경사항 반영].
 *
 * **파일을 불러오는 것만으로는 아무것도 바뀌지 않는다.** 무엇이 어떻게 바뀌는지
 * 먼저 보여 주고, 사람이 반영을 눌렀을 때만 저장한다.
 *
 * 오류가 한 줄이라도 있으면 반영 자체를 막는다 — 절반만 들어간 자료를 되돌리는
 * 것이 더 어렵기 때문이다. 오류 줄을 고쳐 다시 불러오면 된다.
 */

import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'

import { api, errorMessage } from '@/ipc/api'
import type { Department, FeePreview } from '@/ipc/types'
import { won } from '@/lib/format'

import { Modal } from './Modal'
import { useToast } from './Toast'
import { Button, Field, Input, Notice } from './ui'

export function DeptFeeExcel({
  workspaceId,
  dept,
  onDone,
}: {
  workspaceId: number
  /** 고른 부서. 없으면 두 버튼이 잠긴다 */
  dept: Department | undefined
  onDone: () => void
}) {
  const toast = useToast()
  const [preview, setPreview] = useState<FeePreview | null>(null)
  const [reason, setReason] = useState('')
  const [busy, setBusy] = useState(false)

  const label = dept ? dept.name + dept.className : ''

  async function template() {
    if (!dept) return
    try {
      const r = await api.deptFeeTemplate(workspaceId, dept.id)
      toast.ok(`${r.name} (수강생 ${r.rows}명) 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    } catch (e) {
      toast.bad(errorMessage(e))
    }
  }

  async function pick() {
    if (!dept) return
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: 'Excel 파일', extensions: ['xlsx', 'xls', 'xlsm'] }],
      })
      if (typeof path !== 'string') return
      setBusy(true)
      const p = await api.deptFeePreview(workspaceId, dept.id, path)
      setReason('')
      setPreview(p)
    } catch (e) {
      toast.bad(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  async function apply() {
    if (!preview?.token) return
    try {
      setBusy(true)
      const r = await api.deptFeeApply(workspaceId, preview.token, reason)
      setPreview(null)
      toast.ok(`학생 ${r.students}명 · ${r.cells}칸의 금액을 고쳤습니다.`)
      onDone()
    } catch (e) {
      toast.bad(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <Button
        small
        disabled={!dept}
        title={dept ? `${label} 수강생의 지금 금액을 Excel로 받습니다` : '부서를 먼저 고르세요'}
        onClick={template}
      >
        금액 수정 양식 받기
      </Button>
      <Button
        small
        disabled={!dept || busy}
        title={dept ? `${label} 수강생의 금액을 Excel로 고칩니다` : '부서를 먼저 고르세요'}
        onClick={pick}
      >
        {busy && !preview ? '읽는 중…' : '금액 수정 파일 불러오기'}
      </Button>

      {preview && (
        <PreviewModal
          preview={preview}
          reason={reason}
          setReason={setReason}
          busy={busy}
          onApply={apply}
          onClose={() => setPreview(null)}
        />
      )}
    </>
  )
}

function PreviewModal({
  preview: p,
  reason,
  setReason,
  busy,
  onApply,
  onClose,
}: {
  preview: FeePreview
  reason: string
  setReason: (v: string) => void
  busy: boolean
  onApply: () => void
  onClose: () => void
}) {
  const blocked = p.errors.length > 0
  const nothing = !blocked && p.students === 0
  const canApply = !!p.token && reason.trim().length > 0 && !busy

  return (
    <Modal
      wide
      title={`금액 수정 미리보기 — ${p.deptLabel}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>닫기</Button>
          <Button variant="primary" disabled={!canApply} onClick={onApply}>
            {busy ? '반영 중…' : '변경사항 반영'}
          </Button>
        </>
      }
    >
      <div className="hint" style={{ marginBottom: 10 }}>
        {p.fileName} — 자료 <b>{p.total}</b>줄
      </div>

      <div className="toolbar__note" style={{ marginBottom: 10 }}>
        바뀔 학생 <b>{p.students}명</b> · 바뀔 항목 <b>{p.cells}칸</b> · 변경 없음{' '}
        <b>{p.unchanged}명</b> · 매칭 실패 <b>{p.unmatched.length}줄</b> · 오류{' '}
        <b>{p.errors.length}줄</b>
      </div>

      {blocked && (
        <Notice tone="bad">
          오류가 있어 <b>아무것도 반영하지 않습니다.</b> 아래 줄을 고쳐 파일을 다시 불러와
          주세요. 절반만 반영되면 되돌리기가 더 어렵기 때문입니다.
        </Notice>
      )}

      {nothing && (
        <Notice tone="info">
          파일의 금액이 지금 금액과 같습니다. <b>고칠 것이 없어 아무것도 바꾸지 않습니다.</b>
        </Notice>
      )}

      {p.errors.length > 0 && (
        <IssueTable title="오류 — 이 줄 때문에 반영이 막혔습니다" rows={p.errors} />
      )}
      {p.unmatched.length > 0 && (
        <IssueTable
          title="매칭 실패 — 이 부서에서 찾지 못한 학생입니다"
          rows={p.unmatched}
          note="학년·반·번호·이름이 모두 맞아야 짝을 짓습니다. 프로그램이 비슷한 학생을 골라 주지 않습니다."
        />
      )}

      {p.changes.length > 0 && (
        <>
          <div style={{ fontWeight: 700, margin: '14px 0 6px' }}>바뀌는 금액</div>
          <div className="tableWrap" style={{ maxHeight: 320 }}>
            <table className="table">
              <thead>
                <tr>
                  <th className="left" style={{ width: 220 }}>
                    학생
                  </th>
                  <th style={{ width: 90 }}>항목</th>
                  <th className="num" style={{ width: 110 }}>
                    현재
                  </th>
                  <th style={{ width: 34 }} />
                  <th className="num" style={{ width: 110 }}>
                    변경
                  </th>
                </tr>
              </thead>
              <tbody>
                {p.changes.map((c) => (
                  <tr key={`${c.enrollmentId}-${c.itemCode}`}>
                    <td className="left">{c.studentLabel}</td>
                    <td>{c.itemName}</td>
                    <td className="num">{won(c.before)}</td>
                    <td>→</td>
                    <td className="num">
                      <b style={{ color: c.after > c.before ? 'var(--red-600)' : 'var(--teal-600)' }}>
                        {won(c.after)}
                      </b>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {!blocked && p.students > 0 && (
        <div style={{ marginTop: 14 }}>
          <Field label="변경사유 (필수)" hint="변경 이력에 그대로 남습니다.">
            <Input
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="예) 4월 교재 배부분 반영"
              style={{ width: '100%' }}
            />
          </Field>
          <div className="hint" style={{ marginTop: 6 }}>
            반영하면 이 작업공간의 정산이 <b>재정산 필요</b>가 됩니다. 파일에 없는 학생과 다른
            부서는 그대로 있습니다.
          </div>
        </div>
      )}
    </Modal>
  )
}

function IssueTable({
  title,
  rows,
  note,
}: {
  title: string
  rows: { line: number; label: string; message: string }[]
  note?: string
}) {
  return (
    <>
      <div style={{ fontWeight: 700, margin: '14px 0 6px' }}>
        {title} <span className="muted">({rows.length}줄)</span>
      </div>
      {note && (
        <div className="hint" style={{ marginBottom: 6 }}>
          {note}
        </div>
      )}
      <div className="tableWrap" style={{ maxHeight: 220 }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 60 }}>행</th>
              <th className="left" style={{ width: 200 }}>
                파일에 적힌 학생
              </th>
              <th className="left">까닭</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={`${r.line}-${r.message}`}>
                <td>{r.line}</td>
                <td className="left">{r.label}</td>
                <td className="left">{r.message}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  )
}
