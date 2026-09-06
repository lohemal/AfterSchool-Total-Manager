/**
 * 품의 양식 받기 (요구사항 §6~§12).
 *
 * 품의는 **학생별이 아니라 부서별 집계**다. 인원수 열은 없다.
 *
 * 화면의 미리보기와 Excel은 **같은 집계 결과**를 쓴다. 화면에서 본 금액과
 * 파일의 금액이 다를 수 없다는 뜻이다.
 *
 * 정산이 없거나 낡았으면 만들지 않는다 — 틀린 금액이 파일이 되어 학교 밖으로
 * 나가면 되돌릴 수 없다.
 */

import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'

import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Field, Notice, Select } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function ProposalPage() {
  const app = useApp()
  const toast = useToast()
  const wsId = app.workspaceId
  const [kind, setKind] = useState('INSTRUCTOR')

  const kinds = useQuery({ queryKey: ['proposal-kinds'], queryFn: api.proposalKinds })
  const status = useQuery({
    queryKey: ['settle-status', wsId],
    queryFn: () => api.settlementStatus(wsId!),
    enabled: wsId !== null,
  })
  const preview = useQuery({
    queryKey: ['proposal', wsId, kind],
    queryFn: () => api.proposalPreview(wsId!, kind),
    enabled: wsId !== null && status.data?.state === 'FRESH',
    retry: false,
  })

  const download = useMutation({
    mutationFn: () => api.proposalExport(wsId!, kind),
    onSuccess: (r) => {
      toast.ok(`${r.name} (부서 ${r.rows}줄) 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  if (wsId === null) {
    return (
      <div className="page">
        <div className="page__head">
          <h1 className="page__title">품의 양식 받기</h1>
        </div>
        <Card>
          <Empty title="작업공간이 없습니다">먼저 작업공간을 만들어 주세요.</Empty>
        </Card>
      </div>
    )
  }

  const st = status.data
  const fresh = st?.state === 'FRESH'
  const p = preview.data

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">품의 양식 받기</h1>
          <p className="page__desc">
            부서별 재원 집계입니다. 학생 이름과 인원수는 들어가지 않습니다.
          </p>
        </div>
        <div className="page__actions">
          <Button
            variant="primary"
            onClick={() => download.mutate()}
            disabled={!fresh || !p || !p.balanced || download.isPending}
          >
            {download.isPending ? '만드는 중…' : 'Excel 파일 받기'}
          </Button>
        </div>
      </div>

      <Card title="출력할 수강료 항목 선택">
        <div className="toolbar">
          <Field label="작업공간">
            <Select
              value={wsId}
              onChange={(e) => app.setWorkspace(Number(e.target.value))}
              style={{ width: 200 }}
            >
              {app.workspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="비용항목">
            <Select value={kind} onChange={(e) => setKind(e.target.value)} style={{ width: 170 }}>
              {(kinds.data ?? []).map((k) => (
                <option key={k.key} value={k.key}>
                  {k.label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="정산 상태">
            <div style={{ height: 32, display: 'flex', alignItems: 'center' }}>
              {!st && <span className="hint">확인 중…</span>}
              {st?.state === 'FRESH' && (
                <span className="tag tag--free">최신 · {st.createdAt}</span>
              )}
              {st?.state === 'NONE' && <span className="tag tag--plain">정산 전</span>}
              {st && st.state !== 'FRESH' && st.state !== 'NONE' && (
                <span className="tag tag--warn">재정산 필요</span>
              )}
            </div>
          </Field>
        </div>
        <div className="toolbar__note">
          교재·재료비를 합쳐 처리하는 학교는 <b>교재비·재료비</b> 통합을 고릅니다.
        </div>
      </Card>

      {!fresh && st && (
        <Notice tone={st.state === 'NONE' ? 'info' : 'warn'}>
          <b>{st.message}</b>
          <div className="hint" style={{ marginTop: 4 }}>
            품의 자료는 <b>최신 유효 정산</b>에서만 만듭니다. 낡은 금액이 파일이 되어 학교 밖으로
            나가면 되돌릴 수 없기 때문입니다. [정산 관리 › 정산 데이터 생성]에서 다시 만들어
            주세요.
          </div>
        </Notice>
      )}

      {fresh && preview.error && (
        <Notice tone="bad">{errorMessage(preview.error)}</Notice>
      )}

      {fresh && p && (
        <>
          {!p.balanced && (
            <Notice tone="bad">
              품의 합계({won(p.total.total)}원)가 정산 결과({won(p.settlementTotal)}원)와
              다릅니다. Excel을 만들지 않습니다. 다시 정산해 주세요.
            </Notice>
          )}

          <Card flush title={`${p.itemLabel} 품의 — ${p.workspaceName}`}>
            <div className="tableWrap">
              <table className="table">
                <thead>
                  <tr>
                    <th className="left" style={{ width: 180 }}>
                      부서명
                    </th>
                    {p.columns.map((c) => (
                      <th key={c.fund} className="num" style={{ width: 165 }}>
                        {c.label}
                      </th>
                    ))}
                    <th className="num" style={{ width: 150 }}>
                      합계
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {p.rows.map((r) => (
                    <tr key={r.departmentId}>
                      <td className="left">{r.deptLabel}</td>
                      {r.amounts.map((a, i) => (
                        <td key={i} className="num">
                          {won(a)}
                        </td>
                      ))}
                      <td className="num">
                        <b>{won(r.total)}</b>
                      </td>
                    </tr>
                  ))}
                  {p.rows.length === 0 && (
                    <tr>
                      <td colSpan={p.columns.length + 2}>
                        <div className="table__empty">
                          이 항목에 금액이 있는 부서가 없습니다.
                        </div>
                      </td>
                    </tr>
                  )}
                  {p.rows.length > 0 && (
                    <tr style={{ background: 'var(--blue-50)' }}>
                      <td className="left">
                        <b>합계</b>
                      </td>
                      {p.total.amounts.map((a, i) => (
                        <td key={i} className="num">
                          <b>{won(a)}</b>
                        </td>
                      ))}
                      <td className="num">
                        <b style={{ color: 'var(--navy-800)' }}>{won(p.total.total)}</b>
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            <div className="table__foot">
              <span>
                부서 <b>{p.rows.length}</b>개 · {p.settledAt} 정산 기준
              </span>
              <span className="toolbar__spacer" />
              <span>
                {p.balanced ? (
                  <span style={{ color: 'var(--teal-600)' }}>
                    ✓ 정산 결과({won(p.settlementTotal)}원)와 일치합니다
                  </span>
                ) : (
                  <span style={{ color: 'var(--red-600)' }}>✕ 정산 결과와 어긋납니다</span>
                )}
              </span>
            </div>
          </Card>

          <Card title="열 이름은 어떻게 정해지는가">
            <div className="hint" style={{ lineHeight: 1.9 }}>
              열 이름은 <b>학년도 지원금 설정의 대상학년</b>에서 만들어집니다. 내부적으로는
              학년을 모르는 재원 코드({p.columns.map((c) => c.fund).join(' · ')})로만 계산하고,
              이름표는 출력할 때만 붙입니다. 대상학년을 3학년에서 3·4학년으로 바꾸면 이름표가
              저절로 따라 바뀝니다.
              <div style={{ marginTop: 8 }}>
                {p.columns.map((c) => (
                  <div key={c.fund}>
                    · <b>{c.label}</b> ← {c.fund}
                  </div>
                ))}
              </div>
            </div>
          </Card>
        </>
      )}
    </div>
  )
}
