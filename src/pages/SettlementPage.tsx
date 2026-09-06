/**
 * 정산 데이터 생성 (요구사항 §3·§15·§20).
 *
 * **화면을 열 때 계산하지 않는다.** 사람이 [정산 데이터 생성]을 눌렀을 때만
 * 실행하고, 그 결과를 저장한다. 이 화면이 보여 주는 숫자는 전부 저장된 스냅샷이다.
 *
 * 원본이나 선행 정산이 바뀌어 결과가 낡았다면 조용히 다시 계산하지 않고
 * 그 사실을 띄운다. 다시 만들지는 사람이 정한다.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Notice } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { Issue, SettleState } from '@/ipc/types'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function SettlementPage() {
  const app = useApp()
  const qc = useQueryClient()
  const toast = useToast()
  const wsId = app.workspaceId
  const [lastRun, setLastRun] = useState<string | null>(null)

  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const issues = useQuery({
    queryKey: ['settle-validate', wsId],
    queryFn: () => api.settlementValidate(wsId!),
    enabled: wsId !== null,
  })
  const summary = useQuery({
    queryKey: ['settle-summary', wsId],
    queryFn: () => api.settlementSummary(wsId!),
    enabled: wsId !== null,
  })

  const generate = useMutation({
    mutationFn: () => api.settlementGenerate(wsId!),
    onSuccess: (r) => {
      setLastRun(r.createdAt)
      toast.ok(
        `정산 데이터가 생성되었습니다. 학생 ${r.students}명 · 총 ${won(r.total)}원`,
      )
      void qc.invalidateQueries()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">정산 데이터 생성</h1>
        </div>
        <div className="card">
          <div className="card__body">
            <Empty title="작업공간이 없습니다">
              정산은 작업공간 단위로 만듭니다. [작업공간]에서 먼저 하나 만들어 주세요.
            </Empty>
          </div>
        </div>
      </div>
    )
  }

  const st = status.data
  const errors = (issues.data ?? []).filter((i) => i.level === 'ERROR')
  const warns = (issues.data ?? []).filter((i) => i.level === 'WARN')
  const sum = summary.data ?? null

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">정산 데이터 생성</h1>
          <p className="page__desc">
            <b>{app.workspace?.name}</b> ({app.workspace?.startDate} ~ {app.workspace?.endDate})
          </p>
        </div>
        <div className="page__actions">
          <Button
            variant="primary"
            onClick={() => generate.mutate()}
            disabled={generate.isPending || errors.length > 0}
          >
            {generate.isPending ? '계산 중…' : '정산 데이터 생성'}
          </Button>
        </div>
      </div>

      {st && <StatusBanner state={st.state} message={st.message} createdAt={st.createdAt} />}

      {errors.length > 0 && (
        <Notice tone="bad">
          <b>정산을 진행할 수 없습니다.</b>
          <ul style={{ margin: '6px 0 0', paddingLeft: 18 }}>
            {errors.map((e, i) => (
              <li key={i}>{e.message}</li>
            ))}
          </ul>
        </Notice>
      )}

      {warns.length > 0 && (
        <Notice tone="warn">
          <b>확인하고 진행하세요.</b>
          <ul style={{ margin: '6px 0 0', paddingLeft: 18, lineHeight: 1.8 }}>
            {warns.map((w, i) => (
              <li key={i}>{w.message}</li>
            ))}
          </ul>
        </Notice>
      )}

      {lastRun && generate.data && generate.data.warnings.length === 0 && warns.length === 0 && (
        <Notice tone="info">
          {lastRun}에 생성했습니다. 학생 {generate.data.students}명 ·{' '}
          배분 {generate.data.allocs}건 · 총 {won(generate.data.total)}원
        </Notice>
      )}

      {!sum ? (
        <Card>
          <Empty title="정산 결과가 없습니다">
            위의 [정산 데이터 생성]을 누르면 이 자리에 항목 × 재원 요약표가 나옵니다.
          </Empty>
        </Card>
      ) : (
        <SummaryTable summary={sum} />
      )}

      <Card title="정산이 하는 일">
        <ol className="hint" style={{ margin: 0, paddingLeft: 18, lineHeight: 2 }}>
          <li>이 작업공간의 <b>수강 중</b>인 자료와 금액을 모읍니다. 취소한 수강은 뺍니다.</li>
          <li>학생마다 지원기간·이월·학생별 예외를 반영해 <b>쓸 수 있는 금액</b>을 구합니다.</li>
          <li>
            <b>부서 우선순위 → 비용항목 우선순위</b>로 금액을 한 줄로 세우고 앞에서부터
            차감합니다. 한도가 항목 중간에서 끊기면 그 항목을 쪼갭니다.
          </li>
          <li>
            제도를 모두 거친 뒤 남은 금액만 학부모 부담이 됩니다. 이용권 대상자면 초과금,
            아니면 일반 수익자입니다.
          </li>
          <li>
            저장 전에 <b>배분 합계 = 원본 금액</b>을 확인합니다. 1원이라도 어긋나면 아무것도
            저장하지 않습니다.
          </li>
        </ol>
      </Card>
    </div>
  )
}

function StatusBanner({
  state,
  message,
  createdAt,
}: {
  state: SettleState
  message: string
  createdAt: string | null
}) {
  if (state === 'FRESH') {
    return (
      <Notice tone="info">
        <b>✓ 최신</b> — {createdAt} 생성
      </Notice>
    )
  }
  if (state === 'NONE') {
    return <Notice tone="info">{message}</Notice>
  }
  return (
    <Notice tone="warn">
      <b>{message}</b>
      {createdAt && (
        <div className="hint" style={{ marginTop: 4 }}>
          지금 보이는 숫자는 {createdAt}에 만든 것입니다. 다시 만들기 전까지 바뀌지 않습니다.
        </div>
      )}
    </Notice>
  )
}

function SummaryTable({ summary }: { summary: NonNullable<Awaited<ReturnType<typeof api.settlementSummary>>> }) {
  const cols: { key: keyof typeof summary.total; head: string }[] = [
    { key: 'selfPay', head: '수익자 부담금' },
    { key: 'voucher', head: '이용권 사용액' },
    { key: 'voucherOver', head: '이용권 초과금' },
    { key: 'freeVoucher', head: '자유수강권 사용액' },
  ]

  return (
    <Card flush title={`정산 요약 — ${summary.createdAt} 기준`}>
      {!summary.balanced && (
        <div style={{ padding: 12 }}>
          <Notice tone="bad">
            배분 합계({won(summary.total.total)}원)가 원본 수강금액({won(summary.chargeTotal)}원)과
            다릅니다. 다시 정산해 주세요.
          </Notice>
        </div>
      )}

      <div className="tableWrap">
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 120 }}>항목</th>
              {cols.map((c) => (
                <th key={String(c.key)} className="num" style={{ width: 150 }}>
                  {c.head}
                </th>
              ))}
              <th className="num" style={{ width: 150 }}>
                합계
              </th>
            </tr>
          </thead>
          <tbody>
            {summary.rows.map((r) => (
              <tr key={r.itemCode}>
                <td>{r.itemName}</td>
                {cols.map((c) => (
                  <td key={String(c.key)} className="num">
                    {won(r[c.key] as number)}
                  </td>
                ))}
                <td className="num">
                  <b>{won(r.total)}</b>
                </td>
              </tr>
            ))}
            <tr style={{ background: 'var(--blue-50)' }}>
              <td>
                <b>합계</b>
              </td>
              {cols.map((c) => (
                <td key={String(c.key)} className="num">
                  <b>{won(summary.total[c.key] as number)}</b>
                </td>
              ))}
              <td className="num">
                <b style={{ color: 'var(--navy-800)' }}>{won(summary.total.total)}</b>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div className="table__foot">
        <span>
          원본 수강금액 <b>{won(summary.chargeTotal)}</b>원
        </span>
        <span>
          {summary.balanced ? (
            <span style={{ color: 'var(--teal-600)' }}>✓ 배분 합계와 일치합니다</span>
          ) : (
            <span style={{ color: 'var(--red-600)' }}>✕ 합계가 어긋납니다</span>
          )}
        </span>
      </div>
    </Card>
  )
}

/** 다른 정산 화면에서 함께 쓰는 낡음 배너. */
export function SettleGuard({ children }: { children: React.ReactNode }) {
  const app = useApp()
  const wsId = app.workspaceId
  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const st = status.data

  return (
    <>
      {st && st.state !== 'FRESH' && (
        <Notice tone={st.state === 'NONE' ? 'info' : 'warn'}>
          {st.message}
          {st.state !== 'NONE' && (
            <div className="hint" style={{ marginTop: 4 }}>
              아래 숫자는 {st.createdAt}에 만든 것입니다.
            </div>
          )}
        </Notice>
      )}
      {children}
    </>
  )
}

export function issueTone(level: Issue['level']): 'bad' | 'warn' {
  return level === 'ERROR' ? 'bad' : 'warn'
}
