/**
 * 행정자료 › 학생별 징수 내역 (v0.1.3, 최종 QA).
 *
 * **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`)를 그대로 보여
 * 준다. 그래서 정산이 없거나 재정산 필요 상태여도 조회·내려받기가 된다 —
 * 정산 **전에** 금액을 대조하는 자료이기 때문이다.
 *
 * 지원금이 어느 재원에서 나가는지는 수익자 · 방과후 이용권 · 자유수강권 화면에서
 * 본다. 여기에는 그 열을 넣지 않는다.
 *
 * 읽기 전용이다. 금액 수정은 수강생 명단에서 한다.
 *
 * 취소한 수강도 사라지지 않는다. 취소할 때 확정한 징수금액을 그대로 보여 주고
 * 상태를 `수강취소`로 적는다. 합계 0원인 취소자도 상태 확인을 위해 보여 준다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Notice, Search, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Enrollment, EnrollmentFilter } from '@/ipc/types'
import { compareClassNo, supportLabel, won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

/** 사람이 읽을 조건 문구. 파일 이름과 시트 첫 줄에 들어간다. */
export function condText(f: {
  grade: string
  classNo: string
  deptLabel: string
  status: string
  query: string
}): string {
  const parts: string[] = []
  if (f.grade) parts.push(`${f.grade}학년`)
  if (f.classNo) parts.push(`${f.classNo}반`)
  if (f.deptLabel) parts.push(f.deptLabel)
  if (f.status === 'ACTIVE') parts.push('수강중')
  if (f.status === 'CANCELLED') parts.push('수강취소')
  if (f.query.trim()) parts.push(`"${f.query.trim()}"`)
  return parts.join('·')
}

