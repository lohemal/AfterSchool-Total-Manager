/**
 * 수강생 명단 — 실제 업무에서 가장 오래 머무는 화면 (요구사항 §14).
 *
 * 이 표에 보이는 금액은 `charge` 하나뿐이다. `기본 합계 / 실제 합계`를 나누지
 * 않는다 — 사용자에게 필요한 것은 실제로 적용되는 금액이다.
 *
 * 취소한 수강도 목록에 남는다. 상태 열에서 확인하고 필터로 걸러 본다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { ApplyFeesModal } from '@/components/ApplyFeesModal'
import { CancelModal } from '@/components/CancelModal'
import { cmp, DataTable, type Column } from '@/components/DataTable'
import { EnrollmentModal } from '@/components/EnrollmentModal'
import { ExcelTools } from '@/components/ExcelTools'
import { Confirm, Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Input, Notice, Search, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Enrollment, EnrollmentFilter } from '@/ipc/types'
import { compareClassNo, supportLabel, won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function RosterPage() {
  const app = useApp()
  const qc = useQueryClient()
  const items = app.boot.costItems
  const wsId = app.workspaceId

  const [departmentId, setDepartmentId] = useState('')
  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [program, setProgram] = useState('')
  const [status, setStatus] = useState('ACTIVE')
  const [query, setQuery] = useState('')

  const [selected, setSelected] = useState<number[]>([])
  const [editing, setEditing] = useState<Enrollment | 'new' | null>(null)
  const [cancelling, setCancelling] = useState<Enrollment | null>(null)
  const [restoring, setRestoring] = useState<Enrollment | null>(null)
  const [applying, setApplying] = useState(false)
  const [deletingAll, setDeletingAll] = useState(false)

  const filter: EnrollmentFilter = useMemo(
    () => ({
      departmentId: departmentId ? Number(departmentId) : null,
      grade: grade ? Number(grade) : null,
      classNo: classNo || null,
      program: program || null,
      status: (status || null) as EnrollmentFilter['status'],
      query: query.trim() || null,
    }),
    [departmentId, grade, classNo, program, status, query],
  )

  const list = useQuery({
    queryKey: ['enrollments', wsId, filter],
    queryFn: () => api.enrollmentList(wsId!, filter),
    enabled: wsId !== null,
  })

  const departments = useQuery({
    queryKey: ['departments', wsId, ''],
    queryFn: () => api.departmentList(wsId!),
    enabled: wsId !== null,
  })

  // 필터 후보는 전체 명단에서 뽑는다.
  const all = useQuery({
    queryKey: ['enrollments-all', wsId],
    queryFn: () => api.enrollmentList(wsId!, {}),
    enabled: wsId !== null,
  })
  const grades = useMemo(
    () => [...new Set((all.data ?? []).map((e) => e.grade))].sort((a, b) => a - b),
    [all.data],
  )
  const classes = useMemo(() => {
    const rows = (all.data ?? []).filter((e) => !grade || e.grade === Number(grade))
    return [...new Set(rows.map((e) => e.classNo))].sort(compareClassNo)
  }, [all.data, grade])

  const one = selected.length === 1 ? list.data?.find((e) => e.id === selected[0]) : undefined

  const toast = useToast()
  const deleteAll = useMutation({
    mutationFn: () => api.enrollmentDeleteAll(wsId!),
    onSuccess: (r) => {
      setDeletingAll(false)
      setSelected([])
      toast.ok(`수강 자료 ${r.deleted}건을 지웠습니다. 삭제 전 자료는 ${r.backup} 에 있습니다.`)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">수강생 명단</h1>
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

  const rows = list.data ?? []
  const activeRows = rows.filter((e) => e.status === 'ACTIVE')
  const sumOf = (code: string) =>
    activeRows.reduce(
      (s, e) => s + (e.fees.find((f) => f.itemCode === code)?.amount ?? 0),
      0,
    )

  const columns: Column<Enrollment>[] = [
    {
      key: 'dept',
      head: '부서',
      width: 140,
      align: 'left',
      sort: cmp.text((e) => e.deptLabel),
      render: (e) => e.deptLabel,
    },
    { key: 'grade', head: '학년', width: 54, sort: cmp.num((e) => e.grade), render: (e) => e.grade },
    { key: 'classNo', head: '반', width: 54, sort: cmp.classNo((e) => e.classNo), render: (e) => e.classNo },
    { key: 'studentNo', head: '번호', width: 54, sort: cmp.num((e) => e.studentNo), render: (e) => e.studentNo },
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
      width: 96,
      sort: cmp.num((e) => e.total),
      render: (e) => (
        <b>
          {won(e.total)}
          {e.hasOverride && (
            <span className="tag tag--warn" style={{ marginLeft: 4 }}>
              수정
            </span>
          )}
        </b>
      ),
    },
    {
      key: 'status',
      head: '상태',
      width: 72,
      render: (e) =>
        e.status === 'ACTIVE' ? (
          <span className="tag tag--free">수강중</span>
        ) : (
          <span className="tag tag--plain">취소</span>
        ),
    },
    {
      key: 'reason',
      head: '변경사유',
      align: 'left',
      render: (e) => e.changeReason || <span className="muted">—</span>,
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">수강생 명단</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 수강 자료가 있는 학생만 나옵니다.
          </p>
        </div>
        <div className="page__actions">
          <ExcelTools
            kind="enrollments"
            yearId={app.yearId}
            workspaceId={wsId}
            enrollmentFilter={filter}
            onDone={() => void qc.invalidateQueries()}
          />
        </div>
      </div>

      <Notice tone="info">
        금액은 이 학생에게 실제로 적용되는 값입니다. 부서 기준금액을 고쳐도 여기 금액은
        저절로 바뀌지 않으며, [부서금액 반영]을 눌렀을 때만 바뀝니다.
      </Notice>

      <Card
        flush
        title="명단"
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
              variant="danger"
              disabled={!one || one.status === 'CANCELLED'}
              onClick={() => one && setCancelling(one)}
            >
              수강 취소
            </Button>
            <Button
              small
              disabled={!one || one.status === 'ACTIVE'}
              onClick={() => one && setRestoring(one)}
            >
              취소 되돌리기
            </Button>
            <Button small onClick={() => setApplying(true)}>
              부서금액 반영
            </Button>
            <Button small variant="danger" onClick={() => setDeletingAll(true)}>
              전체 삭제
            </Button>
          </>
        }
      >
        <div style={{ padding: 12 }}>
          <div className="toolbar">
            <Field label="부서">
              <Select
                value={departmentId}
                onChange={(e) => setDepartmentId(e.target.value)}
                style={{ width: 168 }}
              >
                <option value="">전체</option>
                {(departments.data ?? []).map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                    {d.className ? ` ${d.className}` : ''}
                  </option>
                ))}
              </Select>
            </Field>
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
            <Field label="지원유형">
              <Select value={program} onChange={(e) => setProgram(e.target.value)} style={{ width: 148 }}>
                <option value="">전체</option>
                <option value="NONE">일반</option>
                <option value="VOUCHER">방과후 이용권</option>
                <option value="FREE_VOUCHER">자유수강권</option>
                <option value="BOTH">이용권+자유수강권</option>
              </Select>
            </Field>
            <Field label="수강상태">
              <Select value={status} onChange={(e) => setStatus(e.target.value)} style={{ width: 108 }}>
                <option value="">전체</option>
                <option value="ACTIVE">수강중</option>
                <option value="CANCELLED">취소</option>
              </Select>
            </Field>
            <Field label="검색">
              <Search value={query} onValue={setQuery} width={180} />
            </Field>
            <Button
              onClick={() => {
                setDepartmentId('')
                setGrade('')
                setClassNo('')
                setProgram('')
                setStatus('ACTIVE')
                setQuery('')
              }}
            >
              초기화
            </Button>
          </div>
        </div>

        <div className="toolbar__note">
          줄을 두 번 누르면 <b>수정</b>창이 열립니다. 한 번 누르는 것은 선택입니다.
        </div>

        <DataTable
          rows={rows}
          columns={columns}
          getId={(e) => e.id}
          selected={selected}
          onSelected={setSelected}
          onRowClick={(e) => setSelected([e.id])}
          onRowDoubleClick={(e) => setEditing(e)}
          empty={
            list.isLoading
              ? '불러오는 중…'
              : (all.data?.length ?? 0) > 0
                ? '조건에 맞는 수강생이 없습니다.'
                : '수강 자료가 없습니다. [수기 추가]나 [Excel 업로드]로 등록해 주세요.'
          }
          foot={
            <>
              <span>
                모두 <b>{rows.length}</b>건 · 수강중 <b>{activeRows.length}</b>건
              </span>
              {selected.length > 0 && <span>· 선택 {selected.length}건</span>}
              <span className="toolbar__spacer" />
              {items.map((it) => (
                <span key={it.code}>
                  {it.name} <b>{won(sumOf(it.code))}</b>
                </span>
              ))}
              <span>
                합계 <b>{won(activeRows.reduce((s, e) => s + e.total, 0))}</b>
              </span>
            </>
          }
        />
      </Card>

      {editing && (
        <EnrollmentModal
          value={editing === 'new' ? null : editing}
          departments={departments.data ?? []}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {cancelling && (
        <CancelModal
          target={cancelling}
          items={items}
          onClose={() => setCancelling(null)}
          onDone={() => {
            setCancelling(null)
            setSelected([])
            void qc.invalidateQueries()
          }}
        />
      )}

      {restoring && (
        <RestoreModal
          target={restoring}
          onClose={() => setRestoring(null)}
          onDone={() => {
            setRestoring(null)
            setSelected([])
            void qc.invalidateQueries()
          }}
        />
      )}

      {deletingAll && (
        <Confirm
          danger
          title="수강 자료 전체 삭제"
          confirmText="모두 삭제"
          message={
            <>
              이 작업은 <b>{app.workspace?.name}</b> 작업공간의 수강 자료{' '}
              <b>{(all.data ?? []).length}건</b>을 모두 삭제합니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                취소 상태인 수강까지 함께 사라지고, 금액과 변경 대상 기록도 지워집니다.
                <br />학생정보와 부서정보는 그대로 남습니다.
              </div>
              <div style={{ marginTop: 10 }}>
                삭제 직전에 <b>자동으로 백업</b>됩니다. 다른 작업공간의 자료는 그대로입니다.
              </div>
            </>
          }
          busy={deleteAll.isPending}
          onConfirm={() => deleteAll.mutate()}
          onClose={() => setDeletingAll(false)}
        />
      )}

      {applying && (
        <ApplyFeesModal
          department={null}
          onClose={() => setApplying(false)}
          onApplied={() => {
            setApplying(false)
            void qc.invalidateQueries()
          }}
        />
      )}
    </div>
  )
}

/**
 * 취소 되돌리기.
 *
 * **금액을 저절로 되돌리지 않는다.** 취소할 때 담당자가 확정한 징수금액을
 * 그대로 둔다 — 프로그램이 임의로 바꾸면 그 판단이 조용히 사라진다.
 * 다시 받아야 한다면 사람이 [부서 기준금액으로 되돌리기]를 고른다.
 */
