/**
 * 작업공간 — 월별·분기별·기수별 등 학교마다 다른 관리 단위 (요구사항 §4).
 *
 * **순서를 사람이 바꾸지 않는다.** 목록도 지원금 누적의 선행/후행 판단도 언제나
 * 시작일 오름차순(같으면 종료일 → 등록 순서)이다. 화면에 보이는 차례와 계산에
 * 쓰이는 차례가 갈릴 여지를 아예 없앤 것이다.
 *
 * 날짜가 곧 순서이므로, 기간이 겹치면 저장하기 전에 한 번 알려 준다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { DataTable, type Column } from '@/components/DataTable'
import { Confirm, Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Field, Input, Notice } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Workspace, WorkspaceInput } from '@/ipc/types'
import { currentSchoolYear, monthRange } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function WorkspacesPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()

  const [editing, setEditing] = useState<Workspace | 'new' | null>(null)
  const [removing, setRemoving] = useState<Workspace | null>(null)
  const [active, setActive] = useState<number | null>(app.workspaceId)

  const list = useQuery({
    queryKey: ['workspaces', app.yearId],
    queryFn: () => api.workspaceList(app.yearId),
  })

  const remove = useMutation({
    mutationFn: (id: number) => api.workspaceDelete(id),
    onSuccess: () => {
      toast.ok('작업공간을 지웠습니다.')
      setRemoving(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const use = useMutation({
    mutationFn: (id: number) => api.workspaceSetCurrent(id),
    onSuccess: () => {
      toast.ok('현재 작업공간을 바꿨습니다.')
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const rows = list.data ?? []

  // 목록이 이미 날짜 순이므로, 바로 앞 줄과 겹치는지만 보면 된다.
  const overlaps = new Set<number>()
  rows.forEach((w, i) => {
    const prev = rows[i - 1]
    if (prev && prev.endDate >= w.startDate) {
      overlaps.add(prev.id)
      overlaps.add(w.id)
    }
  })

  const columns: Column<Workspace>[] = [
    { key: 'no', head: '순서', width: 56, render: (_w, i) => i + 1 },
    {
      key: 'name',
      head: '작업공간명',
      align: 'left',
      render: (w) => (
        <>
          {w.name}
          {w.isCurrent && (
            <span className="tag tag--voucher" style={{ marginLeft: 6 }}>
              현재
            </span>
          )}
          {overlaps.has(w.id) && (
            <span className="tag tag--warn" style={{ marginLeft: 6 }}>
              기간 겹침
            </span>
          )}
        </>
      ),
    },
    { key: 'start', head: '시작일', width: 110, render: (w) => w.startDate },
    { key: 'end', head: '종료일', width: 110, render: (w) => w.endDate },
    { key: 'dept', head: '부서', align: 'num', width: 70, render: (w) => w.departmentCount },
    { key: 'enr', head: '수강', align: 'num', width: 70, render: (w) => w.enrollmentCount },
    { key: 'note', head: '비고', align: 'left', render: (w) => w.note || <span className="muted">—</span> },
    {
      key: 'act',
      head: '작업',
      width: 190,
      render: (w) => (
        <span style={{ display: 'inline-flex', gap: 4 }} onClick={(e) => e.stopPropagation()}>
          <Button small disabled={w.isCurrent} onClick={() => use.mutate(w.id)}>
            선택
          </Button>
          <Button small onClick={() => setEditing(w)}>
            수정
          </Button>
          <Button small variant="danger" onClick={() => setRemoving(w)}>
            삭제
          </Button>
        </span>
      ),
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">작업공간</h1>
          <p className="page__desc">
            2026년 4월 · 1분기 · 1기 · 여름방학처럼 학교가 쓰는 단위로 자유롭게 만듭니다.
          </p>
        </div>
        <div className="page__actions">
          <Button variant="primary" onClick={() => setEditing('new')}>
            작업공간 만들기
          </Button>
        </div>
      </div>

      <Notice tone="info">
        작업공간은 <b>시작일 순서</b>로 정렬됩니다. 지원금 누적에서 "앞선 작업공간"을
        정하는 기준도 같은 순서이므로, 화면에 보이는 차례가 곧 계산 차례입니다.
      </Notice>

      {overlaps.size > 0 && (
        <Notice tone="warn">
          운영기간이 겹치는 작업공간이 있습니다. 겹쳐도 계산은 되지만, 같은 기간의 수강이
          두 작업공간에 나뉘어 들어갈 수 있으니 날짜를 확인해 주세요.
        </Notice>
      )}

      <Card flush title={`${app.year?.name ?? ''} 작업공간`}>
        <DataTable
          rows={rows}
          columns={columns}
          getId={(w) => w.id}
          activeId={active}
          onRowClick={(w) => setActive(w.id)}
          empty={
            list.isLoading
              ? '불러오는 중…'
              : '작업공간이 없습니다. [작업공간 만들기]로 첫 번째를 만들어 주세요.'
          }
        />
      </Card>

      {editing && (
        <WorkspaceModal
          value={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {removing && (
        <Confirm
          danger
          title="작업공간 삭제"
          message={
            <>
              <b>{removing.name}</b>을(를) 지웁니다. 이 작업공간의 부서 {removing.departmentCount}개와
              수강 {removing.enrollmentCount}건, 정산 결과가 함께 사라집니다.
              <br />
              되돌릴 수 없습니다. 계속할까요?
            </>
          }
          busy={remove.isPending}
          onConfirm={() => remove.mutate(removing.id)}
          onClose={() => setRemoving(null)}
        />
      )}
    </div>
  )
}

function WorkspaceModal({
  value,
  onClose,
  onSaved,
}: {
  value: Workspace | null
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [confirmOverlap, setConfirmOverlap] = useState(false)
  const [form, setForm] = useState<WorkspaceInput>(() => {
    if (value) {
      return {
        name: value.name,
        startDate: value.startDate,
        endDate: value.endDate,
        note: value.note,
      }
    }
    const now = new Date()
    const [s, e] = monthRange(now.getFullYear(), now.getMonth() + 1)
    return { name: `${now.getFullYear()}년 ${now.getMonth() + 1}월`, startDate: s, endDate: e, note: '' }
  })

  const dateOk =
    /^\d{4}-\d{2}-\d{2}$/.test(form.startDate) &&
    /^\d{4}-\d{2}-\d{2}$/.test(form.endDate) &&
    form.startDate <= form.endDate

  // 날짜를 고칠 때마다 겹치는 작업공간을 미리 확인한다.
  const overlaps = useQuery({
    queryKey: ['ws-overlaps', app.yearId, value?.id ?? null, form.startDate, form.endDate],
    queryFn: () =>
      api.workspaceOverlaps(app.yearId, value?.id ?? null, form.startDate, form.endDate),
    enabled: dateOk,
  })
  const hits = overlaps.data ?? []

  const save = useMutation({
    mutationFn: async () => {
      if (value) await api.workspaceUpdate(value.id, form)
      else await api.workspaceCreate(app.yearId, form)
    },
    onSuccess: () => {
      toast.ok('저장되었습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function trySave() {
    if (hits.length > 0) setConfirmOverlap(true)
    else save.mutate()
  }

  function quick(kind: 'month' | 'term' | 'vacation') {
    const y = currentSchoolYear()
    if (kind === 'month') {
      const now = new Date()
      const [s, e] = monthRange(now.getFullYear(), now.getMonth() + 1)
      setForm({ ...form, name: `${now.getFullYear()}년 ${now.getMonth() + 1}월`, startDate: s, endDate: e })
    } else if (kind === 'term') {
      setForm({ ...form, name: '1학기', startDate: `${y}-03-01`, endDate: `${y}-08-31` })
    } else {
      setForm({ ...form, name: '여름방학', startDate: `${y}-07-20`, endDate: `${y}-08-20` })
    }
  }

  return (
    <>
      <Modal
        title={value ? '작업공간 수정' : '작업공간 만들기'}
        onClose={onClose}
        footer={
          <>
            <Button onClick={onClose}>취소</Button>
            <Button variant="primary" onClick={trySave} disabled={save.isPending || !dateOk}>
              {save.isPending ? '저장 중…' : '저장'}
            </Button>
          </>
        }
      >
        {!value && (
          <div style={{ display: 'flex', gap: 6, marginBottom: 14 }}>
            <span className="field__label" style={{ alignSelf: 'center' }}>
              빠른 입력
            </span>
            <Button small onClick={() => quick('month')}>
              이번 달
            </Button>
            <Button small onClick={() => quick('term')}>
              1학기
            </Button>
            <Button small onClick={() => quick('vacation')}>
              여름방학
            </Button>
          </div>
        )}

        <Field label="작업공간명" hint="2026년 4월 · 1분기 · 1기 · 여름방학 등 자유롭게">
          <Input
            autoFocus
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
        </Field>

        <div style={{ height: 12 }} />

        <div className="grid2">
          <Field label="시작일">
            <Input
              type="date"
              value={form.startDate}
              onChange={(e) => setForm({ ...form, startDate: e.target.value })}
            />
          </Field>
          <Field label="종료일">
            <Input
              type="date"
              value={form.endDate}
              onChange={(e) => setForm({ ...form, endDate: e.target.value })}
            />
          </Field>
        </div>

        {dateOk && form.startDate > form.endDate && (
          <div className="err" style={{ marginTop: 8 }}>
            종료일이 시작일보다 빠릅니다.
          </div>
        )}

        {hits.length > 0 && (
          <div style={{ marginTop: 12 }}>
            <Notice tone="warn">
              기존 작업공간과 운영기간이 겹칩니다. 작업공간 기간을 확인해 주세요.
              <div className="hint" style={{ marginTop: 6 }}>
                {hits.map((w) => (
                  <div key={w.id}>
                    · {w.name} ({w.startDate} ~ {w.endDate})
                  </div>
                ))}
              </div>
            </Notice>
          </div>
        )}

        <div style={{ height: 12 }} />

        <Field label="비고">
          <Input value={form.note ?? ''} onChange={(e) => setForm({ ...form, note: e.target.value })} />
        </Field>

        <div className="hint" style={{ marginTop: 12 }}>
          날짜는 목록 순서와 지원금 누적 순서를 함께 정합니다. 어느 지원기간(1학기·2학기 등)에
          속하는지도 시작일로 정해집니다.
        </div>
      </Modal>

      {confirmOverlap && (
        <Confirm
          title="운영기간이 겹칩니다"
          confirmText="그래도 저장"
          message={
            <>
              아래 작업공간과 기간이 겹칩니다.
              <div style={{ margin: '8px 0', color: 'var(--gray-600)' }}>
                {hits.map((w) => (
                  <div key={w.id}>
                    · {w.name} ({w.startDate} ~ {w.endDate})
                  </div>
                ))}
              </div>
              겹쳐도 저장은 되지만, 같은 기간의 수강이 두 작업공간에 나뉘어 들어갈 수 있습니다.
              날짜가 맞는지 확인해 주세요.
            </>
          }
          busy={save.isPending}
          onConfirm={() => {
            setConfirmOverlap(false)
            save.mutate()
          }}
          onClose={() => setConfirmOverlap(false)}
        />
      )}
    </>
  )
}
