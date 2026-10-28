/**
 * 방과후드림 결제관리 (P2 — 파일 읽기 · 구조 검증 · 작업공간 연결 · 읽기 전용 미리보기).
 *
 * 이 단계에서는 금액을 배분하거나 파일을 고치지 않는다. 고른 결제파일을 서버가 읽어
 * 검사하고, 화면은 그 결과를 보여 주기만 한다. 규칙은 모두 Rust 에 있다.
 *
 * 개인참조번호 · 생년월일은 서버가 가려서 보낸다. 화면은 원래 값을 받지 않는다.
 */

import { open } from '@tauri-apps/plugin-dialog'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'

import { Confirm } from '@/components/Modal'
import { DEFAULT_PAGE_SIZE, Pager } from '@/components/Pager'
import { useToast } from '@/components/Toast'
import { Button, Card, Field, Input, Notice, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { DreamFileCheck, DreamIssue } from '@/ipc/types'
import {
  cellText,
  DREAM_HEADERS,
  DREAM_LETTERS,
  issueText,
  monthCode,
  monthLabel,
  statusLabel,
} from '@/lib/dream'
import { useApp } from '@/lib/useApp'

/** 줄 문제는 이만큼만 한꺼번에 그린다 — 수백 개여도 화면이 무겁지 않게 */
const ISSUE_LIMIT = 200

export function DreamPaymentPage() {
  const app = useApp()
  const wsId = app.workspaceId!

  // 들어올 때 한 번 — 30일 지난 처리 이력 정리 + 개인정보 키 상태
  const enter = useQuery({
    queryKey: ['dream-enter'],
    queryFn: () => api.dreamEnter(),
    staleTime: Infinity,
  })
  const settings = useQuery({
    queryKey: ['dream-settings', wsId],
    queryFn: () => api.dreamSettings(wsId),
  })

  const [path, setPath] = useState<string | null>(null)
  // 작업공간 · 설정이 바뀌면 같은 파일을 다시 검사한다(파일은 읽기만 한다)
  const check = useQuery({
    queryKey: ['dream-check', wsId, path, settings.data?.orgCode, settings.data?.billingMonth],
    queryFn: () => api.dreamCheckFile(wsId, path!),
    enabled: path !== null && settings.isSuccess,
    retry: false,
  })

  async function pick() {
    const p = await open({
      multiple: false,
      filters: [{ name: '방과후드림 결제파일', extensions: ['xlsx'] }],
    })
    if (typeof p === 'string') setPath(p)
  }

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">방과후드림 결제관리</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> — 방과후드림에서 내려받은 결제파일을 읽어 검사합니다.
            이 단계에서는 파일을 고치지 않습니다.
          </p>
        </div>
      </div>

      {enter.data?.cleanupError && (
        <Notice tone="warn">
          지난 처리 이력을 정리하지 못했습니다({enter.data.cleanupError}). 결제파일 검사에는 영향이
          없습니다.
        </Notice>
      )}
      {enter.data?.keyStatus === 'KEY_UNAVAILABLE' && <KeyUnavailable />}

      <BasicInfo />

      <Card
        title="결제파일"
        actions={
          <Button variant="primary" onClick={() => void pick()} disabled={!settings.isSuccess}>
            파일 선택
          </Button>
        }
      >
        {!path && (
          <div className="hint">
            방과후드림에서 내려받은 결제파일(.xlsx)을 고르세요. 파일은 읽기만 하고 복사하거나 고치지
            않습니다.
          </div>
        )}
        {check.isFetching && <div className="hint">검사하는 중…</div>}
        {check.isError && <Notice tone="bad">{errorMessage(check.error)}</Notice>}
        {check.data && !check.isFetching && <FileSummary c={check.data} />}
      </Card>

      {check.data && !check.isFetching && (
        <>
          <Issues c={check.data} />
          {check.data.rows.length > 0 && <Preview c={check.data} />}
        </>
      )}
    </div>
  )
}

// ─────────────────────────────────────────────── 기본정보