export function FeeReportPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const items = app.boot.costItems
  const wsId = app.workspaceId

  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [departmentId, setDepartmentId] = useState('')
  const [status, setStatus] = useState('')
  const [query, setQuery] = useState('')

  const filter: EnrollmentFilter = useMemo(
    () => ({
      departmentId: departmentId ? Number(departmentId) : null,
      grade: grade ? Number(grade) : null,
      classNo: classNo || null,
      status: (status || null) as EnrollmentFilter['status'],
      query: query.trim() || null,
    }),
    [departmentId, grade, classNo, status, query],
  )

  const report = useQuery({
    queryKey: ['fee-report', wsId, filter],
    queryFn: () => api.feeReport(wsId!, filter),
    enabled: wsId !== null,
  })

  const departments = useQuery({
    queryKey: ['departments', wsId, ''],
    queryFn: () => api.departmentList(wsId!),
    enabled: wsId !== null,
  })

  // 필터 후보는 전체에서 뽑는다 — 거른 뒤에 뽑으면 후보가 사라진다.
  const all = useQuery({
    queryKey: ['fee-report-all', wsId],
    queryFn: () => api.feeReport(wsId!, {}),
    enabled: wsId !== null,
  })
  const allRows = all.data?.rows ?? []
  const grades = useMemo(
    () => [...new Set(allRows.map((e) => e.grade))].sort((a, b) => a - b),
    [allRows],
  )
  const classes = useMemo(() => {
    const rows = allRows.filter((e) => !grade || e.grade === Number(grade))
    return [...new Set(rows.map((e) => e.classNo))].sort(compareClassNo)
  }, [allRows, grade])

  const dept = departments.data?.find((d) => d.id === Number(departmentId))
  const deptLabel = dept ? dept.name + dept.className : ''
  const cond = condText({ grade, classNo, deptLabel, status, query })

  const download = useMutation({
    mutationFn: () => api.feeReportExport(wsId!, filter, cond),
    onSuccess: (r) => {
      toast.ok(`${r.name} (${r.rows}건)을 만들었습니다.`)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function reset() {
    setGrade('')
    setClassNo('')
    setDepartmentId('')
    setStatus('')
    setQuery('')
  }

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">학생별 징수 내역</h1>
        </div>
        <div className="card">
          <div className="card__body">
            <Empty title="작업공간이 없습니다">
              수강 자료는 작업공간에 속합니다. [작업공간]에서 먼저 하나 만들어 주세요.
            </Empty>
          </div>
        </div>
      </div>
    )
  }

  const rows = report.data?.rows ?? []

  const columns: Column<Enrollment>[] = [
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((e) => e.grade), render: (e) => e.grade },
    {
      key: 'classNo',
      head: '반',
      width: 60,
      sort: cmp.classNo((e) => e.classNo),
      render: (e) => e.classNo,
    },
    {
      key: 'studentNo',
      head: '번호',
      width: 54,
      sort: cmp.num((e) => e.studentNo),
      render: (e) => e.studentNo,
    },
    { key: 'name', head: '이름', width: 96, sort: cmp.text((e) => e.name), render: (e) => e.name },
    {
      key: 'programs',
      head: '지원유형',
      width: 132,
      render: (e) => {
        const v = supportLabel(e.programs)
        return <span className={`tag tag--${v.tone}`}>{v.text}</span>
      },
    },
    {
      key: 'dept',
      head: '부서명',
      width: 140,
      align: 'left',
      sort: cmp.text((e) => e.deptLabel),
      render: (e) => e.deptLabel,
    },
    ...items.map<Column<Enrollment>>((it) => ({
      key: it.code,
      head: it.name,
      align: 'num' as const,
      width: 88,
      sort: cmp.num((e) => e.fees.find((f) => f.itemCode === it.code)?.amount ?? 0),
      render: (e) => won(e.fees.find((f) => f.itemCode === it.code)?.amount ?? 0),
    })),
    {
      key: 'total',
      head: '합계',
      align: 'num',
      width: 100,
      sort: cmp.num((e) => e.total),
      render: (e) => <b>{won(e.total)}</b>,
    },
    {
      key: 'status',
      head: '수강상태',
      width: 84,
      render: (e) =>
        e.status === 'ACTIVE' ? (
          <span className="tag tag--free">수강중</span>
        ) : (
          <span className="tag tag--plain">수강취소</span>
        ),
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">학생별 징수 내역</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 학생에게 발생한 최종 수강료입니다. 한 학생이 여러
            부서를 수강하면 부서별로 한 줄씩 나옵니다.
          </p>
        </div>
        <div className="page__actions">
          <Button
            variant="primary"
            onClick={() => download.mutate()}
            disabled={download.isPending || rows.length === 0}
          >
            {download.isPending ? '만드는 중…' : 'Excel 내려받기'}
          </Button>
        </div>
      </div>

      <Notice tone="info">
        이 금액은 <b>지원금 배분 전</b> 금액입니다. 이용권·자유수강권으로 실제 어느 재원에서
        나가는지는 [수익자] · [방과후 이용권] · [자유수강권] 화면에서 확인하세요.
        정산을 만들지 않았거나 재정산이 필요한 상태여도 이 자료는 볼 수 있습니다.
      </Notice>

      <Card flush title="징수 내역">
        <div className="toolbar">
          <Field label="학년">
            <Select
              value={grade}
              onChange={(e) => {
                setGrade(e.target.value)
                setClassNo('')
              }}
              style={{ width: 96 }}
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
            <Select value={classNo} onChange={(e) => setClassNo(e.target.value)} style={{ width: 96 }}>
              <option value="">전체</option>
              {classes.map((c) => (
                <option key={c} value={c}>
                  {c}반
                </option>
              ))}
            </Select>
          </Field>
          <Field label="부서">
            <Select
              value={departmentId}
              onChange={(e) => setDepartmentId(e.target.value)}
              style={{ width: 160 }}
            >
              <option value="">전체</option>
              {(departments.data ?? []).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                  {d.className ? ' ' + d.className : ''}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="수강상태">
            <Select value={status} onChange={(e) => setStatus(e.target.value)} style={{ width: 110 }}>
              <option value="">전체</option>
              <option value="ACTIVE">수강중</option>
              <option value="CANCELLED">수강취소</option>
            </Select>
          </Field>
          <Field label="학생 이름">
            <Search value={query} onValue={setQuery} placeholder="이름 일부" />
          </Field>
          <Button onClick={reset}>초기화</Button>
        </div>
        <div className="toolbar__note">
          조회 학생 <b>{report.data?.students ?? 0}명</b> · 수강{' '}
          <b>{report.data?.enrollments ?? 0}건</b> · 총 징수금액{' '}
          <b style={{ color: 'var(--navy-800)' }}>{won(report.data?.total ?? 0)}원</b>
          {cond && <> · 조건 {cond}</>}
        </div>

        <DataTable
          rows={rows}
          columns={columns}
          getId={(e) => e.id}
          empty="조건에 맞는 수강 자료가 없습니다."
        />
      </Card>
    </div>
  )
}
