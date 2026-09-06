/**
 * 지원대상자 — 방과후 이용권 / 자유수강권 명단.
 *
 * 명단은 학년도 소속이고, 학생마다 **유효기간**을 가진다. 전입·전출·신규 선정·
 * 지원 중지가 학년도 중에 일어나기 때문이다 (설계안 0-2).
 * 날짜를 비워 두면 학년도 내내 유효하다.
 *
 * 대상학년과 어긋나는 학생이 있어도 **자동으로 지우거나 고치지 않는다.**
 * 표시만 하고 판단은 사람이 한다 (설계안 0-3).
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { ExcelTools } from '@/components/ExcelTools'
import { Confirm, Modal } from '@/components/Modal'
import { StudentPicker } from '@/components/StudentPicker'
import { useToast } from '@/components/Toast'
import { Button, Card, Field, Input, Notice, Search } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Eligibility, EligibilityInput, ProgramCode } from '@/ipc/types'
import { useApp } from '@/lib/useApp'

const TABS: { code: ProgramCode; label: string }[] = [
  { code: 'VOUCHER', label: '방과후 이용권' },
  { code: 'FREE_VOUCHER', label: '자유수강권' },
]

export function EligibilityPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()

  const [program, setProgram] = useState<ProgramCode>('VOUCHER')
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<number[]>([])
  const [editing, setEditing] = useState<Eligibility | 'new' | null>(null)
  const [confirm, setConfirm] = useState<'some' | 'all' | null>(null)
  const [onlyMismatch, setOnlyMismatch] = useState(false)

  const view = useQuery({
    queryKey: ['eligibility', app.yearId, program, query],
    queryFn: () => api.eligibilityList(app.yearId, program, query.trim() || undefined),
  })

  const rows = (view.data?.rows ?? []).filter((r) => !onlyMismatch || r.gradeMismatch)

  const remove = useMutation({
    mutationFn: (ids: number[]) => api.eligibilityDelete(ids),
    onSuccess: (n) => {
      toast.ok(`${n}건을 지웠습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const removeAll = useMutation({
    mutationFn: () => api.eligibilityDeleteAll(app.yearId, program),
    onSuccess: (r) => {
      toast.ok(`${r.deleted}건을 지웠습니다. 삭제 전 자료는 ${r.backup} 에 있습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const columns: Column<Eligibility>[] = [
    { key: 'grade', head: '학년', width: 60, sort: cmp.num((r) => r.grade), render: (r) => r.grade },
    { key: 'classNo', head: '반', width: 60, sort: cmp.num((r) => r.classNo), render: (r) => r.classNo },
    { key: 'studentNo', head: '번호', width: 60, sort: cmp.num((r) => r.studentNo), render: (r) => r.studentNo },
    { key: 'name', head: '이름', width: 110, sort: cmp.text((r) => r.name), render: (r) => r.name },
    {
      key: 'range',
      head: '적용기간',
      width: 210,
      render: (r) =>
        r.validFrom || r.validTo ? (
          `${r.validFrom ?? '학년도 시작'} ~ ${r.validTo ?? '학년도 끝'}`
        ) : (
          <span className="muted">학년도 내내</span>
        ),
    },
    {
      key: 'check',
      head: '확인',
      width: 130,
      render: (r) =>
        r.gradeMismatch ? (
          <span className="tag tag--warn">대상학년 아님</span>
        ) : (
          <span className="muted">—</span>
        ),
    },
    { key: 'source', head: '입력', width: 70, render: (r) => (r.source === 'EXCEL' ? 'Excel' : '수기') },
    { key: 'note', head: '비고', align: 'left', render: (r) => r.note || <span className="muted">—</span> },
  ]

  const one = selected.length === 1 ? rows.find((r) => r.id === selected[0]) : undefined
  const mismatch = view.data?.mismatchCount ?? 0

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">지원대상자</h1>
          <p className="page__desc">
            명단을 넣으면 학생정보의 지원유형이 곧바로 따라 바뀝니다. 지원유형을 따로 입력하지 않습니다.
          </p>
        </div>
        <div className="page__actions">
          <ExcelTools
            kind="eligibility"
            yearId={app.yearId}
            program={program}
            onDone={() => void qc.invalidateQueries()}
          />
        </div>
      </div>

      <div style={{ display: 'flex', gap: 6, marginBottom: 12 }}>
        {TABS.map((t) => (
          <Button
            key={t.code}
            variant={program === t.code ? 'primary' : 'default'}
            onClick={() => {
              setProgram(t.code)
              setSelected([])
              setOnlyMismatch(false)
            }}
          >
            {t.label}
          </Button>
        ))}
      </div>

      {mismatch > 0 && (
        <Notice
          tone="warn"
          actions={
            <Button small onClick={() => setOnlyMismatch((v) => !v)}>
              {onlyMismatch ? '전체 보기' : '목록 보기'}
            </Button>
          }
        >
          방과후 이용권 대상학년({view.data?.targetGradeText})이 아닌 학생 <b>{mismatch}명</b>이
          대상자 명단에 포함되어 있습니다. 자료를 임의로 고치지 않았으니 확인 후 직접 수정해 주세요.
        </Notice>
      )}

      {view.data && mismatch === 0 && view.data.targetGradeText === '전 학년' && program === 'VOUCHER' && (
        <Notice tone="info">
          아직 대상학년을 정하지 않았습니다. [시스템 › 학년도 지원금 설정]에서 정하면
          대상학년이 아닌 학생을 여기서 알려 줍니다. (Phase 3)
        </Notice>
      )}

      <Card
        flush
        title={`${TABS.find((t) => t.code === program)?.label} 명단`}
        actions={
          <>
            <Button variant="primary" small onClick={() => setEditing('new')}>
              수기 추가
            </Button>
            <Button small disabled={!one} onClick={() => one && setEditing(one)}>
              수정
            </Button>
            <Button small variant="danger" disabled={selected.length === 0} onClick={() => setConfirm('some')}>
              선택 삭제
            </Button>
            <Button small variant="danger" onClick={() => setConfirm('all')}>
              전체 삭제
            </Button>
          </>
        }
      >
        <div style={{ padding: 12 }}>
          <div className="toolbar">
            <Field label="검색">
              <Search value={query} onValue={setQuery} />
            </Field>
            <Button
              onClick={() => {
                setQuery('')
                setOnlyMismatch(false)
              }}
            >
              초기화
            </Button>
            <span className="toolbar__spacer" />
            <span className="hint">
              적용기간을 비우면 학년도 내내 유효합니다. 중도에 바뀐 학생만 날짜를 넣으세요.
            </span>
          </div>
        </div>

        <DataTable
          rows={rows}
          columns={columns}
          getId={(r) => r.id}
          selected={selected}
          onSelected={setSelected}
          onRowClick={(r) => setSelected([r.id])}
          empty={view.isLoading ? '불러오는 중…' : '대상자가 없습니다.'}
        />
      </Card>

      {editing && (
        <EligibilityModal
          program={program}
          value={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null)
            void qc.invalidateQueries()
          }}
        />
      )}

      {confirm === 'some' && (
        <Confirm
          danger
          title="선택한 대상자 삭제"
          message={
            <>
              선택한 <b>{selected.length}건</b>을 명단에서 지웁니다. 학생정보는 지워지지 않습니다.
            </>
          }
          busy={remove.isPending}
          onConfirm={() => remove.mutate(selected)}
          onClose={() => setConfirm(null)}
        />
      )}

      {confirm === 'all' && (
        <Confirm
          danger
          title="명단 전체 삭제"
          confirmText="모두 삭제"
          message={
            <>
              이 작업은 <b>{app.year?.name}</b>의{' '}
              <b>{TABS.find((t) => t.code === program)?.label}</b> 대상자{' '}
              <b>{(view.data?.rows ?? []).length}명</b>을 명단에서 모두 삭제합니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                학생정보 자체는 지워지지 않습니다. 다만 이 명단이 비면 그 학생들은 정산에서
                지원 대상이 아니게 됩니다.
              </div>
              <div style={{ marginTop: 10 }}>
                삭제 직전에 <b>자동으로 백업</b>됩니다.
              </div>
            </>
          }
          busy={removeAll.isPending}
          onConfirm={() => removeAll.mutate()}
          onClose={() => setConfirm(null)}
        />
      )}
    </div>
  )
}

function EligibilityModal({
  program,
  value,
  onClose,
  onSaved,
}: {
  program: ProgramCode
  value: Eligibility | null
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [studentId, setStudentId] = useState<number | null>(value?.studentId ?? null)
  const [from, setFrom] = useState(value?.validFrom ?? '')
  const [to, setTo] = useState(value?.validTo ?? '')
  const [note, setNote] = useState(value?.note ?? '')

  const save = useMutation({
    mutationFn: async () => {
      if (!studentId) throw { code: 'INVALID', message: '학생을 골라 주세요.' }
      const input: EligibilityInput = {
        studentId,
        program,
        validFrom: from || null,
        validTo: to || null,
        note,
      }
      if (value) await api.eligibilityUpdate(value.id, input)
      else await api.eligibilityCreate(app.yearId, input)
    },
    onSuccess: () => {
      toast.ok('저장되었습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      wide
      title={value ? '대상자 수정' : '대상자 추가'}
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
      {value ? (
        <div className="notice notice--info">
          <span aria-hidden>ℹ</span>
          <div>
            {value.grade}학년 {value.classNo}반 {value.studentNo}번 <b>{value.name}</b>
          </div>
        </div>
      ) : (
        <StudentPicker value={studentId} onChange={(id) => setStudentId(id)} />
      )}

      <div style={{ height: 14 }} />

      <div className="grid3">
        <Field label="적용 시작일" hint="비우면 학년도 시작부터">
          <Input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label="적용 종료일" hint="비우면 학년도 끝까지">
          <Input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </Field>
        <Field label="비고">
          <Input
            value={note}
            placeholder="전입 · 지원 중지 등"
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
      </div>
    </Modal>
  )
}
