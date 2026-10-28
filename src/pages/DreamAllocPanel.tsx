/**
 * 방과후드림 결제관리 — 배분 우선순위 · 지원금 재배분 · 결과 미리보기 (P4).
 *
 * 지원금 총액은 다시 계산하지 않는다. 학생마다 최신 정산이 확정한 이용권(V) · 자유수강권(F)을
 * 방과후드림 전용 부서 · 항목 차례대로 결제행에 나눠 담는 것만 서버가 한다(V 먼저, 그다음 F,
 * 나머지는 개인부담). 화면은 차례를 받고, 결과와 검산을 보여 준다.
 *
 * [배분 계산]을 누를 때마다 서버가 파일 지문 · 정산 · 매칭 전체를 다시 확인한다. 우선순위를
 * 바꾸거나 매칭 쪽에서 무엇이든 다시 하면 이전 결과는 버린다(다시 계산해야 한다).
 *
 * 결과 Excel 은 아직 만들지 않는다(P5). 이체예정일은 이 화면에서만 들고 있다가 P5 에 넘긴다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Fragment, useEffect, useMemo, useRef, useState } from 'react'

import { useToast } from '@/components/Toast'
import { Button, Card, Input, Notice, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { DreamAllocation, DreamAllocStudent, DreamPriorityView, DreamSession, DreamTotals } from '@/ipc/types'
import { allocIssueText, move, prioritySourceLabel, transferDateCode } from '@/lib/dream'

const won = (n: number) => n.toLocaleString('ko-KR')

export function DreamAllocPanel({
  workspaceId,
  path,
  session,
  onSession,
  onError,
}: {
  workspaceId: number
  path: string
  session: DreamSession
  /** 배분 계산이 다시 돌린 매칭 결과로 위 화면을 바꿔 그린다 */
  onSession: (s: DreamSession) => void
  onError: (e: unknown) => void
}) {
  const [result, setResult] = useState<DreamAllocation | null>(null)
  const [staleWhy, setStaleWhy] = useState<string | null>(null)
  const [date, setDate] = useState('')
  // 매칭 쪽에서 무엇이든 다시 하면(다시 검증 · 확인) 이전 배분 결과는 버린다.
  // 배분 계산 자체가 돌려준 매칭 결과로 바뀐 것만 예외다.
  const own = useRef<DreamSession | null>(null)
  const had = useRef(false)
  had.current = result !== null
  useEffect(() => {
    if (session !== own.current && had.current) {
      setResult(null)
      setStaleWhy('매칭 결과가 다시 계산되어 이전 배분 결과는 쓰지 않습니다.')
    }
  }, [session])

  const run = useMutation({
    mutationFn: () => api.dreamAllocate(session.jobId, path),
    onSuccess: (a) => {
      setResult(a)
      setStaleWhy(null)
      if (a.session) {
        own.current = a.session
        onSession(a.session)
      }
    },
    onError,
  })

  const ready = session.matching.p4Ready
  const code = transferDateCode(date)
  return (
    <>
      <PriorityCard
        workspaceId={workspaceId}
        onChanged={() => {
          if (result) {
            setResult(null)
            setStaleWhy('배분 우선순위가 바뀌어 이전 배분 결과는 쓰지 않습니다.')
          }
        }}
      />

      <Card
        title="배분 계산"
        actions={
          <Button variant="primary" small disabled={!ready || run.isPending} onClick={() => run.mutate()}>
            {run.isPending ? '계산하는 중…' : result ? '배분 다시 계산' : '배분 계산'}
          </Button>
        }
      >
        {!ready && (
          <Notice tone="warn">매칭 및 검증이 모두 준비 완료여야 배분할 수 있습니다.</Notice>
        )}
        {ready && !result && (
          <div className="hint">
            누를 때마다 결제파일 지문 · 최신 정산 · 매칭 전체(정산에 금액이 남은 학생과 금액이 모두
            파일에 있는지 포함)를 다시 확인한 뒤 계산합니다. 학생마다 정산이 확정한 이용권 → 자유수강권
            순서로 위 우선순위대로 줄에 담고, 나머지는 개인부담입니다. 정산 자료는 고치지 않습니다.
          </div>
        )}
        {staleWhy && !result && (
          <Notice tone="warn">
            {staleWhy} <b>재계산 필요</b> — [배분 계산]을 다시 눌러 주세요.
          </Notice>
        )}
        {result && result.status !== 'DONE' && (
          <>
            {result.issues.map((i, k) => (
              <Notice key={k} tone="bad">
                {allocIssueText(i)}
              </Notice>
            ))}
          </>
        )}
        {result?.status === 'DONE' && (
          <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
            <span className="tag tag--free">배분 검증 완료</span>
            <span className="hint">
              학생 {result.students.length}명 · 줄마다 정산금액 = 개인부담 + 이용권 + 자유수강권, 학생마다 ·
              파일 전체 합계가 최신 정산과 같습니다.
            </span>
          </div>
        )}
      </Card>

      {result?.status === 'DONE' && <ResultCard a={result} />}

      <Card title="이체예정일">
        <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
          <Input type="date" value={date} onChange={(e) => setDate(e.target.value)} style={{ width: 160 }} />
          {code ? (
            <span className="hint">
              결과 파일의 이체예정일(R)에 모든 결제행(0원 줄 포함) 같은 값 <b>{code}</b>로 들어갑니다. 원본의
              이체예정일은 쓰지 않습니다.
            </span>
          ) : (
            <span className="hint">이체예정일을 입력해야 결과 파일을 만들 수 있습니다.</span>
          )}
        </div>
        <div className="hint" style={{ marginTop: 8 }}>
          결과 파일 만들기(다음 단계):{' '}
          {result?.status === 'DONE' && code ? (
            <span className="tag tag--free">준비됨</span>
          ) : (
            <span className="tag tag--bad">
              {result?.status === 'DONE' ? '이체예정일 필요' : '배분 검증 완료 필요'}
            </span>
          )}
        </div>
      </Card>
    </>
  )
}

