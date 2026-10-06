/**
 * 부서별 수강현황 (v0.1.7, 설계안 28장).
 *
 * 자료를 고치는 화면이 아니라 **지금 운영이 어떻게 돌아가는지 보는 화면**이다.
 * 정원과 수강 가능 학년은 [기초 데이터 > 부서정보]에서 정하고, 여기서는 그것과
 * 실제 수강을 맞대어 본다.
 *
 * ## 셈은 전부 서버가 한다
 *
 * 남은 자리 · 충원율 · 참여율 · 상태는 `repo::capacity` 가 낸 값을 그대로
 * 쓴다. 화면에서 다시 셈하면 Excel 과 숫자가 어긋나는 날이 온다.
 *
 * ## 숫자를 색으로만 가르지 않는다
 *
 * 정원 초과는 눈에 띄어야 하지만, 색을 못 보는 사람과 흑백으로 인쇄한 사람도
 * 같은 것을 읽어야 하므로 상태는 늘 **글로** 함께 적는다.
 */

import { useMutation, useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Notice, Search, Select } from '@/components/ui'
import { api } from '@/ipc/api'
import type { DeptCapacityRow, SeatRow } from '@/ipc/types'
import {
  FINDING_TEXT,
  FINDING_TONE,
  people,
  percent,
  STATUS_ORDER,
  STATUS_TEXT,
  STATUS_TONE,
} from '@/lib/capacity'
import { gradeText, GRADES } from '@/lib/grades'
import { applySort, type SortSpec } from '@/lib/sortSpec'
import { saveExport } from '@/lib/saveFile'
import { useApp } from '@/lib/useApp'

type Tab = 'DEPT' | 'GRADE' | 'WEEKDAY' | 'SEATS'

const TABS: [Tab, string][] = [
  ['DEPT', '부서별 현황'],
  ['GRADE', '학년별 현황'],
  ['WEEKDAY', '요일별 현황'],
  ['SEATS', '수강 가능 부서 찾기'],
]

function Stat({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="statCard">
      <div className="statCard__label">{label}</div>
      <div className="statCard__value">{value}</div>
      {sub && <div className="statCard__sub">{sub}</div>}
    </div>
  )
}

export function CapacityPage() {
  const app = useApp()
  const toast = useToast()
  const wsId = app.workspaceId

  const [tab, setTab] = useState<Tab>('DEPT')

  const stats = useQuery({
    queryKey: ['capacity-stats', wsId],
    queryFn: () => api.capacityStats(wsId!),
    enabled: wsId !== null,
  })

  const download = useMutation({
    mutationFn: () => saveExport(() => api.capacityExport(wsId!), toast, (r) => `반 ${r.rows}개`),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1>부서별 수강현황</h1>
        </div>
        <Card>
          <Empty title="작업공간이 없습니다">
            수강현황은 작업공간마다 다릅니다. [작업공간]에서 먼저 하나 만들어 주세요.
          </Empty>
        </Card>
      </div>
    )
  }

  const s = stats.data

  return (
    <div className="page">
      <div className="page__head">
        <h1>부서별 수강현황</h1>
        <span className="toolbar__spacer" />
        <Button
          variant="primary"
          onClick={() => download.mutate()}
          disabled={!s || download.isPending}
          title="화면 필터와 무관하게 이 작업공간 전체를 냅니다"
        >
          {download.isPending ? '만드는 중…' : 'Excel 받기'}
        </Button>
      </div>

      {s && (
        <div className="statRow">
          <Stat label="운영 반" value={`${s.summary.classes}개`} />
          <Stat
            label="총 정원"
            value={`${people(s.summary.totalCapacity)}명`}
            sub={
              s.summary.classesWithoutCapacity > 0
                ? `정원 설정 ${s.summary.classesWithCapacity}개 · 미설정 ${s.summary.classesWithoutCapacity}개 제외`
                : `${s.summary.classesWithCapacity}개 반`
            }
          />
          <Stat label="수강 건수" value={`${people(s.summary.totalEnrollments)}건`} />
          <Stat
            label="수강 학생 수"
            value={`${people(s.summary.totalStudents)}명`}
            sub="한 학생이 여럿 들어도 1명"
          />
          <Stat
            label="남은 자리"
            value={`${people(s.summary.openSeats)}명`}
            sub="더 받을 수 있는 자리"
          />
          <Stat
            label="평균 충원율"
            value={percent(s.summary.avgFillRate)}
            sub="정원을 정한 반만"
          />
          <Stat label="정원 초과 반" value={`${s.summary.overClasses}개`} />
        </div>
      )}

      <div className="toolbar" style={{ marginBottom: 10 }}>
        {TABS.map(([code, label]) => (
          <Button
            key={code}
            variant={tab === code ? 'primary' : 'default'}
            onClick={() => setTab(code)}
          >
            {label}
          </Button>
        ))}
      </div>

      {stats.isLoading && <Card>불러오는 중…</Card>}
      {s && tab === 'DEPT' && <DeptTab rows={s.rows} />}
      {s && tab === 'GRADE' && <GradeTab stats={s} />}
      {s && tab === 'WEEKDAY' && <WeekdayTab stats={s} />}
      {tab === 'SEATS' && <SeatsTab workspaceId={wsId} />}
    </div>
  )
}

