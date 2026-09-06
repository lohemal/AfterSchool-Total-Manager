/**
 * 수익자 탭 (요구사항 §2).
 *
 * 실제 학부모 부담이 생긴 줄만 보여 준다.
 * **방과후 이용권으로 정상 지원된 금액은 여기 나오지 않는다.**
 *
 * 일반 수익자와 지원제도 소진 후 발생한 부담을 한 표에 두되 `origin`으로
 * 구분해 보여 준다. 내부 코드(`PLAIN` 등)는 화면에 쓰지 않는다.
 *
 * Excel은 **화면 필터와 무관하게 전체 정산 결과**를 내려받는다 (요구사항 §5).
 */

import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { ExportButton } from '@/components/ExportButton'
import { StudentAllocModal } from '@/components/StudentAllocModal'
import { Button, Card, Empty, Field, Notice, Search, Select } from '@/components/ui'
import { api } from '@/ipc/api'
import type { SelfPayRow } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

import { SettleGuard } from './SettlementPage'

const ORIGINS = [
  { code: '', label: '전체' },
  { code: 'PLAIN', label: '일반 수익자' },
  { code: 'VOUCHER', label: '이용권 소진 후 발생' },
  { code: 'FREE', label: '자유수강권 소진 후 발생' },
]

export function SelfPayPage() {
  const app = useApp()
  const items = app.boot.costItems
  const wsId = app.workspaceId

  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [deptId, setDeptId] = useState('')
  const [origin, setOrigin] = useState('')
  const [query, setQuery] = useState('')
  const [detail, setDetail] = useState<SelfPayRow | null>(null)

  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const list = useQuery({
    queryKey: ['settle-self-pay', wsId],
    queryFn: () => api.settlementSelfPay(wsId!),
    enabled: wsId !== null,
  })

  const all = list.data ?? []
  const grades = useMemo(
    () => [...new Set(all.map((r) => r.grade))].sort((a, b) => a - b),
    [all],
  )
  const classes = useMemo(() => {
    const rows = all.filter((r) => !grade || r.grade === Number(grade))
    return [...new Set(rows.map((r) => r.classNo))].sort((a, b) => a - b)
  }, [all, grade])
  const depts = useMemo(() => {
    const map = new Map<number, string>()
    all.forEach((r) => map.set(r.departmentId, r.deptLabel))
    return [...map.entries()].sort((a, b) => a[1].localeCompare(b[1], 'ko'))
  }, [all])

  const rows = useMemo(() => {
    let out = all
    if (grade) out = out.filter((r) => r.grade === Number(grade))
    if (classNo) out = out.filter((r) => r.classNo === Number(classNo))
    if (deptId) out = out.filter((r) => r.departmentId === Number(deptId))
    if (origin === 'PLAIN') out = out.filter((r) => r.originPlain > 0)
    if (origin === 'VOUCHER') out = out.filter((r) => r.originVoucher > 0)
    if (origin === 'FREE') out = out.filter((r) => r.originFree > 0)
    const q = query.trim()
    if (q) out = out.filter((r) => r.name.includes(q) || r.deptLabel.includes(q))
    return out
  }, [all, grade, classNo, deptId, origin, query])

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
      head: '발생원인',
      width: 150,
      render: (r) => {
        if (r.originVoucher > 0) return <span className="tag tag--warn">이용권 소진 후</span>
        if (r.originFree > 0) return <span className="tag tag--free">자유수강권 소진 후</span>
        return <span className="tag tag--plain">일반 수익자</span>
      },
    },
  ]

  const sumItem = (code: string) => rows.reduce((s, r) => s + feeOf(r, code), 0)
  const total = rows.reduce((s, r) => s + r.total, 0)
  const filtered = rows.length !== all.length

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
        <div className="page__actions">
          <ExportButton kind="self_pay" workspaceId={wsId} fresh={status.data?.state === 'FRESH'} />
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
              <Field label="학년">
                <Select
                  value={grade}
                  onChange={(e) => {
                    setGrade(e.target.value)
                    setClassNo('')
                  }}
                  style={{ width: 88 }}
                >
                  <option value="">전체</option>
                  {grades.map((g) => (
                    <option key={g} value={g}>
                      {g}학년
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="반">
                <Select value={classNo} onChange={(e) => setClassNo(e.target.value)} style={{ width: 88 }}>
                  <option value="">전체</option>
                  {classes.map((c) => (
                    <option key={c} value={c}>
                      {c}반
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="부서">
                <Select value={deptId} onChange={(e) => setDeptId(e.target.value)} style={{ width: 168 }}>
                  <option value="">전체</option>
                  {depts.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="발생원인">
                <Select value={origin} onChange={(e) => setOrigin(e.target.value)} style={{ width: 180 }}>
                  {ORIGINS.map((o) => (
                    <option key={o.code} value={o.code}>
                      {o.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="검색">
                <Search value={query} onValue={setQuery} width={160} />
              </Field>
              <Button
                onClick={() => {
                  setGrade('')
                  setClassNo('')
                  setDeptId('')
                  setOrigin('')
                  setQuery('')
                }}
              >
                초기화
              </Button>
            </div>
            {filtered && (
              <div className="hint" style={{ marginTop: 8 }}>
                화면에 {rows.length}건이 걸려 있습니다. <b>Excel은 전체 {all.length}건</b>을
                내려받습니다.
              </div>
            )}
          </div>

          <DataTable
            rows={rows}
            columns={columns}
            getId={(r) => r.studentId * 100000 + r.departmentId}
            onRowClick={(r) => setDetail(r)}
            empty={list.isLoading ? '불러오는 중…' : '학부모 부담이 발생한 자료가 없습니다.'}
            foot={
              <>
                <span>
                  모두 <b>{rows.length}</b>건 — 줄을 누르면 정산 상세가 열립니다
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

      {detail && (
        <StudentAllocModal
          workspaceId={wsId}
          studentId={detail.studentId}
          title={`${detail.grade}학년 ${detail.classNo}반 ${detail.studentNo}번 ${detail.name}`}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  )
}