// ─────────────────────────────────────────────── 배분 우선순위

function PriorityCard({ workspaceId, onChanged }: { workspaceId: number; onChanged: () => void }) {
  const toast = useToast()
  const qc = useQueryClient()
  const key = ['dreamPriority', workspaceId]
  const q = useQuery({ queryKey: key, queryFn: () => api.dreamPriority(workspaceId) })
  const view = q.data
  // 늘어놓을 부서 — 정산할 금액이 있거나 강좌코드에 연결된 부서(없으면 모두)
  const shown = useMemo(() => {
    const all = view?.departments ?? []
    const used = all.filter((d) => d.inUse)
    return used.length > 0 ? used : all
  }, [view])
  const [order, setOrder] = useState<number[]>([])
  const [pick, setPick] = useState<number | null>(null)
  const [items, setItems] = useState<string[]>([])
  useEffect(() => setOrder(shown.map((d) => d.departmentId)), [shown])
  const dept = shown.find((d) => d.departmentId === pick) ?? shown[0]
  useEffect(() => setItems(dept ? dept.items.map((i) => i.code) : []), [dept])

  const saved = (v: DreamPriorityView) => {
    const changed = v.digest !== view?.digest
    qc.setQueryData(key, v)
    toast.ok(changed ? '배분 우선순위를 저장했습니다.' : '바뀐 것이 없습니다.')
    if (changed) onChanged()
  }
  const saveDepts = useMutation({
    mutationFn: () => api.dreamSaveDeptPriority(workspaceId, order),
    onSuccess: saved,
    onError: (e) => toast.bad(errorMessage(e)),
  })
  const saveItems = useMutation({
    mutationFn: () => api.dreamSaveItemPriority(workspaceId, dept!.departmentId, items),
    onSuccess: saved,
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (!view) return null
  const label = (id: number) => shown.find((d) => d.departmentId === id)?.label ?? String(id)
  const name = (code: string) => dept?.items.find((i) => i.code === code)?.name ?? code
  const deptDirty = order.join(',') !== shown.map((d) => d.departmentId).join(',')
  const itemDirty = dept ? items.join(',') !== dept.items.map((i) => i.code).join(',') : false

  return (
    <Card title="배분 우선순위">
      <div className="hint" style={{ marginBottom: 8 }}>
        방과후드림 결제파일에서 이용권 · 자유수강권을 <b>어느 줄부터</b> 채울지 정합니다. 학생마다 지원
        총액은 바뀌지 않습니다. 기존 정산의 차감 우선순위와는 따로이며, 여기서 바꿔도 정산은 그대로입니다.
      </div>
      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', alignItems: 'flex-start' }}>
        <div style={{ minWidth: 280 }}>
          <div style={{ marginBottom: 6 }}>
            <b>부서 차례</b> <span className="hint">— {prioritySourceLabel(view.deptSource)}</span>
          </div>
          <ol style={{ margin: 0, paddingLeft: 20 }}>
            {order.map((id, i) => (
              <li key={id} style={{ marginBottom: 4 }}>
                <span style={{ display: 'inline-block', minWidth: 170 }}>{label(id)}</span>
                <Button small disabled={i === 0} onClick={() => setOrder(move(order, i, -1))} aria-label="위로">
                  ↑
                </Button>{' '}
                <Button
                  small
                  disabled={i === order.length - 1}
                  onClick={() => setOrder(move(order, i, 1))}
                  aria-label="아래로"
                >
                  ↓
                </Button>
              </li>
            ))}
          </ol>
          <div style={{ marginTop: 8, display: 'flex', gap: 6 }}>
            <Button small variant="primary" disabled={saveDepts.isPending || (!deptDirty && view.deptSource === 'DREAM')} onClick={() => saveDepts.mutate()}>
              부서 차례 저장
            </Button>
            {deptDirty && (
              <Button small onClick={() => setOrder(shown.map((d) => d.departmentId))}>
                되돌리기
              </Button>
            )}
          </div>
        </div>

        {dept && (
          <div style={{ minWidth: 280 }}>
            <div style={{ marginBottom: 6, display: 'flex', gap: 8, alignItems: 'center' }}>
              <b>항목 차례</b>
              <Select
                value={dept.departmentId}
                onChange={(e) => setPick(Number(e.target.value))}
                style={{ minWidth: 180 }}
              >
                {shown.map((d) => (
                  <option key={d.departmentId} value={d.departmentId}>
                    {d.label}
                  </option>
                ))}
              </Select>
            </div>
            <div className="hint" style={{ marginBottom: 4 }}>
              {prioritySourceLabel(dept.itemSource)} · 부서마다 따로입니다
            </div>
            <ol style={{ margin: 0, paddingLeft: 20 }}>
              {items.map((c, i) => (
                <li key={c} style={{ marginBottom: 4 }}>
                  <span style={{ display: 'inline-block', minWidth: 90 }}>{name(c)}</span>
                  <Button small disabled={i === 0} onClick={() => setItems(move(items, i, -1))} aria-label="위로">
                    ↑
                  </Button>{' '}
                  <Button
                    small
                    disabled={i === items.length - 1}
                    onClick={() => setItems(move(items, i, 1))}
                    aria-label="아래로"
                  >
                    ↓
                  </Button>
                </li>
              ))}
            </ol>
            <div style={{ marginTop: 8 }}>
              <Button
                small
                variant="primary"
                disabled={saveItems.isPending || (!itemDirty && dept.itemSource === 'DREAM')}
                onClick={() => saveItems.mutate()}
              >
                이 부서 항목 차례 저장
              </Button>
            </div>
          </div>
        )}
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 배분 결과 미리보기

function ResultCard({ a }: { a: DreamAllocation }) {
  const [open, setOpen] = useState<number | null>(null)
  const [onlyChanged, setOnlyChanged] = useState(false)
  const changed = (s: DreamAllocStudent) => s.lines.some((l) => l.before.some((v, i) => v !== l.after[i]))
  const rows = onlyChanged ? a.students.filter(changed) : a.students
  const selected = a.students.find((s) => s.key === open) ?? null
  return (
    <Card
      title="배분 결과 미리보기"
      flush
      actions={
        <Select
          value={onlyChanged ? 'changed' : ''}
          onChange={(e) => setOnlyChanged(e.target.value === 'changed')}
          style={{ width: 150 }}
        >
          <option value="">학생 전체</option>
          <option value="changed">바뀐 학생만</option>
        </Select>
      }
    >
      <SummaryTable a={a} />
      <div className="tableWrap" style={{ maxHeight: 420 }}>
        <table className="table">
          <thead>
            <tr>
              <th className="left" rowSpan={2}>
                학생
              </th>
              <th colSpan={4}>최신 정산 기준액</th>
              <th colSpan={3}>원본 합계</th>
              <th colSpan={3}>새 합계</th>
              <th rowSpan={2}>상태</th>
            </tr>
            <tr>
              <th className="num">정산금액</th>
              <th className="num">이용권</th>
              <th className="num">자유수강권</th>
              <th className="num">개인부담</th>
              <th className="num">개인부담</th>
              <th className="num">이용권</th>
              <th className="num">자유수강권</th>
              <th className="num">개인부담</th>
              <th className="num">이용권</th>
              <th className="num">자유수강권</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((s) => (
              <tr
                key={s.key}
                className={open === s.key ? 'on' : undefined}
                onClick={() => setOpen(open === s.key ? null : s.key)}
                style={{ cursor: 'pointer' }}
              >
                <td className="left">{s.studentLabel}</td>
                <td className="num">{won(s.amount.total)}</td>
                <td className="num">{won(s.amount.voucher)}</td>
                <td className="num">{won(s.amount.freeVoucher)}</td>
                <td className="num">{won(s.amount.personal)}</td>
                <td className="num">{won(s.before.o)}</td>
                <td className="num">{won(s.before.p)}</td>
                <td className="num">{won(s.before.q)}</td>
                <td className="num">{won(s.after.o)}</td>
                <td className="num">{won(s.after.p)}</td>
                <td className="num">{won(s.after.q)}</td>
                <td>
                  {changed(s) ? (
                    <span className="tag tag--voucher">재배분으로 변경</span>
                  ) : (
                    <span className="tag tag--plain">원본과 같음</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {selected && <StudentAlloc s={selected} />}
    </Card>
  )
}

function SummaryTable({ a }: { a: DreamAllocation }) {
  const rows: [string, keyof DreamTotals][] = [
    ['정산금액', 'n'],
    ['개인부담금', 'o'],
    ['이용권', 'p'],
    ['자유수강권', 'q'],
  ]
  return (
    <div style={{ padding: 12 }}>
      <table className="table" style={{ maxWidth: 640 }}>
        <thead>
          <tr>
            <th className="left">파일 전체</th>
            <th className="num">원본</th>
            <th className="num">새 값</th>
            <th className="num">최신 정산</th>
            <th>검산</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([label, k]) => (
            <tr key={k}>
              <td className="left">{label}</td>
              <td className="num">{won(a.before[k])}</td>
              <td className="num">{won(a.after[k])}</td>
              <td className="num">{won(a.settlement[k])}</td>
              <td>{a.after[k] === a.settlement[k] ? <span className="tag tag--free">일치</span> : <span className="tag tag--bad">불일치</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** 값이 바뀐 칸 — 경고가 아니라 '재배분으로 바뀜' */
const CHANGED = { background: 'var(--blue-50)', fontWeight: 600 } as const

function StudentAlloc({ s }: { s: DreamAllocStudent }) {
  const lines = [...s.lines].sort((x, y) => x.seq - y.seq)
  return (
    <div style={{ padding: 12, borderTop: '1px solid var(--gray-200)' }}>
      <div style={{ marginBottom: 8 }}>
        <b>{s.studentLabel}</b> — 정산금액 {won(s.amount.total)} · 이용권 {won(s.amount.voucher)} · 자유수강권{' '}
        {won(s.amount.freeVoucher)} · 개인부담 {won(s.amount.personal)}
      </div>
      <div className="hint" style={{ marginBottom: 8 }}>
        적용된 차례: 이용권 → 자유수강권 순서로, 부서{' '}
        {s.order.map((d, i) => (
          <Fragment key={d.label}>
            {i > 0 && ' → '}
            <b>{d.label}</b>({d.items.join(' · ')})
          </Fragment>
        ))}
      </div>
      <table className="table">
        <thead>
          <tr>
            <th>배분 차례</th>
            <th>Excel 행</th>
            <th className="left">부서</th>
            <th>항목</th>
            <th className="num">정산금액(N)</th>
            <th className="num">개인부담금(O)</th>
            <th className="num">이용권(P)</th>
            <th className="num">자유수강권(Q)</th>
          </tr>
        </thead>
        <tbody>
          {lines.map((l) => (
            <tr key={l.sourceRow}>
              <td>{l.seq}</td>
              <td>{l.sourceRow}</td>
              <td className="left">{l.deptLabel}</td>
              <td>{l.itemName}</td>
              <td className="num">{won(l.amountN)}</td>
              {[0, 1, 2].map((i) => (
                <td key={i} className="num" style={l.before[i] !== l.after[i] ? CHANGED : undefined}>
                  {l.before[i] !== l.after[i] ? `${won(l.before[i])} → ${won(l.after[i])}` : won(l.after[i])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="hint" style={{ marginTop: 6 }}>
        색이 들어간 칸은 재배분으로 값이 바뀐 칸입니다(원본이 틀렸다는 뜻이 아닙니다). 정산금액(N)은 바뀌지
        않습니다.
      </div>
    </div>
  )
}
