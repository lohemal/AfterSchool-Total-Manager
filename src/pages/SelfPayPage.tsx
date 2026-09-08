/**
 * 수익자 탭 (요구사항 §2).
 *
 * 실제 학부모 부담이 생긴 학생만 보여 준다.
 * **방과후 이용권으로 정상 지원된 금액은 여기 나오지 않는다.**
 *
 * ## 학생당 한 줄이다 (v0.1.4)
 *
 * 방과후 이용권·자유수강권 탭과 같은 모양이다 — 목록에서 학생이 얼마인지 보고,
 * 눌러서 왜 그 금액인지 본다. 부서와 발생원인은 한 줄에 담기지 않으므로
 * (한 학생이 여러 부서를 듣고 원인이 섞일 수 있다) 상세에서 본다.
 *
 * ## 어디서 나온 돈인가
 *
 * 여기 금액은 **정산 스냅샷의 배분액 가운데 학부모 부담**이다. 학생별 징수
 * 내역(원본 `charge` 전체)과 섞으면 학부모가 실제로 내는 돈이 부풀려진다.
 * 집계 수준만 바뀌었을 뿐이므로 총액은 v0.1.3과 같다.
 *
 * 필터는 **학생을 찾는 조건**이고 금액은 찾은 학생의 전체 부담이다.
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
import type { StudentSumRow } from '@/ipc/types'
import { compareClassNo, studentLabel, supportLabel, won } from '@/lib/format'
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
  const [detail, setDetail] = useState<StudentSumRow | null>(null)

  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const report = useQuery({
    queryKey: ['settle-self-pay', wsId],
    queryFn: () => api.settlementSelfPay(wsId!),
    enabled: wsId !== null,
  })

  const all = report.data?.rows ?? []
  const details = report.data?.details ?? []

  const grades = useMemo(() => [...new Set(all.map((r) => r.grade))].sort((a, b) => a - b), [all])
  const classes = useMemo(() => {
    const rows = all.filter((r) => !grade || r.grade === Number(grade))
    return [...new Set(rows.map((r) => r.classNo))].sort(compareClassNo)
  }, [all, grade])
  const depts = useMemo(() => {
    const map = new Map<number, string>()
    details.forEach((d) => map.set(d.departmentId, d.deptLabel))
    return [...map.entries()].sort((a, b) => a[1].localeCompare(b[1], 'ko'))
  }, [details])

  /**
   * 부서·발생원인은 **학생을 찾는 조건**이다. 조건에 맞는 상세를 가진 학생을
   * 고르고, 금액은 그 학생의 전체 부담을 그대로 보여 준다.
   */
  const rows = useMemo(() => {
    let out = all
    if (grade) out = out.filter((r) => r.grade === Number(grade))
    if (classNo) out = out.filter((r) => r.classNo === classNo)
    if (deptId) {
      const ids = new Set(
        details.filter((d) => d.departmentId === Number(deptId)).map((d) => d.studentId),
      )
      out = out.filter((r) => ids.has(r.studentId))
    }
    if (origin) {
      const has = (d: (typeof details)[number]) =>
        origin === 'PLAIN'
          ? d.originPlain > 0
          : origin === 'VOUCHER'
            ? d.originVoucher > 0
            : d.originFree > 0
      const ids = new Set(details.filter(has).map((d) => d.studentId))
      out = out.filter((r) => ids.has(r.studentId))
    }
    const q = query.trim()
    if (q) {
      const ids = new Set(details.filter((d) => d.deptLabel.includes(q)).map((d) => d.studentId))
      out = out.filter((r) => r.name.includes(q) || ids.has(r.studentId))
    }
    return out
  }, [all, details, grade, classNo, deptId, origin, query])

  const feeOf = (r: StudentSumRow, code: string) =>
    r.fees.find((f) => f.itemCode === code)?.amount ?? 0

  const columns: Column<StudentSumRow>[] = [
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((r) => r.grade), render: (r) => r.grade },
    {
      key: 'classNo',
      head: '반',
      width: 54,
      sort: cmp.classNo((r) => r.classNo),
      render: (r) => r.classNo,
    },
    {
      key: 'studentNo',
      head: '번호',
      width: 54,
      sort: cmp.num((r) => r.studentNo),
      render: (r) => r.studentNo,
    },
    { key: 'name', head: '이름', width: 100, sort: cmp.text((r) => r.name), render: (r) => r.name },
    {
      key: 'programs',
      head: '지원유형',
      width: 128,
      render: (r) => {
        const v = supportLabel(r.programs)
        return <span className={`tag tag--${v.tone}`}>{v.text}</span>
      },
    },
    ...items.map<Column<StudentSumRow>>((it) => ({
      key: it.code,
      head: it.name,
      align: 'num' as const,
      width: 94,
      sort: cmp.num((r) => feeOf(r, it.code)),
      render: (r) => won(feeOf(r, it.code)),
    })),
    {
      key: 'total',
      head: '합계',
      align: 'num',
      width: 108,
      sort: cmp.num((r) => r.total),
      render: (r) => <b>{won(r.total)}</b>,
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
            <b>{app.workspace?.name}</b> — 학생 한 명이 실제로 부담하는 금액입니다.
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
              <Field label="부서 수강">
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
            <div className="toolbar__note">
              부서·발생원인은 <b>학생을 찾는 조건</b>입니다. 찾은 학생의 줄에는 그 학생의{' '}
              <b>전체 부담</b>이 더해집니다.
            </div>
            {filtered && (
              <div className="hint" style={{ marginTop: 8 }}>
                화면에 {rows.length}명이 걸려 있습니다. <b>Excel은 전체 {all.length}명</b>을
                내려받습니다.
              </div>
            )}
          </div>

          <DataTable
            rows={rows}
            columns={columns}
            getId={(r) => r.studentId}
            activeId={detail?.studentId ?? null}
            onRowClick={(r) => setDetail(r)}
            empty={report.isLoading ? '불러오는 중…' : '학부모 부담이 발생한 자료가 없습니다.'}
            foot={
              <>
                <span>
                  모두 <b>{rows.length}</b>명 — 줄을 누르면 정산 상세가 열립니다
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
          title={studentLabel(detail)}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  )
}
