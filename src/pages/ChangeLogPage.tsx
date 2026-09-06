/**
 * 변경이력 (요구사항 §16).
 *
 * 수강 추가 · 수정 · 취소 · 복원 · 금액 변경 · 부서금액 재반영을 남긴다.
 * `대상`은 그때 화면에 보이던 문구를 그대로 담은 스냅샷이라, 나중에 학생 이름이나
 * 부서명이 바뀌어도 이력은 그때의 표현을 지킨다.
 */

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'

import { DataTable, type Column } from '@/components/DataTable'
import { Button, Card, Field, Select } from '@/components/ui'
import { api } from '@/ipc/api'
import type { ChangeLog } from '@/ipc/types'
import { useApp } from '@/lib/useApp'

const KINDS = [
  { code: '', label: '전체' },
  { code: 'ENROLL_ADD', label: '수강 추가' },
  { code: 'ENROLL_EDIT', label: '수강 수정' },
  { code: 'CHARGE_EDIT', label: '금액 변경' },
  { code: 'ENROLL_CANCEL', label: '수강 취소' },
  { code: 'ENROLL_RESTORE', label: '수강 복원' },
  { code: 'DEPT_APPLY', label: '부서금액 재반영' },
]

export function ChangeLogPage() {
  const app = useApp()
  const [scope, setScope] = useState<'ws' | 'year'>('ws')
  const [kind, setKind] = useState('')

  const list = useQuery({
    queryKey: ['change-log', app.yearId, scope, app.workspaceId, kind],
    queryFn: () =>
      api.changeLogList({
        yearId: app.yearId,
        workspaceId: scope === 'ws' ? app.workspaceId : null,
        kind: kind || null,
        limit: 1000,
      }),
  })

  const columns: Column<ChangeLog>[] = [
    { key: 'at', head: '변경일시', width: 148, render: (r) => r.at },
    { key: 'ws', head: '작업공간', width: 130, render: (r) => r.workspaceName || <span className="muted">—</span> },
    {
      key: 'kind',
      head: '변경유형',
      width: 118,
      render: (r) => <span className={`tag ${tone(r.kind)}`}>{r.kindLabel}</span>,
    },
    { key: 'target', head: '대상', align: 'left', width: 260, render: (r) => r.target },
    {
      key: 'before',
      head: '이전값',
      align: 'left',
      render: (r) => r.beforeValue || <span className="muted">—</span>,
    },
    {
      key: 'after',
      head: '변경값',
      align: 'left',
      render: (r) => r.afterValue || <span className="muted">—</span>,
    },
    {
      key: 'reason',
      head: '변경사유',
      align: 'left',
      width: 170,
      render: (r) => r.reason || <span className="muted">—</span>,
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">변경 이력</h1>
          <p className="page__desc">
            수강과 금액에 손댄 기록입니다. 자료를 지워도 이력은 남습니다.
          </p>
        </div>
      </div>

      <Card flush title="이력">
        <div style={{ padding: 12 }}>
          <div className="toolbar">
            <Field label="범위">
              <Select
                value={scope}
                onChange={(e) => setScope(e.target.value as 'ws' | 'year')}
                style={{ width: 200 }}
              >
                <option value="ws">
                  현재 작업공간{app.workspace ? ` (${app.workspace.name})` : ''}
                </option>
                <option value="year">학년도 전체</option>
              </Select>
            </Field>
            <Field label="변경유형">
              <Select value={kind} onChange={(e) => setKind(e.target.value)} style={{ width: 150 }}>
                {KINDS.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Button
              onClick={() => {
                setScope('ws')
                setKind('')
              }}
            >
              초기화
            </Button>
          </div>
        </div>

        <DataTable
          rows={list.data ?? []}
          columns={columns}
          getId={(r) => r.id}
          empty={list.isLoading ? '불러오는 중…' : '기록이 없습니다.'}
        />
      </Card>
    </div>
  )
}

function tone(kind: string): string {
  switch (kind) {
    case 'ENROLL_ADD':
      return 'tag--free'
    case 'ENROLL_CANCEL':
      return 'tag--warn'
    case 'CHARGE_EDIT':
    case 'DEPT_APPLY':
      return 'tag--voucher'
    default:
      return 'tag--plain'
  }
}
