/**
 * 맞춤형 강사료 (v0.1.8, 설계안 30장).
 *
 * ## 학생 수강료와 다른 돈이다
 *
 * 여기 금액은 **학교가 강사에게 주는 돈**이다. 학생이 내는 수강료와 그 정산·
 * 지원금에는 섞이지 않는다. 이 화면에서 무엇을 고쳐도 기존 정산은 낡음이 되지
 * 않는다.
 *
 * ## 셈은 전부 서버가 한다
 *
 * 예정 수업일 · 휴강 · 보강 · 차시 · 강사료는 `repo::custom_fee` 가 낸 값을
 * 그대로 쓴다. 화면이 날짜를 다시 세면 Excel 과 숫자가 갈린다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Input, Notice } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { CustomFeeReport, CustomFeeRow, SessionDay } from '@/ipc/types'
import { won } from '@/lib/format'
import { saveExport } from '@/lib/saveFile'
import { useApp } from '@/lib/useApp'

const KIND_TEXT: Record<SessionDay['kind'], string> = {
  NORMAL: '정상',
  CLOSED: '휴강',
  EXTRA: '보강',
}
const KIND_TONE: Record<SessionDay['kind'], string> = {
  NORMAL: 'free',
  CLOSED: 'plain',
  EXTRA: 'warn',
}

function Stat({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="statCard">
      <div className="statCard__label">{label}</div>
      <div className="statCard__value">{value}</div>
      {sub && <div className="statCard__sub">{sub}</div>}
    </div>
  )
}

export function CustomFeePage() {
  const app = useApp()
  const toast = useToast()
  const qc = useQueryClient()
  const wsId = app.workspaceId

  const [includeAll, setIncludeAll] = useState(false)
  const [detail, setDetail] = useState<CustomFeeRow | null>(null)
  const [closures, setClosures] = useState(false)
  const [rateOpen, setRateOpen] = useState(false)

  const rep = useQuery({
    queryKey: ['custom-fee', wsId, includeAll],
    queryFn: () => api.customFeeReport(wsId!, includeAll),
    enabled: wsId !== null,
  })

  const download = useMutation({
    mutationFn: () =>
      saveExport(() => api.customFeeExport(wsId!), toast, (r) => `반 ${r.rows}개`),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1>맞춤형 강사료</h1>
        </div>
        <Card>
          <Empty title="작업공간이 없습니다">
            수업일은 작업공간 기간 안에서 셉니다. [작업공간]에서 먼저 하나 만들어 주세요.
          </Empty>
        </Card>
      </div>
    )
  }

  const r = rep.data
  // 화면에 보이는 줄에서 다시 세지 않는다 — 요약은 서버가 낸 값이다.
  const s = r?.summary

  return (
    <div className="page">
      <div className="page__head">
        <h1>맞춤형 강사료</h1>
        <span className="toolbar__spacer" />
        <Button
          variant="primary"
          onClick={() => download.mutate()}
          disabled={!r || download.isPending}
          title="화면 설정과 무관하게 이 작업공간의 맞춤형 부서 전체를 냅니다"
        >
          {download.isPending ? '만드는 중…' : 'Excel 받기'}
        </Button>
      </div>

      {r && (
        <Card>
          <div className="toolbar" style={{ padding: 10 }}>
            <span>
              <b>{r.yearName}</b> · {r.workspaceName}
            </span>
            <span className="muted">
              적용 기간 {r.startDate} ~ {r.endDate}
            </span>
            <span className="toolbar__spacer" />
            <span>
              차시당 단가 <b>{won(r.rate)}원</b>
            </span>
            <Button small onClick={() => setRateOpen(true)}>
              단가 변경
            </Button>
            <Button small onClick={() => setClosures(true)}>
              휴업일 관리 ({r.closures.length})
            </Button>
          </div>
        </Card>
      )}

      {s && (
        <div className="statRow" style={{ marginTop: 12 }}>
          <Stat label="맞춤형 운영 반" value={`${s.classes}개`} />
          <Stat label="예정 수업일" value={`${s.plannedDays}일`} />
          <Stat label="휴강" value={`${s.closedDays}일`} />
          <Stat label="추가·보강" value={`${s.extraDays}일`} />
          <Stat label="인정 수업일" value={`${s.countedDays}일`} />
          <Stat label="총 인정 차시" value={`${s.totalSessions}차시`} />
          <Stat
            label="총 강사료"
            value={`${won(s.totalFee)}원`}
            sub={`${s.totalSessions}차시 × ${won(r?.rate ?? 0)}원`}
          />
        </div>
      )}

      {r && r.orphans.length > 0 && (
        <Notice tone="warn">
          <b>검토가 필요한 수업일 기록이 {r.orphans.length}건 있습니다.</b> 작업공간 기간이나 부서
          요일이 바뀌어 기본 예정일과 이어지지 않는 기록입니다. 자동으로 지우지 않았습니다.
          <ul style={{ margin: '6px 0 0', paddingLeft: 18 }}>
            {r.orphans.map((o) => (
              <li key={`${o.departmentId}-${o.date}`}>
                {o.departmentLabel} · {o.date} ({o.sessions}차시) — {o.why}
              </li>
            ))}
          </ul>
        </Notice>
      )}

      <Card>
        <div className="toolbar" style={{ marginBottom: 8 }}>
          <label className="checkItem">
            <input
              type="checkbox"
              checked={includeAll}
              onChange={(e) => setIncludeAll(e.target.checked)}
            />
            맞춤형 아닌 부서도 보기
          </label>
          <span className="toolbar__spacer" />
          <span className="muted">줄을 두 번 누르면 수업일을 관리합니다</span>
        </div>

        {rep.isLoading && <div>불러오는 중…</div>}
        {r && r.rows.length === 0 && (
          <Empty title="맞춤형 프로그램이 없습니다">
            [기초 데이터 &gt; 부서정보]에서 맞춤형 프로그램 차시를 1 이상으로 두면 여기에
            나타납니다.
          </Empty>
        )}

        {r && r.rows.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>부서명</th>
                <th>반명</th>
                <th>강사명</th>
                <th>요일</th>
                <th className="num">기본 차시</th>
                <th className="num">예정</th>
                <th className="num">휴강</th>
                <th className="num">추가·보강</th>
                <th className="num">인정 수업일</th>
                <th className="num">총 차시</th>
                <th className="num">차시당 단가</th>
                <th className="num">강사료</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {r.rows.map((x) => (
                <tr
                  key={x.departmentId}
                  onDoubleClick={() => x.baseSessions > 0 && setDetail(x)}
                  style={{ cursor: x.baseSessions > 0 ? 'pointer' : undefined }}
                >
                  <td>{x.name}</td>
                  <td>{x.className || <span className="muted">—</span>}</td>
                  <td>{x.teacher || <span className="muted">—</span>}</td>
                  <td>{x.days || <span className="muted">—</span>}</td>
                  <td className="num">
                    {x.baseSessions > 0 ? x.baseSessions : <span className="muted">맞춤형 아님</span>}
                  </td>
                  <td className="num">{x.plannedDays}</td>
                  <td className="num">{x.closedDays}</td>
                  <td className="num">{x.extraDays}</td>
                  <td className="num">{x.countedDays}</td>
                  <td className="num">{x.totalSessions}</td>
                  <td className="num">{won(x.rate)}</td>
                  <td className="num">
                    <b>{won(x.fee)}</b>
                  </td>
                  <td>
                    {x.baseSessions > 0 && (
                      <Button small onClick={() => setDetail(x)}>
                        수업일 관리
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        <div className="hint" style={{ marginTop: 8 }}>
          강사료는 <b>총 인정 차시 × 차시당 단가</b>입니다. 예정 수업일은 작업공간 기간과 부서
          요일에서 달력대로 세며, 휴강·보강·차시 조정만 따로 기록합니다.
          <br />
          이 금액은 <b>학생이 내는 수강료와 다른 회계</b>입니다. 여기서 무엇을 고쳐도 학생 수강료
          정산에는 영향이 없습니다.
        </div>
      </Card>

      {detail && (
        <SessionModal
          row={detail}
          onClose={() => setDetail(null)}
          onChanged={() => void qc.invalidateQueries({ queryKey: ['custom-fee'] })}
        />
      )}
      {closures && r && (
        <ClosureModal
          workspaceId={wsId}
          report={r}
          onClose={() => setClosures(false)}
          onChanged={() => void qc.invalidateQueries({ queryKey: ['custom-fee'] })}
        />
      )}
      {rateOpen && r && (
        <RateModal
          yearId={app.yearId}
          rate={r.rate}
          onClose={() => setRateOpen(false)}
          onChanged={() => void qc.invalidateQueries({ queryKey: ['custom-fee'] })}
        />
      )}
    </div>
  )
}

// ─────────────────────────────────────────────── 수업일 관리

function SessionModal({
  row,
  onClose,
  onChanged,
}: {
  row: CustomFeeRow
  onClose: () => void
  onChanged: () => void
}) {
  const app = useApp()
  const toast = useToast()
  const [addDate, setAddDate] = useState('')
  const [addSessions, setAddSessions] = useState(row.baseSessions)
  const [addReason, setAddReason] = useState('')

  // 표를 늘 최신으로 — 고친 뒤 바로 다시 읽는다
  const rep = useQuery({
    queryKey: ['custom-fee', app.workspaceId, false],
    queryFn: () => api.customFeeReport(app.workspaceId!, false),
  })
  const live = rep.data?.rows.find((x) => x.departmentId === row.departmentId) ?? row

  const run = useMutation({
    mutationFn: (f: () => Promise<void>) => f(),
    onSuccess: () => {
      void rep.refetch()
      onChanged()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const set = (date: string, sessions: number, reason: string) =>
    run.mutate(() => api.customSessionSet(row.departmentId, date, sessions, reason))
  const clear = (date: string) =>
    run.mutate(() => api.customSessionClear(row.departmentId, date))

  return (
    <Modal
      wide
      title={`수업일 관리 — ${row.name}${row.className}`}
      onClose={onClose}
      footer={<Button onClick={onClose}>닫기</Button>}
    >
      <div className="toolbar" style={{ marginBottom: 10 }}>
        <span>
          기본 <b>{live.baseSessions}차시</b> · 요일 {live.days || '—'}
        </span>
        <span className="toolbar__spacer" />
        <span>
          인정 {live.countedDays}일 · <b>{live.totalSessions}차시</b> ·{' '}
          <b>{won(live.fee)}원</b>
        </span>
      </div>

      <table className="table">
        <thead>
          <tr>
            <th>날짜</th>
            <th>요일</th>
            <th>구분</th>
            <th className="num">차시</th>
            <th>사유/메모</th>
            <th style={{ width: 210 }} />
          </tr>
        </thead>
        <tbody>
          {live.sessionsList.map((d) => (
            <tr key={d.date}>
              <td>{d.date}</td>
              <td>{d.weekday}</td>
              <td>
                <span className={`tag tag--${KIND_TONE[d.kind]}`}>{KIND_TEXT[d.kind]}</span>
                {d.fromClosure && <span className="muted"> 휴업일</span>}
              </td>
              <td className="num">
                <Input
                  type="number"
                  min={0}
                  style={{ width: 70 }}
                  value={d.sessions}
                  onChange={(e) =>
                    set(d.date, Math.max(0, Number(e.target.value) || 0), d.reason)
                  }
                />
              </td>
              <td>
                {/* 글자마다 저장하면 이력이 쌓이므로 칸을 벗어날 때 한 번만 보낸다.
                    `key` 를 날짜·사유로 두어 서버 값이 바뀌면 칸도 따라 바뀐다. */}
                <Input
                  key={`${d.date}-${d.reason}`}
                  defaultValue={d.reason}
                  placeholder={d.kind === 'CLOSED' ? '공휴일 · 강사 사정 …' : ''}
                  onBlur={(e) => {
                    if (e.target.value !== d.reason) set(d.date, d.sessions, e.target.value)
                  }}
                />
              </td>
              <td>
                {d.sessions > 0 ? (
                  <Button small onClick={() => set(d.date, 0, d.reason || '휴강')}>
                    휴강
                  </Button>
                ) : (
                  <Button small onClick={() => set(d.date, live.baseSessions, d.reason)}>
                    휴강 해제
                  </Button>
                )}
                {d.overridden && (
                  <Button small onClick={() => clear(d.date)} style={{ marginLeft: 6 }}>
                    {d.kind === 'EXTRA' ? '삭제' : '기본으로'}
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <div style={{ height: 14 }} />
      <div className="card__title" style={{ marginBottom: 8 }}>
        수업일 추가 (보강)
      </div>
      <div className="toolbar">
        <Field label="날짜">
          <Input
            type="date"
            value={addDate}
            onChange={(e) => setAddDate(e.target.value)}
            style={{ width: 170 }}
          />
        </Field>
        <Field label="차시">
          <Input
            type="number"
            min={1}
            value={addSessions}
            onChange={(e) => setAddSessions(Math.max(1, Number(e.target.value) || 1))}
            style={{ width: 80 }}
          />
        </Field>
        <Field label="사유">
          <Input
            value={addReason}
            placeholder="10/20 보강"
            onChange={(e) => setAddReason(e.target.value)}
            style={{ width: 220 }}
          />
        </Field>
        <Button
          variant="primary"
          disabled={!addDate || run.isPending}
          onClick={() => {
            set(addDate, addSessions, addReason)
            setAddDate('')
            setAddReason('')
          }}
        >
          추가
        </Button>
      </div>
      <div className="hint" style={{ marginTop: 8 }}>
        운영 요일이 아닌 날짜도 넣을 수 있습니다. 작업공간 기간 안이어야 합니다. 차시를 <b>0</b>
        으로 두면 휴강이고, [기본으로]를 누르면 조정을 지워 기본 차시로 돌아갑니다.
      </div>
    </Modal>
  )
}