function RestoreModal({
  target,
  onClose,
  onDone,
}: {
  target: Enrollment
  onClose: () => void
  onDone: () => void
}) {
  const toast = useToast()
  const [reason, setReason] = useState('')
  const [reset, setReset] = useState(false)

  const act = useMutation({
    mutationFn: () => api.enrollmentRestore(target.id, reset, reason.trim()),
    onSuccess: () => {
      toast.ok('되돌렸습니다.')
      onDone()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      title="취소 되돌리기"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>닫기</Button>
          <Button variant="primary" onClick={() => act.mutate()} disabled={act.isPending}>
            {act.isPending ? '처리 중…' : '되돌리기'}
          </Button>
        </>
      }
    >
      <div style={{ lineHeight: 1.8, marginBottom: 12 }}>
        {target.grade}학년 {target.classNo}반 {target.studentNo}번 <b>{target.name}</b>
        <br />
        <b>{target.deptLabel}</b>
      </div>

      <Notice tone="info">
        현재 금액은 취소할 때 확정한 <b>{won(target.total)}원</b>입니다. 되돌려도 이 금액을
        그대로 둡니다.
      </Notice>

      <label
        style={{ display: 'flex', alignItems: 'center', gap: 8, margin: '12px 0', cursor: 'pointer' }}
      >
        <input type="checkbox" checked={reset} onChange={(e) => setReset(e.target.checked)} />
        <span>부서 기준금액으로 되돌리기</span>
      </label>
      <div className="hint" style={{ marginTop: -6, marginBottom: 4 }}>
        다시 처음부터 수강하는 경우에만 고르세요. 학생별로 고쳐 둔 금액이 사라집니다.
      </div>

      <Field label="변경사유" hint="이 수강을 다시 수강중으로 바꿉니다.">
        <Input
          autoFocus
          value={reason}
          placeholder="착오 · 취소 철회 등"
          onChange={(e) => setReason(e.target.value)}
        />
      </Field>
    </Modal>
  )
}
