/**
 * 수강 관리 › 추가·취소 관리 (v0.1.5, 설계안 26장).
 *
 * **수강생 명단과 다른 자료다.** 수강생 명단은 *지금 이 학생이 얼마를 내는가*를
 * 다루고, 여기는 *이미 한 번 걷은 뒤에 얼마를 더 걷거나 돌려주어야 하는가*를
 * 다룬다.
 *
 * 그래서 금액이 만든 시점에서 굳는다. 나중에 수강생 명단에서 금액을 고쳐도 이
 * 기록이 저절로 따라 바뀌지 않고, **사람에게 확인을 구한다** — 이미 그 금액으로
 * 학부모에게 안내했을 수 있기 때문이다.
 *
 * 목록은 학생별 징수 내역과 같은 모양이다 — 학생당 한 줄, 누르면 부서별 상세.
 * 추가징수와 환불은 **섞지 않고 탭으로 가른다.**
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { todayISO } from '@/components/AdjustmentToggle'
import { cmp, DataTable, type Column } from '@/components/DataTable'
import { Confirm, Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Input, Notice, Search, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type {
  Adjustment,
  AdjustmentDiff,
  AdjustmentFilter,
  AdjustmentReport,
  StudentSumRow,
} from '@/ipc/types'
import { compareClassNo, studentLabel, won } from '@/lib/format'
import { saveExport } from '@/lib/saveFile'
import { useApp } from '@/lib/useApp'

type Tab = 'ADDITIONAL' | 'REFUND'

/** 사람이 읽을 조건 문구. 파일 이름에 들어간다. */
export function condText(f: {
  from: string
  to: string
  grade: string
  classNo: string
  deptLabel: string
  query: string
}): string {
  const parts: string[] = []
  if (f.from || f.to) parts.push(`${f.from || '처음'}~${f.to || '끝'}`)
  if (f.grade) parts.push(`${f.grade}학년`)
  if (f.classNo) parts.push(`${f.classNo}반`)
  if (f.deptLabel) parts.push(`${f.deptLabel} 대상`)
  if (f.query.trim()) parts.push(`"${f.query.trim()}"`)
  return parts.join('·')
}

