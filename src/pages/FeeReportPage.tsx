/**
 * 행정자료 › 학생별 징수 내역.
 *
 * **정산 결과가 아니다.** 학생에게 발생한 최종 수강료(`charge`)를 그대로 보여
 * 준다. 그래서 정산이 없거나 재정산 필요 상태여도 조회·내려받기가 된다 —
 * 정산 **전에** 금액을 대조하는 자료이기 때문이다.
 *
 * 지원금이 어느 재원에서 나가는지는 수익자 · 방과후 이용권 · 자유수강권 화면에서
 * 본다. 여기에는 그 열을 넣지 않는다.
 *
 * ## 학생당 한 줄이다 (v0.1.4)
 *
 * 이 메뉴의 목적은 **한 학생에게 모두 얼마를 징수하는가**다. 그래서 목록은
 * 학생당 한 줄이고, 금액은 그 학생이 듣는 모든 부서를 항목별로 더한 값이다.
 * 줄을 누르면 부서별로 갈라 본다.
 *
 * 한 학생에게 수강중과 취소가 함께 있을 수 있으므로 목록에 수강상태 열을 두지
 * 않는다. 상태는 상세에서 부서별로 확인한다.
 *
 * ## 필터는 학생을 찾는 조건이다
 *
 * 부서나 수강상태로 걸러도 그것은 *학생을 고르는* 조건이고, 금액은 찾은 학생의
 * **전체** 합계다. 조건 문구에 `로봇과학A반 수강`처럼 적어 그 뜻을 드러낸다.
 *
 * 읽기 전용이다. 금액 수정은 수강생 명단이나 부서정보에서 한다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Notice, Search, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Enrollment, EnrollmentFilter, StudentSumRow } from '@/ipc/types'
import { compareClassNo, studentLabel, supportLabel, won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

/**
 * 사람이 읽을 조건 문구. 파일 이름과 Excel 합계 줄에 들어간다.
 *
 * 부서·상태는 **학생을 찾는 조건**이므로 `수강`·`있음`을 붙여 적는다. 그러지
 * 않으면 "로봇과학"이라고만 적혀 그 부서 금액만 담긴 것처럼 읽힌다.
 */
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
  if (f.deptLabel) parts.push(`${f.deptLabel} 수강`)
  if (f.status === 'ACTIVE') parts.push('수강중 있음')
  if (f.status === 'CANCELLED') parts.push('수강취소 있음')
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
  const [detail, setDetail] = useState<StudentSumRow | null>(null)

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
    () => [...new Set(allRows.map((r) => r.grade))].sort((a, b) => a - b),
    [allRows],
  )
  const classes = useMemo(() => {
    const rows = allRows.filter((r) => !grade || r.grade === Number(grade))
    return [...new Set(rows.map((r) => r.classNo))].sort(compareClassNo)
  }, [allRows, grade])

  const dept = departments.data?.find((d) => d.id === Number(departmentId))
  const deptLabel = dept ? dept.name + dept.className : ''
  const cond = condText({ grade, classNo, deptLabel, status, query })

  const download = useMutation({
    mutationFn: () => api.feeReportExport(wsId!, filter, cond),
    onSuccess: (r) => {
      toast.ok(`${r.name} (학생 ${r.rows}명)을 만들었습니다.`)
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
  const feeOf = (fees: { itemCode: string; amount: number }[], code: string) =>
    fees.find((f) => f.itemCode === code)?.amount ?? 0

  const columns: Column<StudentSumRow>[] = [
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((r) => r.grade), render: (r) => r.grade },
    {
      key: 'classNo',
      head: '반',
      width: 60,
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
      width: 132,
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
      sort: cmp.num((r) => feeOf(r.fees, it.code)),
      render: (r) => won(feeOf(r.fees, it.code)),
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

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">학생별 징수 내역</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 학생 한 명에게 모두 얼마를 징수하는지 봅니다. 여러
            부서를 수강하면 항목별로 더해 한 줄로 보여 줍니다.
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
          <Field label="부서 수강">
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
            <Select value={status} onChange={(e) => setStatus(e.target.value)} style={{ width: 130 }}>
              <option value="">전체</option>
              <option value="ACTIVE">수강중 있음</option>
              <option value="CANCELLED">수강취소 있음</option>
            </Select>
          </Field>
          <Field label="학생 이름">
            <Search value={query} onValue={setQuery} placeholder="이름 일부" />
          </Field>
          <Button onClick={reset}>초기화</Button>
        </div>
        <div className="toolbar__note">
          부서·수강상태는 <b>학생을 찾는 조건</b>입니다. 찾은 학생의 줄에는 그 학생이 듣는{' '}
          <b>모든 부서</b>의 금액이 더해집니다. 줄을 누르면 부서별로 갈라 봅니다.
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
          getId={(r) => r.studentId}
          activeId={detail?.studentId ?? null}
          onRowClick={(r) => setDetail(r)}
          empty="조건에 맞는 학생이 없습니다."
          foot={
            <>
              <span>
                학생 <b>{rows.length}</b>명 — 줄을 누르면 부서별 상세가 열립니다
              </span>
              <span className="toolbar__spacer" />
              {items.map((it) => (
                <span key={it.code}>
                  {it.name} <b>{won(feeOf(report.data?.fees ?? [], it.code))}</b>
                </span>
              ))}
              <span>
                합계{' '}
                <b style={{ color: 'var(--navy-800)' }}>{won(report.data?.total ?? 0)}</b>
              </span>
            </>
          }
        />
      </Card>

      {detail && (
        <DetailModal
          row={detail}
          details={(report.data?.details ?? []).filter((e) => e.studentId === detail.studentId)}
          items={items}
          onClose={() => setDetail(null)}
        />
      )}
    </div>
  )
}

/**
 * 한 학생의 부서별 상세.
 *
 * 목록에 이미 실려 온 자료를 걸러 쓴다 — 따로 다시 물어보면 목록 합계와
 * 상세 합계가 어긋날 수 있다.
 */
function DetailModal({
  row,
  details,
  items,
  onClose,
}: {
  row: StudentSumRow
  details: Enrollment[]
  items: { code: string; name: string }[]
  onClose: () => void
}) {
  const feeOf = (e: Enrollment, code: string) =>
    e.fees.find((f) => f.itemCode === code)?.amount ?? 0

  return (
    <Modal
      wide
      title={`${studentLabel(row)} — 부서별 징수 내역`}
      onClose={onClose}
    >
      <div className="tableWrap">
        <table className="table">
          <thead>
            <tr>
              <th className="left" style={{ width: 160 }}>
                부서
              </th>
              {items.map((it) => (
                <th key={it.code} className="num" style={{ width: 96 }}>
                  {it.name}
                </th>
              ))}
              <th className="num" style={{ width: 108 }}>
                합계
              </th>
              <th style={{ width: 84 }}>수강상태</th>
            </tr>
          </thead>
          <tbody>
            {details.map((e) => (
              <tr key={e.id}>
                <td className="left">{e.deptLabel}</td>
                {items.map((it) => (
                  <td key={it.code} className="num">
                    {won(feeOf(e, it.code))}
                  </td>
                ))}
                <td className="num">
                  <b>{won(e.total)}</b>
                </td>
                <td>
                  {e.status === 'ACTIVE' ? (
                    <span className="tag tag--free">수강중</span>
                  ) : (
                    <span className="tag tag--plain">수강취소</span>
                  )}
                </td>
              </tr>
            ))}
            <tr style={{ background: 'var(--blue-50)' }}>
              <td className="left">
                <b>합계</b>
              </td>
              {items.map((it) => (
                <td key={it.code} className="num">
                  <b>{won(row.fees.find((f) => f.itemCode === it.code)?.amount ?? 0)}</b>
                </td>
              ))}
              <td className="num">
                <b style={{ color: 'var(--navy-800)' }}>{won(row.total)}</b>
              </td>
              <td />
            </tr>
          </tbody>
        </table>
      </div>
      <div className="hint" style={{ marginTop: 8 }}>
        취소한 수강도 그대로 있습니다. 취소할 때 확정한 징수금액이 합계에 들어갑니다.
        금액을 고치려면 [수강 관리 › 수강생 명단]이나 [기초 데이터 › 부서정보]에서 하세요.
      </div>
    </Modal>
  )
}
