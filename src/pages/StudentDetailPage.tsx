/**
 * 학생 상세정보 — **독립적인 조회 화면** (요구사항 §23).
 *
 * 수강생 명단에서 학생을 눌렀다고 이 화면이 저절로 채워지지 않는다.
 * 여기서 직접 조회한다. 학년·반·번호·이름 가운데 **하나만 넣어도** 찾는다.
 *
 * 지원금 사용액·잔액은 보여 주지 않는다 — 정산을 아직 실행하지 않았기 때문이다.
 * 없는 숫자를 지어내면 사람이 그것을 믿게 된다. Phase 3에서 연결한다.
 */

import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { Button, Card, Empty, Field, Input, Notice, Select } from '@/components/ui'
import { api } from '@/ipc/api'
import type { Student } from '@/ipc/types'
import { supportLabel, won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function StudentDetailPage() {
  const app = useApp()
  const items = app.boot.costItems

  const [grade, setGrade] = useState('')
  const [classNo, setClassNo] = useState('')
  const [studentNo, setStudentNo] = useState('')
  const [name, setName] = useState('')
  const [picked, setPicked] = useState<number | null>(null)

  const all = useQuery({
    queryKey: ['students-all', app.yearId],
    queryFn: () => api.studentList(app.yearId, {}),
  })
  const rows = all.data ?? []

  const grades = useMemo(
    () => [...new Set(rows.map((s) => s.grade))].sort((a, b) => a - b),
    [rows],
  )
  const classes = useMemo(() => {
    const list = rows.filter((s) => !grade || s.grade === Number(grade))
    return [...new Set(list.map((s) => s.classNo))].sort((a, b) => a - b)
  }, [rows, grade])

  const hasCondition =
    grade !== '' || classNo !== '' || studentNo.trim() !== '' || name.trim() !== ''

  // 조건 가운데 **하나 이상**만 맞으면 찾는다.
  const found: Student[] = useMemo(() => {
    if (!hasCondition) return []
    return rows.filter((s) => {
      if (grade && s.grade !== Number(grade)) return false
      if (classNo && s.classNo !== Number(classNo)) return false
      if (studentNo.trim() && s.studentNo !== Number(studentNo.replace(/[^0-9]/g, ''))) return false
      if (name.trim() && !s.name.includes(name.trim())) return false
      return true
    })
  }, [rows, grade, classNo, studentNo, name, hasCondition])

  const detail = useQuery({
    queryKey: ['student-detail', app.yearId, picked],
    queryFn: () => api.studentDetail(app.yearId, picked!),
    enabled: picked !== null,
  })

  function reset() {
    setGrade('')
    setClassNo('')
    setStudentNo('')
    setName('')
    setPicked(null)
  }

  const d = detail.data

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">학생 상세정보</h1>
          <p className="page__desc">
            학년 · 반 · 번호 · 이름 가운데 하나만 넣어도 찾습니다. 다른 화면과 연동되지 않는
            독립 조회 화면입니다.
          </p>
        </div>
      </div>

      <Card title="조회 조건">
        <div className="toolbar">
          <Field label="학년">
            <Select
              value={grade}
              onChange={(e) => {
                setGrade(e.target.value)
                setClassNo('')
                setPicked(null)
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
            <Select
              value={classNo}
              onChange={(e) => {
                setClassNo(e.target.value)
                setPicked(null)
              }}
              style={{ width: 96 }}
            >
              <option value="">전체</option>
              {classes.map((c) => (
                <option key={c} value={c}>
                  {c}반
                </option>
              ))}
            </Select>
          </Field>
          <Field label="번호">
            <Input
              className="input--num"
              value={studentNo}
              placeholder="예: 5"
              style={{ width: 84 }}
              onChange={(e) => {
                setStudentNo(e.target.value)
                setPicked(null)
              }}
            />
          </Field>
          <Field label="이름">
            <Input
              value={name}
              placeholder="이름 일부"
              style={{ width: 140 }}
              onChange={(e) => {
                setName(e.target.value)
                setPicked(null)
              }}
            />
          </Field>
          <Button onClick={reset}>초기화</Button>
        </div>

        {hasCondition && (
          <div style={{ marginTop: 12 }}>
            {found.length === 0 ? (
              <div className="hint">조건에 맞는 학생이 없습니다.</div>
            ) : (
              <>
                <div className="hint" style={{ marginBottom: 6 }}>
                  {found.length}명 — 이름을 누르면 상세정보가 열립니다.
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, maxHeight: 132, overflow: 'auto' }}>
                  {found.slice(0, 200).map((s) => (
                    <Button
                      key={s.id}
                      small
                      variant={picked === s.id ? 'primary' : 'default'}
                      onClick={() => setPicked(s.id)}
                    >
                      {s.grade}-{s.classNo}-{s.studentNo} {s.name}
                    </Button>
                  ))}
                </div>
              </>
            )}
          </div>
        )}
      </Card>

      {picked === null && (
        <div className="card">
          <div className="card__body">
            <Empty title="학생을 골라 주세요">
              위에서 조건을 넣고 이름을 누르면 기본정보 · 지원유형 · 수강내역이 나옵니다.
            </Empty>
          </div>
        </div>
      )}

      {detail.isLoading && <div className="hint">불러오는 중…</div>}

      {d && (
        <>
          <Card title="기본정보">
            <div className="stat">
              <div className="stat__item">
                <div className="stat__label">학년 · 반 · 번호</div>
                <div className="stat__value" style={{ fontSize: 18 }}>
                  {d.student.grade}-{d.student.classNo}-{d.student.studentNo}
                </div>
              </div>
              <div className="stat__item">
                <div className="stat__label">이름</div>
                <div className="stat__value" style={{ fontSize: 18 }}>
                  {d.student.name}
                </div>
              </div>
              <div className="stat__item">
                <div className="stat__label">지원유형 (학년도 기준)</div>
                <div style={{ marginTop: 6 }}>
                  <span className={`tag tag--${supportLabel(d.student.programs).tone}`}>
                    {supportLabel(d.student.programs).text}
                  </span>
                </div>
              </div>
              <div className="stat__item">
                <div className="stat__label">비고</div>
                <div style={{ marginTop: 6 }}>
                  {d.student.note || <span className="muted">—</span>}
                </div>
              </div>
            </div>
          </Card>

          <Card title="지원제도">
            <div className="tableWrap" style={{ border: '1px solid var(--gray-200)' }}>
              <table className="table">
                <thead>
                  <tr>
                    <th style={{ width: 160 }}>제도</th>
                    <th style={{ width: 110 }}>자격</th>
                    <th className="left">적용기간</th>
                    <th style={{ width: 220 }}>지원금 사용액 · 잔액</th>
                  </tr>
                </thead>
                <tbody>
                  {d.supports.map((s) => (
                    <tr key={s.program}>
                      <td>{s.programLabel}</td>
                      <td>
                        {s.eligible ? (
                          <span className="tag tag--voucher">대상</span>
                        ) : (
                          <span className="tag tag--plain">해당없음</span>
                        )}
                      </td>
                      <td className="left">
                        {s.eligible ? (
                          <>
                            {s.periods.join(' · ')}
                            {s.gradeMismatch && (
                              <span className="tag tag--warn" style={{ marginLeft: 6 }}>
                                대상학년 아님
                              </span>
                            )}
                          </>
                        ) : (
                          <span className="muted">해당없음</span>
                        )}
                      </td>
                      <td className="muted">정산 후 표시 (Phase 3)</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="hint" style={{ marginTop: 8 }}>
              지원금 사용액과 잔액은 정산을 실행해야 나옵니다. 아직 계산하지 않은 값을 임의로
              보여 주지 않습니다.
            </div>
          </Card>

          {d.workspaces.length === 0 ? (
            <Card title="수강내역">
              <Empty title="수강 자료가 없습니다">
                이 학생은 아직 어느 작업공간에서도 수강하지 않았습니다.
              </Empty>
            </Card>
          ) : (
            d.workspaces.map((w) => (
              <Card
                key={w.workspaceId}
                flush
                title={`${w.workspaceName} (${w.startDate} ~ ${w.endDate})`}
                actions={
                  <span className="hint">
                    수강중 합계 <b style={{ color: 'var(--navy-800)' }}>{won(w.activeTotal)}</b>원
                  </span>
                }
              >
                <div className="tableWrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th className="left" style={{ width: 160 }}>
                          부서
                        </th>
                        {items.map((it) => (
                          <th key={it.code} className="num" style={{ width: 92 }}>
                            {it.name}
                          </th>
                        ))}
                        <th className="num" style={{ width: 100 }}>
                          합계
                        </th>
                        <th style={{ width: 76 }}>상태</th>
                        <th className="left">변경사유</th>
                      </tr>
                    </thead>
                    <tbody>
                      {w.rows.map((e) => (
                        <tr key={e.id} style={{ opacity: e.status === 'ACTIVE' ? 1 : 0.55 }}>
                          <td className="left">{e.deptLabel}</td>
                          {items.map((it) => (
                            <td key={it.code} className="num">
                              {won(e.fees.find((f) => f.itemCode === it.code)?.amount ?? 0)}
                            </td>
                          ))}
                          <td className="num">
                            <b>{won(e.total)}</b>
                          </td>
                          <td>
                            {e.status === 'ACTIVE' ? (
                              <span className="tag tag--free">수강중</span>
                            ) : (
                              <span className="tag tag--plain">취소</span>
                            )}
                          </td>
                          <td className="left">
                            {e.changeReason || <span className="muted">—</span>}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Card>
            ))
          )}

          <Notice tone="info">
            수강 금액을 고치거나 취소하려면 [수강생 명단]에서 해당 작업공간을 골라 처리해 주세요.
            이 화면은 조회 전용입니다.
          </Notice>
        </>
      )}
    </div>
  )
}