export function AdjustmentPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const items = app.boot.costItems
  const wsId = app.workspaceId

  const [tab, setTab] = useState<Tab>('ADDITIONAL')
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [departmentId, setDepartmentId] = useState('')
  const [query, setQuery] = useState('')

  const [detail, setDetail] = useState<StudentSumRow | null>(null)
  const [showDiffs, setShowDiffs] = useState(false)
  const [removing, setRemoving] = useState<Adjustment | null>(null)

  const filter: AdjustmentFilter = useMemo(
    () => ({
      from: from || null,
      to: to || null,
      grade: grade ? Number(grade) : null,
      classNo: classNo || null,
      departmentId: departmentId ? Number(departmentId) : null,
      query: query.trim() || null,
    }),
    [from, to, grade, classNo, departmentId, query],
  )

  const view = useQuery({
    queryKey: ['adjustments', wsId, filter],
    queryFn: () => api.adjustmentView(wsId!, filter),
    enabled: wsId !== null,
  })
  const all = useQuery({
    queryKey: ['adjustments-all', wsId],
    queryFn: () => api.adjustmentView(wsId!, {}),
    enabled: wsId !== null,
  })
  const departments = useQuery({
    queryKey: ['departments', wsId, ''],
    queryFn: () => api.departmentList(wsId!),
    enabled: wsId !== null,
  })

  // 필터 후보는 전체에서 뽑는다 — 거른 뒤에 뽑으면 후보가 사라진다.
  const allRows = useMemo(
    () => [...(all.data?.additional.rows ?? []), ...(all.data?.refund.rows ?? [])],
    [all.data],
  )
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
  const cond = condText({ from, to, grade, classNo, deptLabel, query })

  const report: AdjustmentReport | undefined =
    tab === 'ADDITIONAL' ? view.data?.additional : view.data?.refund
  const 이름 = tab === 'ADDITIONAL' ? '추가징수' : '환불'

  const pending = (view.data?.additional.needsCheck ?? 0) + (view.data?.refund.needsCheck ?? 0)
  const pendingAll = view.data?.needsCheckAll ?? 0

  const download = useMutation({
    mutationFn: () =>
      saveExport(
        () => api.adjustmentExport(wsId!, filter, cond),
        toast,
        (r) => `학생 ${r.rows}줄`,
      ),
  })

  const remove = useMutation({
    mutationFn: (id: number) => api.adjustmentDelete([id]),
    onSuccess: () => {
      toast.ok('기록을 지웠습니다.')
      setRemoving(null)
      setDetail(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  function reset() {
    setFrom('')
    setTo('')
    setGrade('')
    setClassNo('')
    setDepartmentId('')
    setQuery('')
  }

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">추가·취소 관리</h1>
        </div>
        <Card>
          <Empty title="작업공간이 없습니다">
            추가징수·환불은 작업공간에 속합니다. [작업공간]에서 먼저 하나 만들어 주세요.
          </Empty>
        </Card>
      </div>
    )
  }

  const rows = report?.rows ?? []
  const details = report?.details ?? []
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
    { key: 'name', head: '이름', width: 104, sort: cmp.text((r) => r.name), render: (r) => r.name },
    ...items.map<Column<StudentSumRow>>((it) => ({
      key: it.code,
      head: it.name,
      align: 'num' as const,
      width: 96,
      sort: cmp.num((r) => feeOf(r.fees, it.code)),
      render: (r) => won(feeOf(r.fees, it.code)),
    })),
    {
      key: 'total',
      head: '합계',
      align: 'num',
      width: 110,
      sort: cmp.num((r) => r.total),
      render: (r) => <b>{won(r.total)}</b>,
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">추가·취소 관리</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 이미 한 번 걷은 뒤에 생긴 <b>추가징수</b>와{' '}
            <b>환불</b>입니다. 수강생 명단의 현재 금액과는 다른 자료입니다.
          </p>
        </div>
        <div className="page__actions">
          <Button
            variant="primary"
            onClick={() => download.mutate()}
            disabled={download.isPending || pending > 0}
            title={
              pending > 0
                ? '금액 변경 확인이 끝나야 만들 수 있습니다'
                : '추가징수·환불·상세 세 장을 한 파일로 받습니다'
            }
          >
            {download.isPending ? '만드는 중…' : 'Excel 받기'}
          </Button>
        </div>
      </div>

      {pendingAll > 0 && (
        <Notice
          tone="warn"
          actions={
            <Button small onClick={() => setShowDiffs(true)}>
              변경내역 확인
            </Button>
          }
        >
          수강생 명단에서 금액이 변경된 내역이 <b>{pendingAll}건</b> 있습니다. 추가징수·환불
          금액을 확인해 주세요.
          <div className="hint" style={{ marginTop: 4 }}>
            프로그램이 저절로 바꾸지 않습니다. 이미 그 금액으로 안내하셨다면 [기존 금액
            유지]를, 지금 금액이 맞다면 [현재 금액 반영]을 고르시면 됩니다.
          </div>
        </Notice>
      )}

      <Notice tone="info">
        여기 금액은 <b>만든 시점에서 굳습니다.</b> 추가징수는 학생을 넣을 때 확정한 금액,
        환불은 <b>취소 직전 금액 − 취소 후 최종 징수금액</b>입니다. 대상 등록은 수강생
        명단에서 학생을 추가하거나 취소할 때 체크하면 됩니다.
      </Notice>

      <div style={{ display: 'flex', gap: 6, marginBottom: 12 }}>
        {(
          [
            ['ADDITIONAL', '추가징수', view.data?.additional],
            ['REFUND', '환불', view.data?.refund],
          ] as [Tab, string, AdjustmentReport | undefined][]
        ).map(([code, label, r]) => (
          <Button
            key={code}
            variant={tab === code ? 'primary' : 'default'}
            onClick={() => {
              setTab(code)
              setDetail(null)
            }}
          >
            {label}
            {r && r.count > 0 && <span style={{ marginLeft: 6 }}>{r.students}명</span>}
            {r && r.needsCheck > 0 && (
              <span className="tag tag--warn" style={{ marginLeft: 6, padding: '0 6px' }}>
                ● {r.needsCheck}
              </span>
            )}
          </Button>
        ))}
      </div>

      <Card flush title={`${이름} 명단`}>
        <div className="toolbar">
          <Field label="발생일 시작">
            <Input type="date" value={from} onChange={(e) => setFrom(e.target.value)} style={{ width: 150 }} />
          </Field>
          <Field label="발생일 끝">
            <Input type="date" value={to} onChange={(e) => setTo(e.target.value)} style={{ width: 150 }} />
          </Field>
          <Field label="학년">
            <Select
              value={grade}
              onChange={(e) => {
                setGrade(e.target.value)
                setClassNo('')
              }}
              style={{ width: 92 }}
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
            <Select value={classNo} onChange={(e) => setClassNo(e.target.value)} style={{ width: 92 }}>
              <option value="">전체</option>
              {classes.map((c) => (
                <option key={c} value={c}>
                  {c}반
                </option>
              ))}
            </Select>
          </Field>
          <Field label="부서 대상">
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
          <Field label="학생 이름">
            <Search value={query} onValue={setQuery} placeholder="이름 일부" />
          </Field>
          <Button onClick={reset}>초기화</Button>
          <Button
            small
            onClick={() => {
              setFrom(todayISO().slice(0, 8) + '01')
              setTo(todayISO())
            }}
            title="이번 달 1일부터 오늘까지"
          >
            이번 달
          </Button>
        </div>
        <div className="toolbar__note">
          부서는 <b>학생을 찾는 조건</b>입니다. 찾은 학생의 줄에는 그 기간 안의{' '}
          <b>모든 부서</b> {이름}이 더해집니다. 줄을 누르면 부서별로 갈라 봅니다.
        </div>
        <div className="toolbar__note">
          학생 <b>{report?.students ?? 0}명</b> · {이름} <b>{report?.count ?? 0}건</b> · 합계{' '}
          <b style={{ color: 'var(--navy-800)' }}>{won(report?.total ?? 0)}원</b>
          {cond && <> · 조건 {cond}</>}
        </div>

        <DataTable
          rows={rows}
          columns={columns}
          getId={(r) => r.studentId}
          activeId={detail?.studentId ?? null}
          onRowClick={(r) => setDetail(r)}
          empty={
            view.isLoading
              ? '불러오는 중…'
              : `조건에 맞는 ${이름} 기록이 없습니다. 수강생 명단에서 학생을 추가하거나 취소할 때 대상으로 체크하면 여기에 쌓입니다.`
          }
          foot={
            <>
              <span>
                학생 <b>{rows.length}</b>명 — 줄을 누르면 부서별 상세가 열립니다
              </span>
              <span className="toolbar__spacer" />
              {items.map((it) => (
                <span key={it.code}>
                  {it.name} <b>{won(feeOf(report?.fees ?? [], it.code))}</b>
                </span>
              ))}
              <span>
                합계 <b style={{ color: 'var(--navy-800)' }}>{won(report?.total ?? 0)}</b>
              </span>
            </>
          }
        />
      </Card>

      {detail && (
        <DetailModal
          kind={이름}
          row={detail}
          rows={details.filter((a) => a.studentId === detail.studentId)}
          items={items}
          onClose={() => setDetail(null)}
          onRemove={(a) => setRemoving(a)}
        />
      )}

      {showDiffs && (
        <DiffModal
          workspaceId={wsId}
          onClose={() => setShowDiffs(false)}
          onDone={() => void qc.invalidateQueries()}
        />
      )}

      {removing && (
        <Confirm
          danger
          title="기록 삭제"
          message={
            <>
              <b>{removing.studentLabelAt}</b> / {removing.deptLabelAt} 의{' '}
              {removing.kind === 'REFUND' ? '환불' : '추가징수'} 기록({won(removing.total)}원)을
              지웁니다.
              <div className="hint" style={{ marginTop: 6 }}>
                수강 자료와 금액은 그대로입니다. 돈이 오간 기록이므로 지운 뒤에는 되돌릴 수
                없습니다.
              </div>
            </>
          }
          confirmText="지우기"
          busy={remove.isPending}
          onClose={() => setRemoving(null)}
          onConfirm={() => remove.mutate(removing.id)}
        />
      )}
    </div>
  )
}

/** 한 학생의 부서별 상세. 목록에 이미 실려 온 자료를 걸러 쓴다. */
function DetailModal({
  kind,
  row,
  rows,
  items,
  onClose,
  onRemove,
}: {
  kind: string
  row: StudentSumRow
  rows: Adjustment[]
  items: { code: string; name: string }[]
  onClose: () => void
  onRemove: (a: Adjustment) => void
}) {
  const feeOf = (a: Adjustment, code: string) =>
    a.fees.find((f) => f.itemCode === code)?.amount ?? 0

  return (
    <Modal wide title={`${studentLabel(row)} — 부서별 ${kind}`} onClose={onClose}>
      <div className="tableWrap">
        <table className="table">
          <thead>
            <tr>
              <th className="left" style={{ width: 150 }}>
                부서
              </th>
              {items.map((it) => (
                <th key={it.code} className="num" style={{ width: 92 }}>
                  {it.name}
                </th>
              ))}
              <th className="num" style={{ width: 104 }}>
                합계
              </th>
              <th style={{ width: 104 }}>발생일</th>
              <th className="left" style={{ width: 150 }}>
                사유 · 메모
              </th>
              <th style={{ width: 70 }} />
            </tr>
          </thead>
          <tbody>
            {rows.map((a) => (
              <tr key={a.id}>
                <td className="left">
                  {a.deptLabel}
                  {a.needsCheck && (
                    <span className="tag tag--warn" style={{ marginLeft: 6 }}>
                      확인 필요
                    </span>
                  )}
                </td>
                {items.map((it) => (
                  <td key={it.code} className="num">
                    {won(feeOf(a, it.code))}
                  </td>
                ))}
                <td className="num">
                  <b>{won(a.total)}</b>
                </td>
                <td>{a.occurredOn}</td>
                <td className="left">{a.note || <span className="muted">—</span>}</td>
                <td>
                  <Button small variant="danger" onClick={() => onRemove(a)}>
                    삭제
                  </Button>
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
              <td colSpan={3} />
            </tr>
          </tbody>
        </table>
      </div>

      {rows.some((a) => a.kind === 'REFUND') && (
        <>
          <div style={{ fontWeight: 700, margin: '14px 0 6px' }}>환불 계산 근거</div>
          <div className="tableWrap">
            <table className="table">
              <thead>
                <tr>
                  <th className="left" style={{ width: 150 }}>
                    부서
                  </th>
                  <th style={{ width: 90 }}>항목</th>
                  <th className="num">취소 직전</th>
                  <th className="num">취소 후 징수</th>
                  <th className="num">환불액</th>
                </tr>
              </thead>
              <tbody>
                {rows
                  .filter((a) => a.kind === 'REFUND')
                  .flatMap((a) =>
                    a.fees.map((f) => (
                      <tr key={`${a.id}-${f.itemCode}`}>
                        <td className="left">{a.deptLabel}</td>
                        <td>{f.itemName}</td>
                        <td className="num">{won(f.baseAmount)}</td>
                        <td className="num">{won(f.baseAmount - f.amount)}</td>
                        <td className="num">
                          <b>{won(f.amount)}</b>
                        </td>
                      </tr>
                    )),
                  )}
              </tbody>
            </table>
          </div>
        </>
      )}

      <div className="hint" style={{ marginTop: 10, lineHeight: 1.8 }}>
        {rows.some((a) => a.studentLabelAt !== studentLabel(row) || a.deptLabelAt !== a.deptLabel) && (
          <>
            <b>등록 당시 표시</b>{' '}
            {rows
              .map((a) => `${a.studentLabelAt} / ${a.deptLabelAt}`)
              .filter((v, i, arr) => arr.indexOf(v) === i)
              .join(' · ')}
            <br />
          </>
        )}
        {rows.some((a) => a.kind === 'REFUND' && a.enrollmentStatus === 'ACTIVE') && (
          <>
            <b style={{ color: 'var(--red-600)' }}>
              환불 기록이 있는 수강이 다시 수강중입니다.
            </b>{' '}
            되돌린 뒤라면 이 환불이 아직 필요한지 확인해 주세요.
            <br />
          </>
        )}
        금액을 고치려면 [수강 관리 › 수강생 명단]에서 하세요. 여기 금액은 저절로 따라
        바뀌지 않고, 달라지면 확인을 구합니다.
      </div>
    </Modal>
  )
}

/**
 * 변경내역 확인.
 *
 * **자동으로 반영하지 않는다.** 무엇이 어떻게 달라졌는지 보여 주고, 기록마다
 * 사람이 [기존 금액 유지] 또는 [현재 금액 반영]을 고른다.
 */
function DiffModal({
  workspaceId,
  onClose,
  onDone,
}: {
  workspaceId: number
  onClose: () => void
  onDone: () => void
}) {
  const toast = useToast()
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['adjustment-diffs', workspaceId],
    queryFn: () => api.adjustmentDiffs(workspaceId),
  })

  const act = useMutation({
    mutationFn: ({ ids, mode }: { ids: number[]; mode: 'KEEP' | 'APPLY' }) =>
      api.adjustmentConfirm(ids, mode),
    onSuccess: (n, v) => {
      toast.ok(
        v.mode === 'KEEP'
          ? `${n}건을 기존 금액 그대로 두었습니다.`
          : `${n}건을 현재 금액으로 바꾸었습니다.`,
      )
      void qc.invalidateQueries()
      onDone()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const rows = list.data ?? []
  // 기록 단위로 묶는다 — 고르는 것은 칸이 아니라 기록이다
  const groups = useMemo(() => {
    const map = new Map<number, AdjustmentDiff[]>()
    for (const d of rows) {
      const cur = map.get(d.adjustmentId) ?? []
      cur.push(d)
      map.set(d.adjustmentId, cur)
    }
    return [...map.entries()]
  }, [rows])

  return (
    <Modal
      wide
      title="변경내역 확인"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>닫기</Button>
          <Button
            disabled={groups.length === 0 || act.isPending}
            onClick={() => act.mutate({ ids: groups.map(([id]) => id), mode: 'KEEP' })}
          >
            모두 기존 금액 유지
          </Button>
        </>
      }
    >
      {groups.length === 0 ? (
        <Empty title="확인할 것이 없습니다">모든 기록이 지금 금액과 맞습니다.</Empty>
      ) : (
        <>
          <Notice tone="info">
            수강생 명단에서 금액이 바뀌었습니다. <b>프로그램이 저절로 바꾸지 않습니다</b> —
            이미 그 금액으로 안내하셨다면 [기존 금액 유지]를, 지금 금액이 맞다면 [현재 금액
            반영]을 고르세요.
          </Notice>

          {groups.map(([id, cells]) => {
            const head = cells[0]
            const 환불 = head.kind === 'REFUND'
            const 음수 = cells.some((c) => c.negative)
            return (
              <div
                key={id}
                style={{
                  marginTop: 12,
                  padding: 12,
                  border: '1px solid var(--gray-200)',
                  borderRadius: 8,
                }}
              >
                <div style={{ fontWeight: 700 }}>
                  {head.studentLabel} / {head.deptLabel}{' '}
                  <span className={`tag tag--${환불 ? 'plain' : 'free'}`} style={{ marginLeft: 6 }}>
                    {환불 ? '환불' : '추가징수'}
                  </span>
                  <span className="hint" style={{ marginLeft: 6 }}>
                    {head.occurredOn}
                  </span>
                </div>

                <div className="tableWrap" style={{ marginTop: 8 }}>
                  <table className="table">
                    <thead>
                      <tr>
                        <th style={{ width: 90 }}>항목</th>
                        {환불 && <th className="num">취소 직전</th>}
                        <th className="num">등록 당시 원본</th>
                        <th className="num">현재 원본</th>
                        <th className="num">등록 금액</th>
                        <th className="num">현재 기준</th>
                      </tr>
                    </thead>
                    <tbody>
                      {cells.map((c) => (
                        <tr key={c.itemCode}>
                          <td>{c.itemName}</td>
                          {환불 && <td className="num">{won(c.baseAmount)}</td>}
                          <td className="num">{won(c.checkedCharge)}</td>
                          <td className="num">{won(c.currentCharge)}</td>
                          <td className="num">{won(c.saved)}</td>
                          <td className="num">
                            {c.negative ? (
                              <b style={{ color: 'var(--red-600)' }}>음수</b>
                            ) : (
                              <b
                                style={{
                                  color:
                                    c.suggested === c.saved
                                      ? undefined
                                      : c.suggested > c.saved
                                        ? 'var(--red-600)'
                                        : 'var(--teal-600)',
                                }}
                              >
                                {won(c.suggested)}
                              </b>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>

                {음수 && (
                  <div style={{ marginTop: 8 }}>
                    <Notice tone="bad">
                      지금 금액으로는 환불액이 음수가 됩니다. 수강생 명단에서 금액을 먼저
                      확인해 주세요. [기존 금액 유지]는 할 수 있습니다.
                    </Notice>
                  </div>
                )}

                <div className="toolbar" style={{ marginTop: 8 }}>
                  <Button small disabled={act.isPending} onClick={() => act.mutate({ ids: [id], mode: 'KEEP' })}>
                    기존 금액 유지
                  </Button>
                  <Button
                    small
                    variant="primary"
                    disabled={act.isPending || 음수}
                    onClick={() => act.mutate({ ids: [id], mode: 'APPLY' })}
                  >
                    현재 금액으로 반영
                  </Button>
                </div>
              </div>
            )
          })}
        </>
      )}
    </Modal>
  )
}
