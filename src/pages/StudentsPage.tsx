/**
 * 학생정보 — 전교생. 방과후 수강생만 넣는 것이 아니다 (요구사항 §5).
 *
 * 지원유형 칸은 저장된 값이 아니라 지원대상자 명단에서 만들어진다.
 * 그래서 명단을 고치면 이 화면이 곧바로 따라 바뀐다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { ExcelTools } from '@/components/ExcelTools'
import { Confirm, Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Field, Input, Notice, NumInput, Search, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Student, StudentFilter, StudentInput } from '@/ipc/types'
import { compareClassNo, supportLabel } from '@/lib/format'
import { useApp } from '@/lib/useApp'

const EMPTY_FILTER: StudentFilter = {}

export function StudentsPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()

  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [program, setProgram] = useState('')
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<number[]>([])
  const [editing, setEditing] = useState<Student | 'new' | null>(null)
  const [confirm, setConfirm] = useState<'some' | 'all' | null>(null)

  const filter: StudentFilter = useMemo(
    () => ({
      workspaceId: app.workspaceId,
      grade: grade ? Number(grade) : null,
      classNo: classNo || null,
      program: program || null,
      query: query.trim() || null,
    }),
    [app.workspaceId, grade, classNo, program, query],
  )

  const list = useQuery({
    queryKey: ['students', app.yearId, filter],
    queryFn: () => api.studentList(app.yearId, filter),
  })

  // 필터 후보는 전체 명단에서 뽑는다 — 지금 필터에 걸린 결과만 보면 후보가 사라진다.
  const all = useQuery({
    queryKey: ['students-all', app.yearId],
    queryFn: () => api.studentList(app.yearId, EMPTY_FILTER),
  })
  const grades = useMemo(
    () => [...new Set((all.data ?? []).map((s) => s.grade))].sort((a, b) => a - b),
    [all.data],
  )
  const classes = useMemo(() => {
    const rows = (all.data ?? []).filter((s) => !grade || s.grade === Number(grade))
    return [...new Set(rows.map((s) => s.classNo))].sort(compareClassNo)
  }, [all.data, grade])

  const remove = useMutation({
    mutationFn: (ids: number[]) => api.studentDelete(ids),
    onSuccess: (n) => {
      toast.ok(`${n}명을 지웠습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const removeAll = useMutation({
    mutationFn: () => api.studentDeleteAll(app.yearId),
    onSuccess: (r) => {
      // 삭제 직전 자동백업 이름을 함께 알려 준다 — 실수해도 되돌릴 수 있다
      toast.ok(`학생정보 ${r.deleted}건을 지웠습니다. 삭제 전 자료는 ${r.backup} 에 있습니다.`)
      setSelected([])
      setConfirm(null)
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const columns: Column<Student>[] = [
    { key: 'grade', head: '학년', width: 60, sort: cmp.num((s) => s.grade), render: (s) => s.grade },
    { key: 'classNo', head: '반', width: 60, sort: cmp.classNo((s) => s.classNo), render: (s) => s.classNo },
    { key: 'studentNo', head: '번호', width: 60, sort: cmp.num((s) => s.studentNo), render: (s) => s.studentNo },
    { key: 'name', head: '이름', width: 110, sort: cmp.text((s) => s.name), render: (s) => s.name },
    {
      key: 'programs',
      head: '지원유형',
      width: 150,
      sort: cmp.text((s) => supportLabel(s.programs).text),
      render: (s) => {
        const v = supportLabel(s.programs)
        return <span className={`tag tag--${v.tone}`}>{v.text}</span>
      },
    },
    { key: 'note', head: '비고', align: 'left', render: (s) => s.note || <span className="muted">—</span> },
  ]

  const one = selected.length === 1 ? list.data?.find((s) => s.id === selected[0]) : undefined

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">학생정보</h1>
          <p className="page__desc">
            전교생을 넣습니다. 학년도 단위로 한 번만 올리면 모든 작업공간이 함께 씁니다.
          </p>
        </div>
        <div className="page__actions">
          <ExcelTools
            kind="students"
            yearId={app.yearId}
            filter={filter}
            onDone={() => void qc.invalidateQueries()}
          />
        </div>
      </div>

      {app.workspaceId && (
        <Notice tone="info">
          지원유형은 <b>{app.workspace?.name}</b> 기간({app.workspace?.startDate} ~{' '}
          {app.workspace?.endDate})에 유효한 자격을 기준으로 표시합니다.
        </Notice>
      )}

      <Card
        flush
        title="명단"
        actions={
          <>
            <Button variant="primary" small onClick={() => setEditing('new')}>
              수기 추가
            </Button>
            <Button small disabled={!one} onClick={() => one && setEditing(one)}>
              수정
            </Button>
            <Button
              small
              variant="danger"
              disabled={selected.length === 0}
              onClick={() => setConfirm('some')}
            >
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
            <Field label="지원유형">
              <Select value={program} onChange={(e) => setProgram(e.target.value)} style={{ width: 150 }}>
                <option value="">전체</option>
                <option value="NONE">일반</option>
                <option value="VOUCHER">방과후 이용권</option>
                <option value="FREE_VOUCHER">자유수강권</option>
                <option value="BOTH">이용권+자유수강권</option>
              </Select>
            </Field>
            <Field label="검색">
              <Search value={query} onValue={setQuery} />
            </Field>
            <Button
              onClick={() => {
                setGrade('')
                setClassNo('')
                setProgram('')
                setQuery('')
              }}
            >
              초기화
            </Button>
          </div>
        </div>

        <DataTable
          rows={list.data ?? []}
          columns={columns}
          getId={(s) => s.id}
          selected={selected}
          onSelected={setSelected}
          onRowClick={(s) => setSelected([s.id])}
          empty={
            list.isLoading
              ? '불러오는 중…'
              : all.data?.length
                ? '조건에 맞는 학생이 없습니다.'
                : '학생정보가 없습니다. [업로드 양식 받기]로 양식을 받아 채운 뒤 올려 주세요.'
          }
        />
      </Card>

      {editing && (
        <StudentModal
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
          title="선택한 학생 삭제"
          message={
            <>
              선택한 <b>{selected.length}명</b>을 지웁니다. 그 학생의 수강·지원자격 자료도 함께
              사라집니다. 계속할까요?
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
          title="학생정보 전체 삭제"
          confirmText="모두 삭제"
          message={
            <>
              이 작업은 <b>{app.year?.name}</b>의 학생정보 <b>{(all.data ?? []).length}명</b>을
              모두 삭제합니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                함께 사라지는 것:
                <br />· 그 학생들의 지원자격(이용권·자유수강권) 명단
                <br />· 모든 작업공간의 수강 자료와 금액
              </div>
              <div style={{ marginTop: 10 }}>
                삭제 직전에 <b>자동으로 백업</b>됩니다. 실수했더라도 [백업·복원·업데이트]에서
                되돌릴 수 있습니다.
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

function StudentModal({
  value,
  onClose,
  onSaved,
}: {
  value: Student | null
  onClose: () => void
  onSaved: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [form, setForm] = useState<StudentInput>({
    grade: value?.grade ?? 1,
    classNo: value?.classNo ?? '',
    studentNo: value?.studentNo ?? 1,
    name: value?.name ?? '',
    note: value?.note ?? '',
  })

  const save = useMutation({
    mutationFn: async () => {
      if (value) await api.studentUpdate(value.id, form)
      else await api.studentCreate(app.yearId, form)
    },
    onSuccess: () => {
      toast.ok('저장되었습니다.')
      onSaved()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const num = (v: string) => (v === '' ? 0 : Number(v.replace(/[^0-9]/g, '')))

  return (
    <Modal
      title={value ? '학생 수정' : '학생 추가'}
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
      <div className="grid3">
        <Field label="학년">
          <NumInput
            value={form.grade || ''}
            onChange={(e) => setForm({ ...form, grade: num(e.target.value) })}
          />
        </Field>
        {/* 반은 학교마다 다르다. '1'도 '가'도 '해'도 그대로 쓴다. */}
        <Field label="반" hint="숫자도 글자도 됩니다 (1, 가, 해)">
          <Input
            value={form.classNo}
            maxLength={10}
            onChange={(e) => setForm({ ...form, classNo: e.target.value })}
          />
        </Field>
        <Field label="번호">
          <NumInput
            value={form.studentNo || ''}
            onChange={(e) => setForm({ ...form, studentNo: num(e.target.value) })}
          />
        </Field>
      </div>
      <div style={{ height: 12 }} />
      <div className="grid2">
        <Field label="이름">
          <Input
            value={form.name}
            autoFocus
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
        </Field>
        <Field label="비고">
          <Input
            value={form.note ?? ''}
            placeholder="전학 · 특이사항"
            onChange={(e) => setForm({ ...form, note: e.target.value })}
          />
        </Field>
      </div>
      <div className="hint" style={{ marginTop: 12 }}>
        지원유형은 여기서 정하지 않습니다. [지원대상자] 화면의 명단에서 정해집니다.
      </div>
    </Modal>
  )
}
