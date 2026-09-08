/**
 * 부서정보 — 작업공간 소속. 수강료·강사·요일이 기간마다 다르기 때문이다.
 *
 * 여기 금액은 부서의 **기준** 수강료다. 이미 등록된 수강생의 금액을 자동으로
 * 덮어쓰지 않는다 (§42-3). 반영은 Phase 2의 [부서정보 반영]에서 사람이 누른다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { ApplyFeesModal } from '@/components/ApplyFeesModal'
import { cmp, DataTable, type Column } from '@/components/DataTable'
import { ExcelTools } from '@/components/ExcelTools'
import { Confirm, Modal } from '@/components/Modal'
import { StudentFeesModal } from '@/components/StudentFeesModal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Input, MoneyInput, Notice, Search } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Department, DepartmentInput, Fee } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function DepartmentsPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const items = app.boot.costItems

  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<number[]>([])
  const [editing, setEditing] = useState<Department | 'new' | null>(null)
  const [confirm, setConfirm] = useState<'some' | 'all' | null>(null)
  /** 학생별 금액 수정 팝업을 띄울 부서 */
  const [feesFor, setFeesFor] = useState<Department | null>(null)
  /** 부서금액 반영 팝업 — 부서 하나 또는 전체 */
  const [applyFor, setApplyFor] = useState<Department | 'all' | null>(null)

  const wsId = app.workspaceId

  const list = useQuery({
    queryKey: ['departments', wsId, query],
    queryFn: () => api.departmentList(wsId!, query.trim() || undefined),
    enabled: wsId !== null,
  })

  const remove = useMutation({
    mutationFn: (ids: number[]) => api.departmentDelete(ids),
    onSuccess: (n) => {
      toast.ok(`${n}개 부서를 지웠습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const removeAll = useMutation({
    mutationFn: () => api.departmentDeleteAll(wsId!),
    onSuccess: (r) => {
      toast.ok(`부서 ${r.deleted}개를 지웠습니다. 삭제 전 자료는 ${r.backup} 에 있습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <div>
            <h1 className="page__title">부서정보</h1>
          </div>
        </div>
        <div className="card">
          <div className="card__body">
            <Empty title="작업공간이 없습니다">
              부서와 수강료는 기간마다 다르므로 작업공간에 속합니다.
              [작업공간]에서 먼저 하나 만들어 주세요.
            </Empty>
          </div>
        </div>
      </div>
    )
  }

  const feeOf = (d: Department, code: string) =>
    d.fees.find((f) => f.itemCode === code)?.amount ?? 0

  const columns: Column<Department>[] = [
    { key: 'name', head: '부서명', width: 130, sort: cmp.text((d) => d.name), render: (d) => d.name },
    { key: 'className', head: '반명', width: 80, render: (d) => d.className || <span className="muted">—</span> },
    { key: 'teacher', head: '강사명', width: 90, render: (d) => d.teacher || <span className="muted">—</span> },
    { key: 'days', head: '요일', width: 90, render: (d) => d.days || <span className="muted">—</span> },
    ...items.map<Column<Department>>((it) => ({
      key: it.code,
      head: it.name,
      align: 'num' as const,
      width: 92,
      sort: cmp.num((d) => feeOf(d, it.code)),
      render: (d) => won(feeOf(d, it.code)),
    })),
    {
      key: 'total',
      head: '합계',
      align: 'num',
      width: 100,
      sort: cmp.num((d) => d.total),
      render: (d) => <b>{won(d.total)}</b>,
    },
    {
      key: 'count',
      head: '수강생',
      align: 'num',
      width: 74,
      sort: cmp.num((d) => d.enrollmentCount),
      render: (d) => won(d.enrollmentCount),
    },
  ]

  const one = selected.length === 1 ? list.data?.find((d) => d.id === selected[0]) : undefined

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">부서정보</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> 작업공간의 부서와 기준 수강료입니다.
          </p>
        </div>
        <div className="page__actions">
          <ExcelTools
            kind="departments"
            yearId={app.yearId}
            workspaceId={wsId}
            onDone={() => void qc.invalidateQueries()}
          />
        </div>
      </div>

      <Notice tone="info">
        여기 금액은 <b>기준</b> 수강료입니다. 이미 등록된 수강생의 금액은 자동으로 바뀌지
        않습니다 — 부서를 고른 뒤 <b>[학생별 수정]</b>으로 한 명씩 고치거나,{' '}
        <b>[부서금액 반영]</b>으로 무엇이 바뀌는지 확인한 뒤 반영합니다.
      </Notice>

      <Card
        flush
        title="부서 목록"
        actions={
          <>
            <Button variant="primary" small onClick={() => setEditing('new')}>
              수기 추가
            </Button>
            <Button
              small
              disabled={!one}
              title="줄을 두 번 눌러도 수정창이 열립니다"
              onClick={() => one && setEditing(one)}
            >
              수정
            </Button>
            <Button
              small
              disabled={!one}
              title={one ? `${one.name} 수강생의 금액을 학생별로 고칩니다` : '부서를 먼저 고르세요'}
              onClick={() => one && setFeesFor(one)}
            >
              학생별 수정
            </Button>
            <Button
              small
              onClick={() => setApplyFor(one ?? 'all')}
              title={one ? `${one.name}의 기준금액을 수강생에게 반영합니다` : '모든 부서의 기준금액을 반영합니다'}
            >
              {one ? '이 부서 금액 반영' : '부서금액 반영'}
            </Button>
            <Button small variant="danger" disabled={selected.length === 0} onClick={() => setConfirm('some')}>
              선택 삭제
            </Button>
            <Button small variant="danger" onClick={() => setConfirm('all')}>
              전체 삭제
            </Button>
          </>
        }
      >
        <div style={{ padding: 12 }}>
          <div className="toolbar">
            <Field label="검색">
              <Search value={query} onValue={setQuery} placeholder="검색" />
            </Field>
            <Button onClick={() => setQuery('')}>초기화</Button>
          </div>
        </div>

        <div className="toolbar__note">
          줄을 두 번 누르면 <b>수정</b>창이 열립니다. 한 번 누르는 것은 선택입니다.
        </div>

        <DataTable
          rows={list.data ?? []}
          columns={columns}
          getId={(d) => d.id}
          selected={selected}
          onSelected={setSelected}
          onRowClick={(d) => setSelected([d.id])}
          onRowDoubleClick={(d) => setEditing(d)}
          empty={list.isLoading ? '불러오는 중…' : '부서가 없습니다.'}
          foot={
            <>
              <span>
                모두 <b>{(list.data ?? []).length}</b>개 부서
              </span>
              <span className="toolbar__spacer" />
              {items.map((it) => (
                <span key={it.code}>
                  {it.name}{' '}
                  <b>
                    {won((list.data ?? []).reduce((s, d) => s + feeOf(d, it.code), 0))}
                  </b>
                </span>
              ))}
            </>
          }
        />
      </Card>

      {editing && (
        <DepartmentModal
          value={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {feesFor && (
        <StudentFeesModal
          department={feesFor}
          onClose={() => setFeesFor(null)}
          onSaved={() => {
            setFeesFor(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {applyFor && (
        <ApplyFeesModal
          department={applyFor === 'all' ? null : applyFor}
          onClose={() => setApplyFor(null)}
          onApplied={() => {
            setApplyFor(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {confirm === 'some' && (
        <Confirm
          danger
          title="선택한 부서 삭제"
          message={
            <>
              선택한 <b>{selected.length}개</b> 부서를 지웁니다. 그 부서의 수강 자료도 함께
              사라집니다. 계속할까요?
            </>
          }
          busy={remove.isPending}
          onConfirm={() => remove.mutate(selected)}
          onClose={() => setConfirm(null)}
        />
      )}

      {confirm === 'all' && (
        <Confirm
          danger
          title="부서정보 전체 삭제"
          confirmText="모두 삭제"
          message={
            <>
              이 작업은 <b>{app.workspace?.name}</b> 작업공간의 부서{' '}
              <b>{(list.data ?? []).length}개</b>를 모두 삭제합니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                함께 사라지는 것:
                <br />· 이 부서들의 수강 자료{' '}
                <b>{(list.data ?? []).reduce((s, d) => s + d.enrollmentCount, 0)}건</b>과 금액
                <br />· 이 작업공간의 부서 차감 우선순위
              </div>
              <div style={{ marginTop: 10 }}>
                삭제 직전에 <b>자동으로 백업</b>됩니다. 다른 작업공간의 자료는 그대로입니다.
              </div>
            </>
          }
          busy={removeAll.isPending}
          onConfirm={() => removeAll.mutate()}
          onClose={() => setConfirm(null)}
        />
      )}
    </div>
  )
}

function DepartmentModal({
  value,
  onClose,
  onSaved,
}: {
  value: Department | null
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const items = app.boot.costItems

  const [form, setForm] = useState<DepartmentInput>({
    name: value?.name ?? '',
    className: value?.className ?? '',
    teacher: value?.teacher ?? '',
    days: value?.days ?? '',
    note: value?.note ?? '',
    fees: items.map<Fee>((it) => ({
      itemCode: it.code,
      amount: value?.fees.find((f) => f.itemCode === it.code)?.amount ?? 0,
    })),
  })

  const total = form.fees.reduce((s, f) => s + f.amount, 0)

  const save = useMutation({
    mutationFn: async () => {
      if (value) await api.departmentUpdate(value.id, form)
      else await api.departmentCreate(app.workspaceId!, form)
    },
    onSuccess: () => {
      toast.ok('저장되었습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function setFee(code: string, amount: number) {
    setForm((f) => ({
      ...f,
      fees: f.fees.map((x) => (x.itemCode === code ? { ...x, amount } : x)),
    }))
  }

  return (
    <Modal
      wide
      title={value ? '부서 수정' : '부서 추가'}
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
      <div className="grid4">
        <Field label="부서명">
          <Input
            autoFocus
            value={form.name}
            placeholder="로봇과학"
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
        </Field>
        <Field label="반명">
          <Input
            value={form.className ?? ''}
            placeholder="A반"
            onChange={(e) => setForm({ ...form, className: e.target.value })}
          />
        </Field>
        <Field label="강사명">
          <Input
            value={form.teacher ?? ''}
            onChange={(e) => setForm({ ...form, teacher: e.target.value })}
          />
        </Field>
        <Field label="요일">
          <Input
            value={form.days ?? ''}
            placeholder="월,수"
            onChange={(e) => setForm({ ...form, days: e.target.value })}
          />
        </Field>
      </div>

      <div style={{ height: 16 }} />
      <div className="card__title" style={{ marginBottom: 8 }}>
        기준 수강료
      </div>

      <div className="grid4">
        {items.map((it) => (
          <Field key={it.code} label={it.name}>
            <MoneyInput
              value={form.fees.find((f) => f.itemCode === it.code)?.amount ?? 0}
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

      <div style={{ height: 12 }} />
      <Field label="비고">
        <Input value={form.note ?? ''} onChange={(e) => setForm({ ...form, note: e.target.value })} />
      </Field>
    </Modal>
  )
}
