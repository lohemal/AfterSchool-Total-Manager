/**
 * 방과후드림 결제관리 — 매칭 및 검증 (P3).
 *
 * 판정은 모두 서버가 한다. 화면은 결과를 보여 주고, 사람이 확정할 것(개인참조번호 등록 ·
 * 생년월일 등록 · 강좌코드 연결)을 받아 서버에 넘긴다. 확정할 때 화면은 가린 값만 가지고
 * 있고, 실제 번호 · 생년월일은 서버가 고른 파일에서 다시 읽는다.
 *
 * 확정한 뒤에는 그 한 줄이 아니라 파일 전체를 다시 검증한 결과로 바꿔 그린다.
 *
 * 모두 준비 완료가 되면 아래에 배분 우선순위 · 배분 계산(P4, `DreamAllocPanel`)이 이어진다.
 */

import { useMutation } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'

import { Confirm } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Notice, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type {
  DreamCourseMatch,
  DreamFileCheck,
  DreamMatchState,
  DreamSession,
  DreamStudentMatch,
} from '@/ipc/types'
import { matchStateLabel, reasonText } from '@/lib/dream'

import { DreamAllocPanel } from './DreamAllocPanel'

function StateTag({ s }: { s: DreamMatchState }) {
  const cls = s === 'READY' ? 'tag tag--free' : s === 'NEEDS_CONFIRMATION' ? 'tag tag--warn' : 'tag tag--bad'
  return <span className={cls}>{matchStateLabel(s)}</span>
}

const won = (n: number | null | undefined) => (n === null || n === undefined ? '—' : n.toLocaleString('ko-KR'))

/** 작업이 더는 쓸 수 없게 된 오류 — 처음(파일 선택 · 검사)부터 다시 */
const RESTART = new Set(['FILE_CHANGED', 'SETTLEMENT_CHANGED', 'SETTLEMENT_NOT_FRESH'])

