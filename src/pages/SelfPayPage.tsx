/**
 * 수익자 탭 (요구사항 §16).
 *
 * 실제 학부모 부담이 생긴 줄만 보여 준다.
 * **방과후 이용권으로 정상 지원된 금액은 여기 나오지 않는다.**
 *
 * 일반 수익자와 지원제도 소진 후 발생한 부담을 한 표에 두되, `origin`으로
 * 구분해 보여 주고 필터로 나눠 볼 수 있게 한다.
 */

import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { Button, Card, Empty, Field, Notice, Search, Select } from '@/components/ui'
import { api } from '@/ipc/api'
import type { SelfPayRow } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { SettleGuard } from './SettlementPage'

const ORIGINS = [
  { code: '', label: '전체' },
  { code: 'PLAIN', label: '일반 수익자' },
  { code: 'VOUCHER', label: '이용권 소진 후' },
  { code: 'FREE', label: '자유수강권 소진 후' },
]

export function SelfPayPage() {
  const app = useApp()
  const items = app.boot.costItems
  const wsId = app.workspaceId

  const [query, setQuery] = useState('')
  const [origin, setOrigin] = useState('')

  const list = useQuery({
    queryKey: ['settle-self-pay', wsId],
    queryFn: () => api.settlementSelfPay(wsId!),
    enabled: wsId !== null,
  })

  const rows = useMemo(() => {
    let out = list.data ?? []
    const q = query.trim()
    if (q) {
      out = out.filter((r) => r.name.includes(q) || r.deptLabel.includes(q))
    }
    if (origin === 'PLAIN') out = out.filter((r) => r.originPlain > 0)
    if (origin === 'VOUCHER') out = out.filter((r) => r.originVoucher > 0)
    if (origin === 'FREE') out = out.filter((r) => r.originFree > 0)
    return out
  }, [list.data, query, origin])

  const feeOf = (r: SelfPayRow, code: string) =>
    r.fees.find((f) => f.itemCode === code)?.amount ?? 0

  const columns: Column<SelfPayRow>[] = [
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((r) => r.grade), render: (r) => r.grade },
    { key: 'classNo', head: '반', width: 54, sort: cmp.num((r) => r.classNo), render: (r) => r.classNo },
    { key: 'studentNo', head: '번호', width: 54, sort: cmp.num((r) => r.studentNo), render: (r) => r.studentNo },
    { key: 'name', head: '이름', width: 96, sort: cmp.text((r) => r.name), render: (r) => r.name },
    {
      key: 'dept',
      head: '부서',
      align: 'left',
      width: 150,
      sort: cmp.text((r) => r.deptLabel),
      render: (r) => r.deptLabel,
    },
    ...items.map<Column<SelfPayRow>>((it) => ({
      key: it.code,
      head: it.name,
      align: 'num' as const,
      width: 92,
      sort: cmp.num((r) => feeOf(r, it.code)),
      render: (r) => won(feeOf(r, it.code)),
    })),
    {
      key: 'total',
      head: '합계',
      align: 'num',
      width: 104,
      sort: cmp.num((r) => r.total),
      render: (r) => <b>{won(r.total)}</b>,
    },
    {
      key: 'origin',
      head: '구분',
      width: 130,
      render: (r) => {
        if (r.originVoucher > 0) return <span className="tag tag--warn">이용권 소진 후</span>
        if (r.originFree > 0) return <span className="tag tag--free">자유수강권 소진 후</span>
        return <span className="tag tag--plain">일반 수익자</span>
      },
    },
  ]

  const sumItem = (code: string) => rows.reduce((s, r) => s + feeOf(r, code), 0)
  const total = rows.reduce((s, r) => s + r.total, 0)

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">수익자</h1>
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
          <h1 className="page__title">수익자</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 실제 학부모 부담이 발생한 금액입니다.
          </p>
        </div>
      </div>

      <SettleGuard>
        <Notice tone="info">
          방과후 이용권으로 <b>정상 지원된 금액은 여기에 나오지 않습니다.</b> 일반 수익자
          부담금과, 지원제도를 다 쓴 뒤 학부모 부담으로 넘어온 금액만 모았습니다.
        </Notice>

        <Card flush title="명단">
          <div style={{ padding: 12 }}>
            <div className="toolbar">
              <Field label="구분">
                <Select value={origin} onChange={(e) => setOrigin(e.target.value)} style={{ width: 170 }}>
                  {ORIGINS.map((o) => (
                    <option key={o.code} value={o.code}>
                      {o.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="검색">
                <Search value={query} onValue={setQuery} />
              </Field>
              <Button
                onClick={() => {
                  setQuery('')
                  setOrigin('')
                }}
              >
                초기화
              </Button>
            </div>
          </div>

          <DataTable
            rows={rows}
            columns={columns}
            getId={(r) => r.studentId * 100000 + r.departmentId}
            empty={list.isLoading ? '불러오는 중…' : '학부모 부담이 발생한 자료가 없습니다.'}
            foot={
              <>
                <span>
                  모두 <b>{rows.length}</b>건
                </span>
                <span className="toolbar__spacer" />
                {items.map((it) => (
                  <span key={it.code}>
                    {it.name} <b>{won(sumItem(it.code))}</b>
                  </span>
                ))}
                <span>
                  합계 <b style={{ color: 'var(--navy-800)' }}>{won(total)}</b>
                </span>
              </>
            }
          />
        </Card>
      </SettleGuard>
    </div>
  )
}
