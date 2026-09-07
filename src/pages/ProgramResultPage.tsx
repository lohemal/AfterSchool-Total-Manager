/**
 * 방과후 이용권 · 자유수강권 탭 (요구사항 §17·§18).
 *
 * 두 화면의 모양이 같아 하나로 쓴다. 다른 점은 하나뿐이다 —
 * **이용권에는 초과금 열이 있고, 자유수강권에는 없다.** 자유수강권을 다 쓴 뒤의
 * 금액은 학부모 부담이 되어 수익자 탭으로 가기 때문이다.
 *
 * 오른쪽 지원금 칸은 정산 당시에 실제로 쓰인 값(`settlement_budget`)이다.
 * 화면에서 다시 계산하지 않는다.
 */

import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { ExportButton } from '@/components/ExportButton'
import { StudentAllocModal } from '@/components/StudentAllocModal'
import { Button, Card, Empty, Field, Notice, Search } from '@/components/ui'
import { api } from '@/ipc/api'
import type { ProgramCode, ProgramRow, SettleExportKind } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { SettleGuard } from './SettlementPage'

export function VoucherPage() {
  return (
    <ProgramResult
      program="VOUCHER"
      exportKind="voucher"
      title="방과후 이용권"
      hint="지원받은 금액과 한도를 넘어 학부모 부담이 된 금액을 함께 봅니다."
      showOver
    />
  )
}

export function FreeVoucherPage() {
  return (
    <ProgramResult
      program="FREE_VOUCHER"
      exportKind="free_voucher"
      title="자유수강권"
      hint="지원한도를 넘은 금액은 여기 나오지 않고 수익자 탭으로 갑니다."
    />
  )
}