// ─────────────────────────────────────────────── 공통 휴업일

function ClosureModal({
  workspaceId,
  report,
  onClose,
  onChanged,
}: {
  workspaceId: number
  report: CustomFeeReport
  onClose: () => void
  onChanged: () => void
}) {
  const toast = useToast()
  const qc = useQueryClient()
  const [date, setDate] = useState('')
  const [reason, setReason] = useState('')

  const run = useMutation({
    mutationFn: (f: () => Promise<void>) => f(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['custom-fee'] })
      onChanged()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      title="작업공간 공통 휴업일"
      onClose={onClose}
      footer={<Button onClick={onClose}>닫기</Button>}
    >
      <div className="hint" style={{ marginBottom: 10 }}>
        공휴일·재량휴업일처럼 <b>그날 예정된 모든 맞춤형 반</b>이 쉬는 날입니다. 어느 한 반만
        쉬는 날은 그 반의 [수업일 관리]에서 휴강으로 두세요. 그쪽에 적어 둔 값이 있으면 그것이
        휴업일보다 앞섭니다.
      </div>

      <div className="toolbar" style={{ marginBottom: 10 }}>
        <Field label="날짜">
          <Input
            type="date"
            value={date}
            min={report.startDate}
            max={report.endDate}
            onChange={(e) => setDate(e.target.value)}
            style={{ width: 170 }}
          />
        </Field>
        <Field label="사유">
          <Input
            value={reason}
            placeholder="한글날 · 재량휴업일"
            onChange={(e) => setReason(e.target.value)}
            style={{ width: 220 }}
          />
        </Field>
        <Button
          variant="primary"
          disabled={!date || run.isPending}
          onClick={() => {
            run.mutate(() => api.customClosureAdd(workspaceId, date, reason))
            setDate('')
            setReason('')
          }}
        >
          등록
        </Button>
      </div>

      {report.closures.length === 0 ? (
        <Empty title="등록된 휴업일이 없습니다">
          공휴일이나 재량휴업일을 넣으면 그날 예정된 모든 맞춤형 반에서 빠집니다.
        </Empty>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>날짜</th>
              <th>요일</th>
              <th>사유</th>
              <th style={{ width: 80 }} />
            </tr>
          </thead>
          <tbody>
            {report.closures.map((c) => (
              <tr key={c.id}>
                <td>{c.date}</td>
                <td>{c.weekday}</td>
                <td>{c.reason || <span className="muted">—</span>}</td>
                <td>
                  <Button small onClick={() => run.mutate(() => api.customClosureDelete(c.id))}>
                    삭제
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="hint" style={{ marginTop: 8 }}>
        휴업일을 지우면 그날은 다시 정상 예정일로 돌아갑니다.
      </div>
    </Modal>
  )
}

// ─────────────────────────────────────────────── 차시당 단가

function RateModal({
  yearId,
  rate,
  onClose,
  onChanged,
}: {
  yearId: number
  rate: number
  onClose: () => void
  onChanged: () => void
}) {
  const toast = useToast()
  const [v, setV] = useState(rate)

  const save = useMutation({
    mutationFn: () => api.customRateSet(yearId, v),
    onSuccess: () => {
      toast.ok(`차시당 단가를 ${won(v)}원으로 저장했습니다.`)
      onChanged()
      onClose()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      title="맞춤형 차시당 단가"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending}>
            저장
          </Button>
        </>
      }
    >
      <Field label="차시당 강사료">
        <div className="inlineRow">
          <Input
            type="number"
            min={0}
            step={1000}
            value={v}
            onChange={(e) => setV(Math.max(0, Number(e.target.value) || 0))}
            style={{ width: 140 }}
          />
          <span className="muted">원</span>
        </div>
      </Field>
      <div className="hint" style={{ marginTop: 8 }}>
        단가는 <b>학년도마다</b> 둡니다. 올해 단가를 고쳐도 지난 학년도의 강사료 자료는 그대로
        남습니다. 이 값은 학생 수강료·정산과 무관합니다.
      </div>
    </Modal>
  )
}
