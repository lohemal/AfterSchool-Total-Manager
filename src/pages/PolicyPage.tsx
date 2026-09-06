/**
 * 학년도 지원금 설정 (요구사항 §4~§8).
 *
 * 회계 구조를 몰라도 정책만 보고 설정할 수 있어야 한다. 그래서 화면은
 * `연간한도 → 지원기간 → 미사용 지원금 처리 → 대상학년` 차례로 놓고,
 * 프로그램이 대신 확인해 주는 것(합계 불일치·기간 겹침·작업공간 배치)을
 * 그 자리에서 알려 준다.
 *
 * **기본값을 임의로 넣지 않는다.** 한도와 대상학년은 시도마다 다르다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { PriorityEditor } from '@/components/PriorityEditor'
import { Confirm, Modal } from '@/components/Modal'
import { StudentPicker } from '@/components/StudentPicker'
import { useToast } from '@/components/Toast'
import { Button, Card, Field, Input, MoneyInput, Notice, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Grant, Period, ProgramCode } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

const TABS: { code: ProgramCode; label: string }[] = [
  { code: 'VOUCHER', label: '방과후 이용권' },
  { code: 'FREE_VOUCHER', label: '자유수강권' },
]

export function PolicyPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const [program, setProgram] = useState<ProgramCode>('VOUCHER')

  const policies = useQuery({
    queryKey: ['policies', app.yearId],
    queryFn: () => api.policyList(app.yearId),
  })
  const view = policies.data?.find((p) => p.policy.program === program)

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">학년도 지원금 설정</h1>
          <p className="page__desc">
            {app.year?.name} · 여기서 정한 값으로 정산이 계산됩니다.
          </p>
        </div>
      </div>

      <div style={{ display: 'flex', gap: 6, marginBottom: 12 }}>
        {TABS.map((t) => (
          <Button
            key={t.code}
            variant={program === t.code ? 'primary' : 'default'}
            onClick={() => setProgram(t.code)}
          >
            {t.label}
          </Button>
        ))}
      </div>

      {view && (
        <PolicyForm
          key={`${program}-${view.policy.id}`}
          program={program}
          annualLimit={view.policy.annualLimit}
          carryover={view.policy.carryover}
          targetGrades={view.policy.targetGrades}
          periods={view.policy.periods}
          notice={view.notice}
          onSaved={() => {
            toast.ok('지원금 설정을 저장했습니다.')
            void qc.invalidateQueries()
          }}
        />
      )}

      <GrantCard program={program} periods={view?.policy.periods ?? []} />

      <PriorityCard />
    </div>
  )
}

// ─────────────────────────────────────────────── 정책 본문

function PolicyForm({
  program,
  annualLimit,
  carryover,
  targetGrades,
  periods,
  notice,
  onSaved,
}: {
  program: ProgramCode
  annualLimit: number
  carryover: boolean
  targetGrades: string
  periods: Period[]
  notice: string | null
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [annual, setAnnual] = useState(annualLimit)
  const [carry, setCarry] = useState(carryover)
  const [grades, setGrades] = useState(targetGrades)
  const [rows, setRows] = useState<Period[]>(periods)

  const workspaces = app.workspaces
  const sum = rows.reduce((s, p) => s + p.limitAmount, 0)

  const save = useMutation({
    mutationFn: () => api.policySave(app.yearId, program, annual, carry, grades, rows),
    onSuccess: onSaved,
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function addPeriod() {
    const y = app.year?.startDate.slice(0, 4) ?? '2026'
    setRows([
      ...rows,
      {
        id: null,
        name: rows.length === 0 ? '1학기' : `${rows.length + 1}학기`,
        startDate: rows.length === 0 ? `${y}-03-01` : '',
        endDate: rows.length === 0 ? `${y}-08-31` : '',
        limitAmount: 0,
        seq: rows.length + 1,
      },
    ])
  }

  function setRow(i: number, patch: Partial<Period>) {
    setRows(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  }

  /** 각 작업공간이 어느 기간에 속하는지 — 시작일이 들어가는 기간 */
  function periodOf(startDate: string): string {
    const hit = rows.find((p) => p.startDate && p.endDate && p.startDate <= startDate && startDate <= p.endDate)
    return hit?.name ?? '(속하는 기간 없음)'
  }

  const overlap = (() => {
    const sorted = [...rows]
      .filter((r) => r.startDate && r.endDate)
      .sort((a, b) => a.startDate.localeCompare(b.startDate))
    for (let i = 0; i + 1 < sorted.length; i++) {
      if (sorted[i].endDate >= sorted[i + 1].startDate) {
        return `'${sorted[i].name}'와 '${sorted[i + 1].name}'의 기간이 겹칩니다.`
      }
    }
    return null
  })()

  return (
    <Card
      title={program === 'VOUCHER' ? '방과후 이용권' : '자유수강권'}
      actions={
        <Button variant="primary" small onClick={() => save.mutate()} disabled={save.isPending}>
          {save.isPending ? '저장 중…' : '저장'}
        </Button>
      }
    >
      <div className="grid2" style={{ maxWidth: 640 }}>
        <Field label="연간 지원한도" hint="한 학년도에 학생 한 명이 받을 수 있는 최대 금액">
          <MoneyInput value={annual} onValue={setAnnual} />
        </Field>
        <Field
          label="대상 학년"
          hint="쉼표로 구분합니다. 비우면 전 학년이 대상입니다. (예: 3 또는 3,4)"
        >
          <Input value={grades} placeholder="예: 3" onChange={(e) => setGrades(e.target.value)} />
        </Field>
      </div>

      <div style={{ height: 18 }} />
      <div className="card__head" style={{ padding: 0, border: 'none', marginBottom: 8 }}>
        <span className="card__title">지원기간</span>
        <div style={{ marginLeft: 'auto' }}>
          <Button small onClick={addPeriod}>
            + 기간 추가
          </Button>
        </div>
      </div>

      {rows.length === 0 ? (
        <Notice tone="info">
          지원기간을 만들지 않으면 <b>연간 한도 하나</b>로 운영됩니다. 학기별로 한도를 나누어
          쓰는 학교만 기간을 추가하세요.
        </Notice>
      ) : (
        <>
          <div className="tableWrap" style={{ border: '1px solid var(--gray-200)' }}>
            <table className="table">
              <thead>
                <tr>
                  <th style={{ width: 130 }}>이름</th>
                  <th style={{ width: 150 }}>시작일</th>
                  <th style={{ width: 150 }}>종료일</th>
                  <th className="num" style={{ width: 130 }}>
                    지원한도
                  </th>
                  <th style={{ width: 52 }} />
                </tr>
              </thead>
              <tbody>
                {rows.map((p, i) => (
                  <tr key={i}>
                    <td style={{ padding: '2px 6px' }}>
                      <Input
                        value={p.name}
                        style={{ height: 26 }}
                        onChange={(e) => setRow(i, { name: e.target.value })}
                      />
                    </td>
                    <td style={{ padding: '2px 6px' }}>
                      <Input
                        type="date"
                        value={p.startDate}
                        style={{ height: 26 }}
                        onChange={(e) => setRow(i, { startDate: e.target.value })}
                      />
                    </td>
                    <td style={{ padding: '2px 6px' }}>
                      <Input
                        type="date"
                        value={p.endDate}
                        style={{ height: 26 }}
                        onChange={(e) => setRow(i, { endDate: e.target.value })}
                      />
                    </td>
                    <td style={{ padding: '2px 6px' }}>
                      <MoneyInput
                        value={p.limitAmount}
                        style={{ height: 26 }}
                        onValue={(n) => setRow(i, { limitAmount: n })}
                      />
                    </td>
                    <td>
                      <Button
                        small
                        variant="danger"
                        onClick={() => setRows(rows.filter((_, idx) => idx !== i))}
                      >
                        ✕
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="hint" style={{ marginTop: 8 }}>
            기간 한도 합계 <b>{won(sum)}</b>원 / 연간 한도 <b>{won(annual)}</b>원{' '}
            {sum === annual ? '✓' : '— 확인이 필요합니다'}
          </div>
        </>
      )}

      {overlap && (
        <div style={{ marginTop: 10 }}>
          <Notice tone="bad">{overlap}</Notice>
        </div>
      )}
      {!overlap && notice && rows.length > 0 && (
        <div style={{ marginTop: 10 }}>
          <Notice tone="warn">
            {notice} 계산은 기간 한도와 연간 한도 가운데 <b>작은 쪽</b>을 따르므로 어느 쪽이든
            안전하게 처리됩니다.
          </Notice>
        </div>
      )}

      <div style={{ height: 18 }} />
      <div className="card__title" style={{ marginBottom: 8 }}>
        미사용 지원금 처리
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxWidth: 640 }}>
        {[
          {
            v: true,
            label: '다음 지원기간으로 이월',
            desc: '남은 금액이 다음 기간에 더해집니다. 연간 한도는 넘지 못합니다.',
          },
          {
            v: false,
            label: '지원기간 종료 시 소멸',
            desc: '남은 금액이 사라지고 다음 기간은 기본 한도만 씁니다.',
          },
        ].map((o) => (
          <label
            key={String(o.v)}
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: 8,
              padding: '8px 10px',
              border: '1px solid',
              borderColor: carry === o.v ? 'var(--navy-500)' : 'var(--gray-200)',
              background: carry === o.v ? 'var(--blue-50)' : 'var(--white)',
              borderRadius: 'var(--radius)',
              cursor: rows.length === 0 ? 'default' : 'pointer',
              opacity: rows.length === 0 ? 0.5 : 1,
            }}
          >
            <input
              type="radio"
              name={`carry-${program}`}
              checked={carry === o.v}
              disabled={rows.length === 0}
              onChange={() => setCarry(o.v)}
              style={{ marginTop: 2, accentColor: 'var(--navy-500)' }}
            />
            <span>
              <b>{o.label}</b>
              <div className="hint">{o.desc}</div>
            </span>
          </label>
        ))}
      </div>
      {rows.length === 0 && (
        <div className="hint" style={{ marginTop: 6 }}>
          지원기간이 없으면 이월할 것도 없으므로 이 설정은 쓰이지 않습니다.
        </div>
      )}

      {workspaces.length > 0 && rows.length > 0 && (
        <>
          <div style={{ height: 18 }} />
          <div className="card__title" style={{ marginBottom: 8 }}>
            작업공간이 속하는 기간
          </div>
          <div className="tableWrap" style={{ border: '1px solid var(--gray-200)', maxHeight: 180 }}>
            <table className="table">
              <thead>
                <tr>
                  <th className="left" style={{ width: 200 }}>
                    작업공간
                  </th>
                  <th style={{ width: 200 }}>기간</th>
                  <th style={{ width: 160 }}>지원기간</th>
                </tr>
              </thead>
              <tbody>
                {workspaces.map((w) => {
                  const p = periodOf(w.startDate)
                  return (
                    <tr key={w.id}>
                      <td className="left">{w.name}</td>
                      <td>
                        {w.startDate} ~ {w.endDate}
                      </td>
                      <td style={{ color: p.startsWith('(') ? 'var(--amber-600)' : undefined }}>
                        {p}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
          <div className="hint" style={{ marginTop: 6 }}>
            작업공간의 <b>시작일</b>이 들어가는 기간에 속합니다.
          </div>
        </>
      )}
    </Card>
  )
}

// ─────────────────────────────────────────────── 학생별 예외 한도

function GrantCard({ program, periods }: { program: ProgramCode; periods: Period[] }) {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const [adding, setAdding] = useState(false)
  const [removing, setRemoving] = useState<Grant | null>(null)

  const list = useQuery({
    queryKey: ['grants', app.yearId, program],
    queryFn: () => api.grantList(app.yearId, program),
  })

  const remove = useMutation({
    mutationFn: (id: number) => api.grantDelete([id]),
    onSuccess: () => {
      toast.ok('예외 한도를 지웠습니다.')
      setRemoving(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const rows = list.data ?? []

  return (
    <Card
      title="학생별 예외 한도"
      actions={
        <Button small variant="primary" onClick={() => setAdding(true)}>
          예외 추가
        </Button>
      }
    >
      <Notice tone="info">
        <b>연간 한도 예외</b>와 <b>기간 한도 예외</b>는 서로 독립입니다. 연간만 바꾸면 기간
        한도는 그대로이고, 계산은 둘 중 작은 쪽을 따릅니다.
      </Notice>

      {rows.length === 0 ? (
        <div className="hint">예외 한도가 없습니다. 모든 학생이 기본 정책을 따릅니다.</div>
      ) : (
        <div className="tableWrap" style={{ border: '1px solid var(--gray-200)', maxHeight: 240 }}>
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 54 }}>학년</th>
                <th style={{ width: 54 }}>반</th>
                <th style={{ width: 54 }}>번호</th>
                <th style={{ width: 100 }}>이름</th>
                <th style={{ width: 140 }}>적용 대상</th>
                <th className="num" style={{ width: 120 }}>
                  한도
                </th>
                <th className="left">사유</th>
                <th style={{ width: 60 }} />
              </tr>
            </thead>
            <tbody>
              {rows.map((g) => (
                <tr key={g.id}>
                  <td>{g.grade}</td>
                  <td>{g.classNo}</td>
                  <td>{g.studentNo}</td>
                  <td>{g.name}</td>
                  <td>
                    {g.periodId === null ? (
                      <span className="tag tag--voucher">연간 한도</span>
                    ) : (
                      <span className="tag tag--free">{g.periodName}</span>
                    )}
                  </td>
                  <td className="num">
                    <b>{won(g.amount)}</b>
                  </td>
                  <td className="left">{g.reason || <span className="muted">—</span>}</td>
                  <td>
                    <Button small variant="danger" onClick={() => setRemoving(g)}>
                      ✕
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {adding && (
        <GrantModal
          program={program}
          periods={periods}
          onClose={() => setAdding(false)}
          onSaved={() => {
            setAdding(false)
            void qc.invalidateQueries()
          }}
        />
      )}

      {removing && (
        <Confirm
          danger
          title="예외 한도 삭제"
          message={
            <>
              <b>{removing.name}</b>의 {removing.periodId === null ? '연간' : removing.periodName}{' '}
              예외 한도를 지웁니다. 이 학생은 기본 정책을 따르게 됩니다.
            </>
          }
          busy={remove.isPending}
          onConfirm={() => remove.mutate(removing.id)}
          onClose={() => setRemoving(null)}
        />
      )}
    </Card>
  )
}

function GrantModal({
  program,
  periods,
  onClose,
  onSaved,
}: {
  program: ProgramCode
  periods: Period[]
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [studentId, setStudentId] = useState<number | null>(null)
  const [scope, setScope] = useState<string>('')
  const [amount, setAmount] = useState(0)
  const [reason, setReason] = useState('')

  const save = useMutation({
    mutationFn: async () => {
      if (!studentId) throw { code: 'INVALID', message: '학생을 골라 주세요.' }
      await api.grantSave(app.yearId, {
        studentId,
        program,
        periodId: scope === '' ? null : Number(scope),
        amount,
        reason,
      })
    },
    onSuccess: () => {
      toast.ok('예외 한도를 저장했습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      wide
      title="학생별 예외 한도"
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
      <StudentPicker value={studentId} onChange={(id) => setStudentId(id)} />

      <div style={{ height: 14 }} />
      <div className="grid3">
        <Field label="적용 대상" hint="연간 한도와 기간 한도는 따로 정합니다.">
          <Select value={scope} onChange={(e) => setScope(e.target.value)}>
            <option value="">연간 한도</option>
            {periods.map((p) => (
              <option key={p.id ?? p.name} value={p.id ?? ''}>
                {p.name} 한도
              </option>
            ))}
          </Select>
        </Field>
        <Field label="한도">
          <MoneyInput value={amount} onValue={setAmount} />
        </Field>
        <Field label="사유">
          <Input value={reason} onChange={(e) => setReason(e.target.value)} />
        </Field>
      </div>

      <div className="hint" style={{ marginTop: 12 }}>
        같은 학생·같은 대상에 이미 예외가 있으면 금액이 바뀝니다.
      </div>
    </Modal>
  )
}

// ─────────────────────────────────────────────── 차감 우선순위

function PriorityCard() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const wsId = app.workspaceId

  const depts = useQuery({
    queryKey: ['priority-dept', wsId],
    queryFn: () => api.priorityDeptList(wsId!),
    enabled: wsId !== null,
  })
  const items = useQuery({
    queryKey: ['priority-item', wsId],
    queryFn: () => api.priorityItemList(wsId!),
    enabled: wsId !== null,
  })

  const saveDept = useMutation({
    mutationFn: (order: string[]) => api.priorityDeptSave(wsId!, order.map(Number)),
    onSuccess: () => {
      toast.ok('부서 차감 순서를 저장했습니다.')
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })
  const saveItem = useMutation({
    mutationFn: (order: string[]) => api.priorityItemSave(wsId!, order),
    onSuccess: () => {
      toast.ok('비용항목 차감 순서를 저장했습니다.')
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (wsId === null) return null

  return (
    <Card title={`차감 우선순위 — ${app.workspace?.name ?? ''}`}>
      <Notice tone="info">
        지원금이 모자랄 때 <b>어느 부서·어느 항목부터</b> 쓸지 정합니다. 이 순서는 지금
        작업공간에만 적용됩니다.
      </Notice>

      <div className="grid2" style={{ alignItems: 'start' }}>
        <div>
          <div className="card__title" style={{ marginBottom: 8 }}>
            부서
          </div>
          <PriorityEditor
            rows={depts.data ?? []}
            showCount
            emptyTitle="이용권 대상 수강생이 없습니다"
            emptyHint="이용권 대상 학생이 수강을 등록하면 그 부서가 여기 나옵니다."
            onChange={(keys) => saveDept.mutate(keys)}
          />
        </div>
        <div>
          <div className="card__title" style={{ marginBottom: 8 }}>
            비용항목
          </div>
          <PriorityEditor
            rows={items.data ?? []}
            emptyTitle="비용항목이 없습니다"
            emptyHint=""
            onChange={(keys) => saveItem.mutate(keys)}
          />
        </div>
      </div>
    </Card>
  )
}