function ProgramResult({
  program,
  exportKind,
  title,
  hint,
  showOver,
}: {
  program: ProgramCode
  exportKind: SettleExportKind
  title: string
  hint: string
  showOver?: boolean
}) {
  const app = useApp()
  const items = app.boot.costItems
  const wsId = app.workspaceId
  const [query, setQuery] = useState('')
  const [detail, setDetail] = useState<ProgramRow | null>(null)

  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const list = useQuery({
    queryKey: ['settle-program', wsId, program],
    queryFn: () => api.settlementProgram(wsId!, program),
    enabled: wsId !== null,
  })

  const rows = useMemo(() => {
    const q = query.trim()
    const out = list.data ?? []
    return q ? out.filter((r) => r.name.includes(q)) : out
  }, [list.data, query])

  const usedOf = (r: ProgramRow, code: string) =>
    r.used.find((f) => f.itemCode === code)?.amount ?? 0
  const overOf = (r: ProgramRow, code: string) =>
    r.over.find((f) => f.itemCode === code)?.amount ?? 0

  const columns: Column<ProgramRow>[] = [
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((r) => r.grade), render: (r) => r.grade },
    { key: 'classNo', head: '반', width: 54, sort: cmp.classNo((r) => r.classNo), render: (r) => r.classNo },
    { key: 'studentNo', head: '번호', width: 54, sort: cmp.num((r) => r.studentNo), render: (r) => r.studentNo },
    { key: 'name', head: '이름', width: 96, sort: cmp.text((r) => r.name), render: (r) => r.name },

    ...items.map<Column<ProgramRow>>((it) => ({
      key: `u-${it.code}`,
      head: it.name,
      align: 'num' as const,
      width: 88,
      sort: cmp.num((r) => usedOf(r, it.code)),
      render: (r) => won(usedOf(r, it.code)),
    })),
    {
      key: 'usedTotal',
      head: '사용 합계',
      align: 'num',
      width: 100,
      sort: cmp.num((r) => r.usedTotal),
      render: (r) => <b>{won(r.usedTotal)}</b>,
    },

    ...(showOver
      ? [
          ...items.map<Column<ProgramRow>>((it) => ({
            key: `o-${it.code}`,
            head: `초과 ${it.name}`,
            align: 'num' as const,
            width: 92,
            sort: cmp.num((r) => overOf(r, it.code)),
            render: (r) => won(overOf(r, it.code)),
          })),
          {
            key: 'overTotal',
            head: '초과 합계',
            align: 'num' as const,
            width: 100,
            sort: cmp.num((r: ProgramRow) => r.overTotal),
            render: (r: ProgramRow) => (
              <b style={{ color: r.overTotal > 0 ? 'var(--amber-600)' : undefined }}>
                {won(r.overTotal)}
              </b>
            ),
          },
        ]
      : []),

    {
      key: 'period',
      head: '지원기간',
      width: 96,
      render: (r) => r.budget?.periodName || <span className="muted">연간</span>,
    },
    {
      key: 'available',
      head: '기간 사용 가능액',
      align: 'num',
      width: 120,
      render: (r) => (r.budget ? won(r.budget.available) : <span className="muted">—</span>),
    },
    {
      key: 'before',
      head: '이전 사용액',
      align: 'num',
      width: 104,
      render: (r) => (r.budget ? won(r.budget.usedInPeriodBefore) : <span className="muted">—</span>),
    },
    {
      key: 'left',
      head: '기간 잔액',
      align: 'num',
      width: 104,
      render: (r) => (r.budget ? won(r.budget.periodLeft) : <span className="muted">—</span>),
    },
    {
      key: 'annual',
      head: '연간 누적',
      align: 'num',
      width: 104,
      render: (r) => (r.budget ? won(r.budget.annualUsed) : <span className="muted">—</span>),
    },
    {
      key: 'annualLeft',
      head: '연간 잔액',
      align: 'num',
      width: 104,
      render: (r) => (r.budget ? won(r.budget.annualLeft) : <span className="muted">—</span>),
    },
  ]

  const usedSum = rows.reduce((s, r) => s + r.usedTotal, 0)
  const overSum = rows.reduce((s, r) => s + r.overTotal, 0)
  const carried = rows.filter((r) => (r.budget?.carryIn ?? 0) > 0).length

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">{title}</h1>
        </div>
        <Card>
          <Empty title="작업공간이 없습니다">먼저 작업공간을 만들어 주세요.</Empty>
        </Card>
      </div>
    )
  }

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">{title}</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — {hint}
          </p>
        </div>
        <div className="page__actions">
          <ExportButton
            kind={exportKind}
            workspaceId={wsId}
            fresh={status.data?.state === 'FRESH'}
          />
        </div>
      </div>

      <SettleGuard>
        {carried > 0 && (
          <Notice tone="info">
            이전 지원기간에서 이월된 금액이 있는 학생이 <b>{carried}명</b> 있습니다. 이월액은
            기간 사용 가능액에 이미 더해져 있습니다.
          </Notice>
        )}

        <Card flush title="명단">
          <div style={{ padding: 12 }}>
            <div className="toolbar">
              <Field label="검색">
                <Search value={query} onValue={setQuery} />
              </Field>
              <Button onClick={() => setQuery('')}>초기화</Button>
              <span className="toolbar__spacer" />
              <span className="hint">
                지원금 칸은 정산 당시에 쓰인 값입니다. 화면에서 다시 계산하지 않습니다.
              </span>
            </div>
          </div>

          <DataTable
            rows={rows}
            columns={columns}
            getId={(r) => r.studentId}
            onRowClick={(r) => setDetail(r)}
            empty={list.isLoading ? '불러오는 중…' : `${title} 대상 자료가 없습니다.`}
            foot={
              <>
                <span>
                  모두 <b>{rows.length}</b>명 — 줄을 누르면 부서별 상세가 열립니다
                </span>
                <span className="toolbar__spacer" />
                <span>
                  사용 합계 <b style={{ color: 'var(--navy-800)' }}>{won(usedSum)}</b>
                </span>
                {showOver && (
                  <span>
                    초과 합계 <b style={{ color: 'var(--amber-600)' }}>{won(overSum)}</b>
                  </span>
                )}
              </>
            }
          />
        </Card>
      </SettleGuard>

      {detail && (
        <StudentAllocModal
          workspaceId={wsId}
          studentId={detail.studentId}
          title={`학년 반 번 `}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  )
}