export function DreamMatchPanel({
  workspaceId,
  path,
  check,
}: {
  workspaceId: number
  path: string
  check: DreamFileCheck
}) {
  const toast = useToast()
  const [session, setSession] = useState<DreamSession | null>(null)
  // 다른 파일 · 다른 검사 결과면 진행 중이던 작업은 버린다
  useEffect(() => setSession(null), [path, check.sha256, workspaceId])

  const failed = (e: unknown) => {
    const code = (e as { code?: string }).code ?? ''
    toast.bad(errorMessage(e))
    if (RESTART.has(code)) setSession(null)
  }
  const run = useMutation({
    mutationFn: (f: () => Promise<DreamSession>) => f(),
    onSuccess: (s) => setSession(s),
    onError: failed,
  })

  if (check.status === 'ERROR') return null

  if (!session) {
    return (
      <Card title="매칭 및 검증">
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <Button
            variant="primary"
            disabled={run.isPending}
            onClick={() => run.mutate(() => api.dreamStart(workspaceId, path, check.sha256))}
          >
            {run.isPending ? '검증하는 중…' : '매칭 및 검증 시작'}
          </Button>
          <span className="hint">
            이 파일을 처리 대상으로 확정하고 학생 · 강좌 · 항목 · 금액을 통합 매니저 정산 자료와
            맞대어 봅니다. 정산이 최신이어야 합니다. 정산 자료는 고치지 않습니다.
          </span>
        </div>
      </Card>
    )
  }

  const m = session.matching
  const jobId = session.jobId
  return (
    <>
      <Card
        title="매칭 및 검증"
        actions={
          <Button small disabled={run.isPending} onClick={() => run.mutate(() => api.dreamRevalidate(jobId, path))}>
            다시 검증
          </Button>
        }
      >
        <div className="stat">
          <Stat label="파일 학생" value={m.students.length} />
          <Stat label="준비 완료" value={m.ready} />
          <Stat label="확인 필요" value={m.needsConfirmation} />
          <Stat label="오류" value={m.errors} />
          <div className="stat__item">
            <div className="stat__label">다음 단계(배분)</div>
            <div className="stat__value">
              {m.p4Ready ? <span className="tag tag--free">진행 가능</span> : <span className="tag tag--bad">진행 불가</span>}
            </div>
            <div className="stat__sub">모든 학생이 준비 완료여야 합니다</div>
          </div>
        </div>
        <div className="hint" style={{ margin: '4px 0' }}>
          파일 전체 정산금액 {won(m.fileTotal)}원 · 최신 정산 전체 {won(m.settlementTotal)}원{' '}
          {m.settlementId !== null &&
            (m.fileTotal === m.settlementTotal ? (
              <span className="tag tag--free">일치</span>
            ) : (
              <span className="tag tag--bad">불일치</span>
            ))}
        </div>
        {m.fileReasons.map((r, i) => (
          <Notice key={i} tone={r.state === 'READY' ? 'info' : 'bad'}>
            {reasonText(r)}
          </Notice>
        ))}
        {m.absent.length > 0 && (
          <div className="tableWrap" style={{ maxHeight: 240 }}>
            <table className="table">
              <thead>
                <tr>
                  <th className="left">정산에 있는데 파일에 없는 학생</th>
                  <th className="left">부서</th>
                  <th>수강 상태</th>
                  <th className="num">정산 금액</th>
                </tr>
              </thead>
              <tbody>
                {m.absent.map((a) => (
                  <tr key={a.studentId}>
                    <td className="left">{a.label}</td>
                    <td className="left">{a.depts.join(', ')}</td>
                    <td>{a.cancelled ? '취소(금액 남음)' : '수강 중'}</td>
                    <td className="num">{won(a.total)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      <CourseCard
        courses={m.courses}
        departments={m.departments}
        busy={run.isPending}
        onConfirm={(code, dept) => run.mutate(() => api.dreamConfirmCourse(jobId, path, code, dept))}
      />
      <IdentityCard
        students={m.students}
        busy={run.isPending}
        onConfirm={(picks) => run.mutate(() => api.dreamConfirmIdentities(jobId, path, picks))}
        onBirth={(keys) => run.mutate(() => api.dreamConfirmBirths(jobId, path, keys))}
      />
      <StudentCard students={m.students} />
      <DreamAllocPanel workspaceId={workspaceId} path={path} session={session} onSession={setSession} onError={failed} />
    </>
  )
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="stat__item">
      <div className="stat__label">{label}</div>
      <div className="stat__value">{value.toLocaleString('ko-KR')}</div>
    </div>
  )
}

// ─────────────────────────────────────────────── 강좌코드 연결

const WHY: Record<string, string> = {
  SAME_NAME: '이름 같음',
  PREVIOUS_WORKSPACE: '직전 작업공간 연결',
  STUDENT_ENROLLED: '학생 수강 중',
}

function CourseCard({
  courses,
  departments,
  busy,
  onConfirm,
}: {
  courses: DreamCourseMatch[]
  departments: { id: number; label: string }[]
  busy: boolean
  onConfirm: (code: string, dept: number) => void
}) {
  const [pick, setPick] = useState<Record<string, number>>({})
  return (
    <Card title="강좌코드 연결" flush>
      <div className="hint" style={{ padding: '8px 12px' }}>
        강좌명코드 하나를 이 작업공간 부서 하나에 잇습니다. 한 번 확정하면 이 파일의 모든 학생에게
        적용됩니다. 추천은 참고일 뿐이며, [연결 확정]을 눌러야 저장됩니다.
      </div>
      <div className="tableWrap">
        <table className="table">
          <thead>
            <tr>
              <th>강좌명코드</th>
              <th className="left">파일 강좌명</th>
              <th>줄</th>
              <th className="left">현재 연결</th>
              <th className="left">추천 · 선택</th>
              <th>상태</th>
            </tr>
          </thead>
          <tbody>
            {courses.map((c) => {
              const chosen = pick[c.courseCode] ?? c.suggestions.find((s) => !s.taken)?.departmentId
              return (
                <tr key={c.courseCode}>
                  <td>{c.courseCode}</td>
                  <td className="left">{c.courseNames.join(', ')}</td>
                  <td>{c.rows}</td>
                  <td className="left">{c.deptLabel ?? '—'}</td>
                  <td className="left">
                    {c.departmentId === null ? (
                      <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                        <Select
                          value={chosen ?? ''}
                          onChange={(e) => setPick({ ...pick, [c.courseCode]: Number(e.target.value) })}
                          style={{ minWidth: 200 }}
                        >
                          <option value="">부서 선택</option>
                          {c.suggestions.map((s) => (
                            <option key={`s${s.departmentId}`} value={s.departmentId} disabled={s.taken}>
                              ★ {s.label} ({s.why.map((w) => WHY[w]).join(' · ')}){s.taken ? ' — 다른 코드에 연결됨' : ''}
                            </option>
                          ))}
                          {departments
                            .filter((d) => !c.suggestions.some((s) => s.departmentId === d.id))
                            .map((d) => (
                              <option key={d.id} value={d.id}>
                                {d.label}
                              </option>
                            ))}
                        </Select>
                        <Button small disabled={busy || !chosen} onClick={() => chosen && onConfirm(c.courseCode, chosen)}>
                          연결 확정
                        </Button>
                      </div>
                    ) : (
                      '—'
                    )}
                    {c.previous && c.departmentId === null && (
                      <div className="hint">
                        직전 작업공간({c.previous[0]})에서는 “{c.previous[1]}”에 연결됨
                      </div>
                    )}
                    {c.reasons
                      .filter((r) => r.state === 'ERROR')
                      .map((r, i) => (
                        <div key={i} style={{ color: 'var(--red-600)' }}>
                          {reasonText(r)}
                        </div>
                      ))}
                  </td>
                  <td>
                    <StateTag s={c.state} />
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 개인참조번호 · 생년월일 등록

function IdentityCard({
  students,
  busy,
  onConfirm,
  onBirth,
}: {
  students: DreamStudentMatch[]
  busy: boolean
  onConfirm: (picks: [number, number][]) => void
  onBirth: (keys: number[]) => void
}) {
  // 확인할 학생: 개인참조번호 미등록 · 후보가 있고 · 학생 정보가 줄마다 같은 학생
  const pending = useMemo(
    () =>
      students.filter(
        (s) =>
          s.refState === 'NOT_REGISTERED' &&
          s.candidates.length > 0 &&
          !s.reasons.some((r) => r.code === 'STUDENT_INFO_INCONSISTENT'),
      ),
    [students],
  )
  const births = useMemo(
    () =>
      students.filter(
        (s) =>
          s.refState === 'MATCHED' &&
          s.birthState === 'NOT_REGISTERED' &&
          !s.reasons.some((r) => r.state === 'ERROR' && r.code.startsWith('STUDENT_')),
      ),
    [students],
  )
  const [picked, setPicked] = useState<Record<number, number | undefined>>({})
  const [asking, setAsking] = useState<'ref' | 'birth' | null>(null)
  useEffect(() => setPicked({}), [students])

  if (pending.length === 0 && births.length === 0) return null
  const chosen: [number, number][] = pending
    .filter((s) => picked[s.key] !== undefined)
    .map((s) => [s.key, picked[s.key]!])
  const singles = pending.filter((s) => s.candidates.length === 1)

  return (
    <Card title="개인참조번호 · 생년월일 확인" flush>
      {pending.length > 0 && (
        <>
          <div className="hint" style={{ padding: '8px 12px' }}>
            개인참조번호가 아직 등록되지 않은 학생입니다. 학년 · 반 · 번호 · 이름이 모두 같은 학생만
            후보로 나옵니다. 맞는 학생을 직접 고르세요. 자동으로 저장하지 않습니다. 확인하면 파일의
            개인참조번호와 생년월일이 암호화되어 저장됩니다.
          </div>
          <div style={{ padding: '0 12px 8px', display: 'flex', gap: 8 }}>
            <Button
              small
              disabled={singles.length === 0}
              onClick={() =>
                setPicked(Object.fromEntries(singles.map((s) => [s.key, s.candidates[0].studentId])))
              }
            >
              후보가 하나인 학생 {singles.length}명 모두 고르기
            </Button>
            <Button small onClick={() => setPicked({})}>
              선택 해제
            </Button>
            <Button small variant="primary" disabled={busy || chosen.length === 0} onClick={() => setAsking('ref')}>
              고른 {chosen.length}명 확인 · 등록
            </Button>
          </div>
          <div className="tableWrap" style={{ maxHeight: 320 }}>
            <table className="table">
              <thead>
                <tr>
                  <th className="left">파일 학생</th>
                  <th>개인참조번호</th>
                  <th className="left">후보(통합 매니저)</th>
                  <th>상태</th>
                </tr>
              </thead>
              <tbody>
                {pending.map((s) => (
                  <tr key={s.key}>
                    <td className="left">
                      {s.fileGrade} {s.fileClass} {s.fileNo}번 {s.fileName}
                    </td>
                    <td>{s.refMasked}</td>
                    <td className="left">
                      <Select
                        value={picked[s.key] ?? ''}
                        onChange={(e) =>
                          setPicked({ ...picked, [s.key]: e.target.value ? Number(e.target.value) : undefined })
                        }
                      >
                        <option value="">{s.candidates.length > 1 ? `후보 ${s.candidates.length}명 — 고르세요` : '고르지 않음'}</option>
                        {s.candidates.map((c) => (
                          <option key={c.studentId} value={c.studentId}>
                            {c.label}
                          </option>
                        ))}
                      </Select>
                    </td>
                    <td>
                      <StateTag s={s.state} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      {births.length > 0 && (
        <div style={{ padding: 12, display: 'flex', gap: 8, alignItems: 'center' }}>
          <span>
            개인참조번호는 확인됐지만 생년월일이 없는 학생 <b>{births.length}명</b>
          </span>
          <Button small disabled={busy} onClick={() => setAsking('birth')}>
            파일의 생년월일 등록
          </Button>
        </div>
      )}
      {asking === 'ref' && (
        <Confirm
          title="개인참조번호 확인 · 등록"
          confirmText={`${chosen.length}명 등록`}
          busy={busy}
          onClose={() => setAsking(null)}
          onConfirm={() => {
            onConfirm(chosen)
            setAsking(null)
          }}
          message={
            <>
              고른 <b>{chosen.length}명</b>에게 파일의 개인참조번호와 생년월일을 등록합니다. 값은
              암호화되어 저장됩니다. 다른 학생에게 이미 등록된 번호면 아무것도 저장하지 않습니다.
              <div className="hint" style={{ marginTop: 8 }}>
                학생 · 수강 · 정산 자료는 바뀌지 않습니다.
              </div>
            </>
          }
        />
      )}
      {asking === 'birth' && (
        <Confirm
          title="생년월일 등록"
          confirmText={`${births.length}명 등록`}
          busy={busy}
          onClose={() => setAsking(null)}
          onConfirm={() => {
            onBirth(births.map((s) => s.key))
            setAsking(null)
          }}
          message={
            <>
              개인참조번호로 확인된 <b>{births.length}명</b>에게 파일의 생년월일을 암호화해 등록합니다.
            </>
          }
        />
      )}
    </Card>
  )
}

// ─────────────────────────────────────────────── 학생별 결과

function StudentCard({ students }: { students: DreamStudentMatch[] }) {
  const [filter, setFilter] = useState<DreamMatchState | ''>('')
  const [open, setOpen] = useState<number | null>(null)
  const rows = filter ? students.filter((s) => s.state === filter) : students
  const selected = students.find((s) => s.key === open) ?? null

  return (
    <Card
      title="학생별 결과"
      flush
      actions={
        <Select value={filter} onChange={(e) => setFilter(e.target.value as DreamMatchState | '')} style={{ width: 120 }}>
          <option value="">전체</option>
          <option value="READY">준비 완료</option>
          <option value="NEEDS_CONFIRMATION">확인 필요</option>
          <option value="ERROR">오류</option>
        </Select>
      }
    >
      <div className="tableWrap" style={{ maxHeight: 420 }}>
        <table className="table">
          <thead>
            <tr>
              <th className="left">파일 학생</th>
              <th className="left">통합 매니저 학생</th>
              <th>줄</th>
              <th className="num">파일 합계(ΣN)</th>
              <th className="num">정산 총액(T)</th>
              <th>개인참조번호</th>
              <th>생년월일</th>
              <th>상태</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((s) => (
              <tr key={s.key} className={open === s.key ? 'on' : undefined} onClick={() => setOpen(open === s.key ? null : s.key)} style={{ cursor: 'pointer' }}>
                <td className="left">
                  {s.fileGrade} {s.fileClass} {s.fileNo}번 {s.fileName}
                </td>
                <td className="left">{s.studentLabel ?? '—'}</td>
                <td>{s.rows.length}</td>
                <td className="num">{won(s.sumN)}</td>
                <td className="num">{won(s.amount?.total)}</td>
                <td>{refText(s.refState)}</td>
                <td>{birthText(s.birthState)}</td>
                <td>
                  <StateTag s={s.state} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {selected && <StudentDetail s={selected} />}
    </Card>
  )
}

function refText(s: DreamStudentMatch['refState']) {
  return s === 'MATCHED' ? '확인됨' : s === 'NOT_REGISTERED' ? '미등록' : s === 'CONFLICT' ? '충돌' : '열 수 없음'
}
function birthText(s: DreamStudentMatch['birthState']) {
  return s === 'MATCHED' ? '일치' : s === 'NOT_REGISTERED' ? '미등록' : s === 'MISMATCH' ? '불일치' : '—'
}

function StudentDetail({ s }: { s: DreamStudentMatch }) {
  const a = s.amount
  return (
    <div style={{ padding: 12, borderTop: '1px solid var(--gray-200)' }}>
      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', marginBottom: 8 }}>
        <div>
          <b>파일</b> {s.fileGrade} {s.fileClass} {s.fileNo}번 {s.fileName} · 개인참조번호 {s.refMasked} · 생년월일 {s.birthMasked}
        </div>
        <div>
          <b>통합 매니저</b> {s.studentLabel ?? '—'}
        </div>
      </div>
      {a && (
        <div className="hint" style={{ marginBottom: 8 }}>
          기준액(배분 전) — 정산 총액 {won(a.total)} · 이용권 {won(a.voucher)} · 자유수강권 {won(a.freeVoucher)} · 개인부담{' '}
          {won(a.personal)} &nbsp;|&nbsp; 원본 파일 합계 — 개인부담금 {won(s.before[0])} · 이용권 {won(s.before[1])} · 자유수강권{' '}
          {won(s.before[2])} (참고)
        </div>
      )}
      {s.reasons.length > 0 && (
        <ul style={{ margin: '0 0 8px', paddingLeft: 18 }}>
          {s.reasons.map((r, i) => (
            <li key={i} style={{ color: r.state === 'ERROR' ? 'var(--red-600)' : 'var(--amber-600)' }}>
              {reasonText(r)}
            </li>
          ))}
        </ul>
      )}
      <table className="table">
        <thead>
          <tr>
            <th>Excel 행</th>
            <th className="left">부서(강좌코드)</th>
            <th>항목</th>
            <th className="num">파일 N</th>
            <th className="num">정산 Charge</th>
            <th className="num">차이</th>
            <th>상태</th>
            <th className="left">까닭</th>
          </tr>
        </thead>
        <tbody>
          {s.lines.map((l) => (
            <tr key={l.sourceRow}>
              <td>{l.sourceRow}</td>
              <td className="left">
                {l.deptLabel ?? '—'} ({l.courseCode})
              </td>
              <td>{l.itemName}</td>
              <td className="num">{won(l.amountN)}</td>
              <td className="num">{won(l.charge)}</td>
              <td className="num">{l.charge === null ? '—' : won(l.amountN - l.charge)}</td>
              <td>
                <StateTag s={l.state} />
              </td>
              <td className="left">{l.reasons.map((r) => reasonText({ ...r, row: null })).join(' / ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {s.missing.length > 0 && (
        <Notice tone="bad">
          정산에는 있는데 파일에 없는 금액:{' '}
          {s.missing.map((x) => `${x.deptLabel} ${x.itemName} ${won(x.amount)}원`).join(', ')}
        </Notice>
      )}
    </div>
  )
}