// ─────────────────────────────────────────────── 부서별 현황

function DeptTab({ rows }: { rows: DeptCapacityRow[] }) {
  const [sort, setSort] = useState<SortSpec[]>([])
  const [query, setQuery] = useState('')
  const [day, setDay] = useState('')
  const [grade, setGrade] = useState('')
  const [status, setStatus] = useState('')

  const columns: Column<DeptCapacityRow>[] = [
    { key: 'name', head: '부서명', width: 130, sort: cmp.text((d) => d.name), render: (d) => d.name },
    {
      key: 'className',
      head: '반명',
      width: 72,
      sort: cmp.classNo((d) => d.className),
      render: (d) => d.className || <span className="muted">—</span>,
    },
    {
      key: 'teacher',
      head: '강사명',
      width: 90,
      sort: cmp.text((d) => d.teacher),
      render: (d) => d.teacher || <span className="muted">—</span>,
    },
    {
      key: 'days',
      head: '요일',
      width: 80,
      sort: cmp.days((d) => d.days),
      render: (d) => d.days || <span className="muted">—</span>,
    },
    {
      key: 'allowedGrades',
      head: '대상 학년',
      width: 104,
      sort: cmp.text((d) => gradeText(d.allowedGrades)),
      render: (d) =>
        d.allowedGrades.length === 0 ? (
          <span className="muted">미설정</span>
        ) : (
          gradeText(d.allowedGrades)
        ),
    },
    {
      key: 'capacity',
      head: '정원',
      align: 'num',
      width: 70,
      // 미설정을 0 으로 두면 '정원 0명'처럼 보인다 — 맨 뒤로 보낸다.
      sort: cmp.num((d) => d.capacity ?? Number.MAX_SAFE_INTEGER),
      render: (d) => (d.capacity === null ? <span className="muted">—</span> : people(d.capacity)),
    },
    {
      key: 'currentCount',
      head: '현재 수강',
      align: 'num',
      width: 84,
      sort: cmp.num((d) => d.currentCount),
      render: (d) => people(d.currentCount),
    },
    {
      key: 'remaining',
      head: '남은 자리',
      align: 'num',
      width: 84,
      sort: cmp.num((d) => d.remaining ?? Number.MAX_SAFE_INTEGER),
      render: (d) =>
        d.remaining === null ? (
          <span className="muted">—</span>
        ) : d.remaining < 0 ? (
          <b>{d.remaining}</b>
        ) : (
          people(d.remaining)
        ),
    },
    {
      key: 'fillRate',
      head: '충원율',
      align: 'num',
      width: 80,
      sort: cmp.num((d) => d.fillRate ?? -1),
      render: (d) => (d.fillRate === null ? <span className="muted">—</span> : percent(d.fillRate)),
    },
    {
      key: 'status',
      head: '상태',
      width: 96,
      // 손볼 것부터 위로 — 초과 → 도달 → 모집 가능 → 미설정
      sort: cmp.num((d) => STATUS_ORDER[d.status]),
      render: (d) => <span className={`tag tag--${STATUS_TONE[d.status]}`}>{STATUS_TEXT[d.status]}</span>,
    },
  ]

  const shown = useMemo(() => {
    const q = query.trim()
    const filtered = rows.filter((r) => {
      if (q && !`${r.name}${r.className}${r.teacher}`.includes(q)) return false
      if (day && !r.days.includes(day)) return false
      if (grade && !r.allowedGrades.includes(Number(grade))) return false
      if (status && r.status !== status) return false
      return true
    })
    return applySort(filtered, sort, Object.fromEntries(
      columns.filter((c) => c.sort).map((c) => [c.key, c.sort!]),
    ), (a, b) => a.departmentId - b.departmentId)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, sort, query, day, grade, status])

  return (
    <Card>
      <div className="toolbar" style={{ marginBottom: 8 }}>
        <Search value={query} onValue={setQuery} placeholder="부서명·반명·강사명" />
        <Select value={day} onChange={(e) => setDay(e.target.value)} style={{ width: 110 }}>
          <option value="">모든 요일</option>
          {['월', '화', '수', '목', '금', '토', '일'].map((d) => (
            <option key={d} value={d}>
              {d}요일
            </option>
          ))}
        </Select>
        <Select value={grade} onChange={(e) => setGrade(e.target.value)} style={{ width: 130 }}>
          <option value="">모든 대상 학년</option>
          {GRADES.map((g) => (
            <option key={g} value={g}>
              {g}학년 대상
            </option>
          ))}
        </Select>
        <Select value={status} onChange={(e) => setStatus(e.target.value)} style={{ width: 130 }}>
          <option value="">모든 상태</option>
          {(['OPEN', 'FULL', 'OVER', 'UNSET'] as const).map((c) => (
            <option key={c} value={c}>
              {STATUS_TEXT[c]}
            </option>
          ))}
        </Select>
        <span className="toolbar__spacer" />
        <span className="muted">
          {shown.length}개 / 전체 {rows.length}개
        </span>
        {sort.length > 0 && (
          <button type="button" className="linkBtn" onClick={() => setSort([])}>
            정렬 초기화
          </button>
        )}
      </div>

      <DataTable
        rows={shown}
        columns={columns}
        getId={(d) => d.departmentId}
        sort={sort}
        onSort={(next, message) => {
          if (message) alert(message)
          else setSort(next)
        }}
        empty="조건에 맞는 반이 없습니다."
      />
      <div className="hint" style={{ marginTop: 8 }}>
        남은 자리가 <b>음수</b>이면 정원을 넘긴 인원입니다. 윗줄의 <b>남은 자리</b>는 더 받을 수
        있는 자리만 더한 값이라, 넘친 반의 음수가 다른 반의 빈자리를 깎지 않습니다.
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 학년별 현황

function GradeTab({ stats }: { stats: import('@/ipc/types').CapacityStats }) {
  return (
    <Card>
      <table className="table">
        <thead>
          <tr>
            <th>학년</th>
            <th className="num">수강 학생 수</th>
            <th className="num">수강 건수</th>
            <th className="num">학년 전체 학생 수</th>
            <th className="num">참여율</th>
          </tr>
        </thead>
        <tbody>
          {stats.grades.map((g) => (
            <tr key={g.grade}>
              <td>{g.grade}학년</td>
              <td className="num">{people(g.students)}</td>
              <td className="num">{people(g.enrollments)}</td>
              <td className="num">{people(g.totalStudents)}</td>
              <td className="num">{percent(g.joinRate)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {stats.grades.length === 0 && <Empty title="학생이 없습니다">학생정보를 먼저 넣어 주세요.</Empty>}
      <div className="hint" style={{ marginTop: 8 }}>
        <b>수강 학생 수</b>는 사람 수, <b>수강 건수</b>는 신청 건수입니다. 한 학생이 셋을 들으면
        학생 1명 · 건수 3건입니다.
        <br />
        <b>참여율</b>의 분모는 이 학년도 학생명단에 있는 그 학년 전체 학생 수입니다. 학생명단은
        학년도 단위라, 작업공간 기간 중의 전입·전출은 반영되지 않습니다.
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 요일별 현황

function WeekdayTab({ stats }: { stats: import('@/ipc/types').CapacityStats }) {
  return (
    <Card>
      <table className="table">
        <thead>
          <tr>
            <th>요일</th>
            <th className="num">운영 반 수</th>
            <th className="num">수강 건수</th>
            <th className="num">정원 설정 반 수</th>
            <th className="num">설정된 총 정원</th>
            <th className="num">남은 자리</th>
          </tr>
        </thead>
        <tbody>
          {stats.weekdays.map((w) => (
            <tr key={w.day}>
              <td>{w.day === '미지정' ? <span className="muted">요일 미지정</span> : `${w.day}요일`}</td>
              <td className="num">{people(w.classes)}</td>
              <td className="num">{people(w.enrollments)}</td>
              <td className="num">{people(w.classesWithCapacity)}</td>
              <td className="num">{people(w.totalCapacity)}</td>
              <td className="num">{people(w.openSeats)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="hint" style={{ marginTop: 8 }}>
        <b>여러 요일에 운영하는 반은 각 요일에 모두 셉니다.</b> 그래야 ‘수요일에 몇 명이 남는가’를
        알 수 있습니다. 그래서 이 표를 세로로 더한 값은 전체 수강 건수(
        {people(stats.summary.totalEnrollments)}건)보다 클 수 있고, 합계 줄을 두지 않습니다.
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 수강 가능 부서 찾기

function SeatsTab({ workspaceId }: { workspaceId: number }) {
  const app = useApp()
  const [grade, setGrade] = useState(1)
  const [studentId, setStudentId] = useState<number | null>(null)
  const [includeClosed, setIncludeClosed] = useState(false)

  const students = useQuery({
    queryKey: ['students-for-seats', app.yearId, grade],
    queryFn: () => api.studentList(app.yearId, { grade }),
  })

  const result = useQuery({
    queryKey: ['seats', workspaceId, grade, studentId, includeClosed],
    queryFn: () => api.capacityFindSeats(workspaceId, { grade, studentId, includeClosed }),
  })

  // 학생은 학년도 소속이라 작업공간과 무관하다. 고른 학년만 서버에서 걸러 온다.
  const 후보 = students.data ?? []
  const r = result.data
  const 줄수 = (r?.byDay ?? []).reduce((n, d) => n + d.rows.length, 0)

  return (
    <Card>
      <div className="toolbar" style={{ marginBottom: 10 }}>
        <Field label="학년">
          <Select
            value={grade}
            onChange={(e) => {
              setGrade(Number(e.target.value))
              setStudentId(null)
            }}
            style={{ width: 110 }}
          >
            {GRADES.map((g) => (
              <option key={g} value={g}>
                {g}학년
              </option>
            ))}
          </Select>
        </Field>
        <Field label="학생">
          <Select
            value={studentId ?? ''}
            onChange={(e) => setStudentId(e.target.value === '' ? null : Number(e.target.value))}
            style={{ width: 200 }}
          >
            <option value="">선택 안 함</option>
            {후보.map((s) => (
              <option key={s.id} value={s.id}>
                {s.classNo}반 {s.studentNo}번 {s.name}
              </option>
            ))}
          </Select>
        </Field>
        <label className="checkItem">
          <input
            type="checkbox"
            checked={includeClosed}
            onChange={(e) => setIncludeClosed(e.target.checked)}
          />
          마감 부서도 보기
        </label>
        <span className="toolbar__spacer" />
        <span className="muted">{줄수}개 반</span>
      </div>

      {r && r.excluded.length > 0 && (
        <Notice tone="info">
          이미 수강 중이라 뺀 반: <b>{r.excluded.join(' · ')}</b>
        </Notice>
      )}

      {result.isLoading && <div>찾는 중…</div>}
      {r && 줄수 === 0 && (
        <Empty title="들어갈 수 있는 반이 없습니다">
          {includeClosed
            ? `${grade}학년을 대상으로 하는 반이 없습니다. 부서정보에서 수강 가능 학년을 확인해 주세요.`
            : '마감 부서도 보기를 켜면 정원이 찬 반까지 확인할 수 있습니다.'}
        </Empty>
      )}

      {(r?.byDay ?? []).map((g) => (
        <div className="dayGroup" key={g.day}>
          <div className="dayGroup__head">
            {g.day === '미지정' ? '요일 미지정' : `${g.day}요일`}
          </div>
          <table className="table">
            <thead>
              <tr>
                <th>부서 / 반</th>
                <th>강사명</th>
                <th>대상 학년</th>
                <th className="num">현재 / 정원</th>
                <th className="num">남은 자리</th>
                <th>판정</th>
              </tr>
            </thead>
            <tbody>
              {g.rows.map((x: SeatRow) => (
                <tr key={x.departmentId}>
                  <td>
                    {x.name}
                    {x.className && ` ${x.className}`}
                  </td>
                  <td>{x.teacher || <span className="muted">—</span>}</td>
                  <td>
                    {x.allowedGrades.length === 0 ? (
                      <span className="muted">미설정</span>
                    ) : (
                      gradeText(x.allowedGrades)
                    )}
                  </td>
                  <td className="num">
                    {x.currentCount} / {x.capacity === null ? '—' : x.capacity}
                  </td>
                  <td className="num">
                    {x.remaining === null ? (
                      <span className="muted">—</span>
                    ) : x.remaining > 0 ? (
                      `${x.remaining}자리`
                    ) : (
                      <b>{x.remaining}</b>
                    )}
                  </td>
                  <td>
                    <span className={`tag tag--${FINDING_TONE[x.finding]}`}>
                      {FINDING_TEXT[x.finding]}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}

      <div className="hint" style={{ marginTop: 8 }}>
        여러 요일에 운영하는 반은 각 요일에 모두 나옵니다. <b>대상 학년 확인 필요</b>는 그 반의
        수강 가능 학년이 아직 정해지지 않아 이 학년이 대상인지 알 수 없다는 뜻이고,
        <b> 정원 확인 필요</b>는 정원이 없어 자리를 셀 수 없다는 뜻입니다. 둘 다 부서정보에서
        채우면 사라집니다.
        <br />이 화면은 <b>안내용 조회</b>입니다. 여기서 수강이 등록되지는 않습니다. 시간표가 겹치는지는
        아직 판정하지 않으므로, 같은 요일이라도 수업 시간을 따로 확인해 주세요.
      </div>
    </Card>
  )
}