function BasicInfo() {
  const app = useApp()
  const wsId = app.workspaceId!
  const toast = useToast()
  const qc = useQueryClient()
  const settings = useQuery({
    queryKey: ['dream-settings', wsId],
    queryFn: () => api.dreamSettings(wsId),
  })
  const s = settings.data

  const [org, setOrg] = useState('')
  const [yy, setYy] = useState<number | null>(null)
  const [mm, setMm] = useState<number | null>(null)
  useEffect(() => {
    if (!s) return
    setOrg(s.orgCode ?? '')
    const m = s.billingMonth ? /^(\d{4})(\d{2})$/.exec(s.billingMonth) : null
    setYy(m ? Number(m[1]) : null)
    setMm(m ? Number(m[2]) : null)
  }, [s])

  const saveOrg = useMutation({
    mutationFn: () => api.dreamSetOrgCode(org),
    onSuccess: (v) => {
      toast.ok(`기관코드를 ${v} 로 저장했습니다.`)
      void qc.invalidateQueries({ queryKey: ['dream-settings'] })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })
  const saveMonth = useMutation({
    mutationFn: () => api.dreamSetBillingMonth(wsId, monthCode(yy!, mm!)),
    onSuccess: (v) => {
      toast.ok(`결제 기준월을 ${monthLabel(v)}로 저장했습니다.`)
      void qc.invalidateQueries({ queryKey: ['dream-settings'] })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (!s) return null
  // 학년도는 3월~이듬해 2월 — 두 해만 고를 수 있게 한다
  const years = [s.year, s.year + 1]
  const monthChanged = yy !== null && mm !== null && monthCode(yy, mm) !== s.billingMonth

  return (
    <Card title="기본정보">
      <div className="toolbar" style={{ flexWrap: 'wrap', gap: 16 }}>
        <Field label="학년도">
          <div style={{ lineHeight: '30px' }}>
            <b>{s.yearName}</b> <span className="hint">(회계년도 {s.year})</span>
          </div>
        </Field>
        <Field label="작업공간">
          <div style={{ lineHeight: '30px' }}>
            <b>{s.workspaceName}</b>
          </div>
        </Field>
        <Field label="기관코드" hint={s.orgCode ? undefined : '아직 저장하지 않았습니다'}>
          <div style={{ display: 'flex', gap: 6 }}>
            <Input
              value={org}
              onChange={(e) => setOrg(e.target.value)}
              style={{ width: 130 }}
              placeholder="예: I100000987"
            />
            <Button
              onClick={() => saveOrg.mutate()}
              disabled={saveOrg.isPending || org.trim() === '' || org.trim() === (s.orgCode ?? '')}
            >
              저장
            </Button>
          </div>
        </Field>
        <Field label="결제 기준월" hint={s.billingMonth ? undefined : '결제파일 I열(수강월)과 맞대어 봅니다'}>
          <div style={{ display: 'flex', gap: 6 }}>
            <Select value={yy ?? ''} onChange={(e) => setYy(e.target.value ? Number(e.target.value) : null)} style={{ width: 96 }}>
              <option value="">연도</option>
              {years.map((y) => (
                <option key={y} value={y}>
                  {y}년
                </option>
              ))}
            </Select>
            <Select value={mm ?? ''} onChange={(e) => setMm(e.target.value ? Number(e.target.value) : null)} style={{ width: 76 }}>
              <option value="">월</option>
              {Array.from({ length: 12 }, (_, i) => i + 1).map((m) => (
                <option key={m} value={m}>
                  {m}월
                </option>
              ))}
            </Select>
            <Button onClick={() => saveMonth.mutate()} disabled={saveMonth.isPending || !monthChanged}>
              저장
            </Button>
          </div>
        </Field>
      </div>
      <div className="hint" style={{ marginTop: 6 }}>
        결제 기준월은 작업공간마다 따로 정합니다. 작업공간 기간으로 짐작하지 않습니다. 기관코드 ·
        결제 기준월을 바꿔도 정산 결과에는 영향이 없습니다.
      </div>
    </Card>
  )
}

// ─────────────────────────────────────────────── 개인정보 키

function KeyUnavailable() {
  const toast = useToast()
  const qc = useQueryClient()
  const [asking, setAsking] = useState(false)
  const [agree, setAgree] = useState(false)
  const reset = useMutation({
    mutationFn: () => api.dreamResetIdentity(),
    onSuccess: (r) => {
      toast.ok(
        `방과후드림 개인정보를 초기화했습니다(${r.result.removedIdentities}명). 초기화 직전 백업: ${r.backup}`,
      )
      setAsking(false)
      void qc.invalidateQueries({ queryKey: ['dream-enter'] })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <>
      <Notice
        tone="bad"
        actions={
          <Button small variant="danger" onClick={() => setAsking(true)}>
            개인정보 초기화
          </Button>
        }
      >
        이 PC에서는 저장된 방과후드림 개인참조번호와 생년월일을 열 수 없습니다(다른 PC나 다른
        Windows 계정에서 옮겨 온 자료). 학생 · 수강 · 정산 등 다른 기능은 그대로 쓸 수 있습니다.
      </Notice>
      {asking && (
        <Confirm
          danger
          title="방과후드림 개인정보 초기화"
          confirmText="초기화"
          busy={reset.isPending}
          disabled={!agree}
          onConfirm={() => reset.mutate()}
          onClose={() => {
            setAsking(false)
            setAgree(false)
          }}
          message={
            <>
              이 PC에서는 저장된 방과후드림 개인참조번호와 생년월일을 복호화할 수 없습니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                <b>지워지는 것(복구할 수 없음)</b>
                <br />· 모든 학생의 방과후드림 개인참조번호 · 생년월일
                <br />· 이 PC에서 열 수 없는 기존 암호 키
              </div>
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                <b>그대로인 것</b>
                <br />· 학생 · 수강 · 금액 · 정산 · 추가징수/환불
                <br />· 기관코드 · 강좌코드 연결 · 방과후드림 우선순위 · 결제 기준월
              </div>
              <div style={{ marginTop: 8 }}>
                초기화 직전에 <b>자동으로 백업</b>됩니다. 원래 PC에서는 그 백업으로 되살릴 수
                있습니다. 초기화한 뒤에는 개인참조번호 · 생년월일을 새로 등록합니다.
              </div>
              <label style={{ display: 'flex', gap: 6, marginTop: 12, alignItems: 'center' }}>
                <input type="checkbox" checked={agree} onChange={(e) => setAgree(e.target.checked)} />
                기존 개인참조번호와 생년월일을 복구할 수 없다는 것을 확인했습니다.
              </label>
            </>
          }
        />
      )}
    </>
  )
}

// ─────────────────────────────────────────────── 검사 결과

function StatusTag({ s }: { s: DreamFileCheck['status'] }) {
  const cls = s === 'OK' ? 'tag tag--free' : s === 'REVIEW' ? 'tag tag--warn' : 'tag tag--bad'
  return <span className={cls}>{statusLabel(s)}</span>
}

function FileSummary({ c }: { c: DreamFileCheck }) {
  return (
    <div className="stat">
      <div className="stat__item">
        <div className="stat__label">파일</div>
        <div className="stat__value" style={{ fontSize: 13 }}>
          {c.fileName}
        </div>
        <div className="stat__sub">{(c.fileSize / 1024).toFixed(1)} KB</div>
      </div>
      <div className="stat__item">
        <div className="stat__label">읽은 시트</div>
        <div className="stat__value" style={{ fontSize: 13 }}>
          {c.sheetName ?? '—'}
        </div>
      </div>
      <div className="stat__item">
        <div className="stat__label">실제 결제행</div>
        <div className="stat__value">{c.dataRows.toLocaleString('ko-KR')}</div>
        <div className="stat__sub">0원 행 포함</div>
      </div>
      <div className="stat__item">
        <div className="stat__label">수강월 · 회계년도</div>
        <div className="stat__value" style={{ fontSize: 13 }}>
          {monthLabel(c.billingMonth)} · {c.accountingYear ?? '—'}
        </div>
        <div className="stat__sub">
          기준월 {monthLabel(c.expectedBillingMonth)} · 학년도 {c.expectedYear}
        </div>
      </div>
      <div className="stat__item">
        <div className="stat__label">파일 검증</div>
        <div className="stat__value">
          <StatusTag s={c.status} />
        </div>
        <div className="stat__sub">
          오류 줄 {c.rowErrorCount} · 참고 {c.rowInfoCount}
        </div>
      </div>
    </div>
  )
}

function Issues({ c }: { c: DreamFileCheck }) {
  const rowIssues: DreamIssue[] = useMemo(() => c.rows.flatMap((r) => r.issues), [c])
  if (c.fileIssues.length === 0 && rowIssues.length === 0) {
    return (
      <Notice tone="info">
        파일 구조 · 필수값 · 형식 · 회계년도 · 기관코드 · 수강월 · 항목명/코드가 모두 맞습니다. 학생 ·
        강좌 매칭은 다음 단계에서 합니다.
      </Notice>
    )
  }
  const shown = rowIssues.slice(0, ISSUE_LIMIT)
  return (
    <Card title="검증 요약">
      {c.fileIssues.length > 0 && (
        <Notice tone="bad">
          <b>파일 전체 문제 {c.fileIssues.length}건</b>
          <ul style={{ margin: '6px 0 0', paddingLeft: 18 }}>
            {c.fileIssues.map((i, k) => (
              <li key={k}>{issueText(i)}</li>
            ))}
          </ul>
        </Notice>
      )}
      {rowIssues.length > 0 && (
        <div style={{ maxHeight: 260, overflow: 'auto' }}>
          <ul style={{ margin: 0, paddingLeft: 18, lineHeight: 1.8 }}>
            {shown.map((i, k) => (
              <li key={k} style={{ color: i.severity === 'ERROR' ? 'var(--red-600)' : 'var(--gray-600)' }}>
                {i.severity === 'INFO' ? '[참고] ' : ''}
                {issueText(i)}
              </li>
            ))}
          </ul>
          {rowIssues.length > ISSUE_LIMIT && (
            <div className="hint">외 {rowIssues.length - ISSUE_LIMIT}건 — 아래 미리보기에서 줄마다 볼 수 있습니다.</div>
          )}
        </div>
      )}
    </Card>
  )
}

function Preview({ c }: { c: DreamFileCheck }) {
  const [page, setPage] = useState(1)
  const [size, setSize] = useState(DEFAULT_PAGE_SIZE)
  useEffect(() => setPage(1), [c])
  const pageCount = Math.max(1, Math.ceil(c.rows.length / size))
  const rows = c.rows.slice((page - 1) * size, page * size)

  return (
    <Card title="원본 미리보기 (읽기 전용)" flush>
      <div className="hint" style={{ padding: '8px 12px' }}>
        파일에 적힌 그대로입니다. 개인참조번호 · 생년월일은 일부만 보입니다. 개인부담금 · 이용권 ·
        자유수강권은 원본 값이며, 정답으로 쓰지 않습니다.
      </div>
      <div className="tableWrap" style={{ maxHeight: 520 }}>
        <table className="table">
          <thead>
            <tr>
              <th>Excel 행</th>
              <th>상태</th>
              {DREAM_HEADERS.map((h, i) => (
                <th key={h} title={`${DREAM_LETTERS[i]}열`}>
                  {DREAM_LETTERS[i]} {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const bad = new Set(r.issues.filter((i) => i.severity === 'ERROR').map((i) => i.column))
              return (
                <tr key={r.sourceRow}>
                  <td>{r.sourceRow}</td>
                  <td>
                    <StatusTag s={r.status} />
                  </td>
                  {r.cells.map((v, i) => (
                    <td
                      key={i}
                      className={i >= 13 && i <= 16 ? 'num' : undefined}
                      style={bad.has(DREAM_LETTERS[i]) ? { background: 'var(--red-50)', color: 'var(--red-600)' } : undefined}
                    >
                      {cellText(i, v)}
                    </td>
                  ))}
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
      <div style={{ padding: 8 }}>
        <Pager
          page={page}
          pageCount={pageCount}
          total={c.rows.length}
          pageSize={size}
          onPage={setPage}
          onPageSize={(n) => {
            setSize(n)
            setPage(1)
          }}
          unit="줄"
        />
      </div>
    </Card>
  )
}
