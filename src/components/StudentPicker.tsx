/**
 * 학생 고르기 (요구사항 §6).
 *
 * 규칙
 *   · 학년·반을 먼저 고른다. 고르기 전에는 번호·이름 후보를 전부 펼치지 않는다.
 *   · 이름을 입력했다고 학년·반·번호를 임의로 채우지 않는다 — 후보를 좁혀 줄 뿐이다.
 */

import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import type { Student } from '@/ipc/types'
import { api } from '@/ipc/api'
import { useApp } from '@/lib/useApp'

import { Field, Input, Select } from './ui'

export function StudentPicker({
  value,
  onChange,
}: {
  value: number | null
  onChange: (studentId: number | null, student: Student | null) => void
}) {
  const app = useApp()
  const all = useQuery({
    queryKey: ['students-all', app.yearId],
    queryFn: () => api.studentList(app.yearId, {}),
  })
  const rows = all.data ?? []
  const picked = rows.find((s) => s.id === value) ?? null

  const [grade, setGrade] = useState<string>(picked ? String(picked.grade) : '')
  const [classNo, setClassNo] = useState<string>(picked ? String(picked.classNo) : '')
  const [name, setName] = useState('')

  const grades = useMemo(
    () => [...new Set(rows.map((s) => s.grade))].sort((a, b) => a - b),
    [rows],
  )
  const classes = useMemo(() => {
    if (!grade) return []
    return [...new Set(rows.filter((s) => s.grade === Number(grade)).map((s) => s.classNo))].sort(
      (a, b) => a - b,
    )
  }, [rows, grade])

  const candidates = useMemo(() => {
    if (!grade || !classNo) return []
    const key = name.trim()
    return rows
      .filter((s) => s.grade === Number(grade) && s.classNo === Number(classNo))
      .filter((s) => !key || s.name.includes(key))
      .sort((a, b) => a.studentNo - b.studentNo)
  }, [rows, grade, classNo, name])

  return (
    <div className="grid4">
      <Field label="학년">
        <Select
          value={grade}
          onChange={(e) => {
            setGrade(e.target.value)
            setClassNo('')
            onChange(null, null)
          }}
        >
          <option value="">선택</option>
          {grades.map((g) => (
            <option key={g} value={g}>
              {g}학년
            </option>
          ))}
        </Select>
      </Field>

      <Field label="반">
        <Select
          value={classNo}
          disabled={!grade}
          onChange={(e) => {
            setClassNo(e.target.value)
            onChange(null, null)
          }}
        >
          <option value="">{grade ? '선택' : '학년 먼저'}</option>
          {classes.map((c) => (
            <option key={c} value={c}>
              {c}반
            </option>
          ))}
        </Select>
      </Field>

      <Field label="이름으로 좁히기">
        <Input
          value={name}
          disabled={!grade || !classNo}
          placeholder={grade && classNo ? '이름 일부' : '학년·반 먼저'}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>

      <Field
        label="학생"
        hint={grade && classNo ? `${candidates.length}명` : '학년과 반을 먼저 고르세요.'}
      >
        <Select
          value={value ?? ''}
          disabled={!grade || !classNo}
          onChange={(e) => {
            const id = e.target.value ? Number(e.target.value) : null
            onChange(id, rows.find((s) => s.id === id) ?? null)
          }}
        >
          <option value="">선택</option>
          {candidates.map((s) => (
            <option key={s.id} value={s.id}>
              {s.studentNo}번 {s.name}
            </option>
          ))}
        </Select>
      </Field>
    </div>
  )
}
