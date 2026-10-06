/** 왼쪽 고정 Sidebar + 상단 학년도·작업공간 표시 (요구사항 §35). */

import { useQuery } from '@tanstack/react-query'
import { NavLink, Outlet } from 'react-router-dom'

import { api } from '@/ipc/api'
import { useApp } from '@/lib/useApp'

import { Select } from './ui'

interface Item {
  to: string
  label: string
  /** 아직 만들지 않은 화면에 붙는 표시 */
  soon?: string
  /** 사람이 확인해야 할 것이 몇 건인지 알리는 표시 */
  badge?: 'adjustment'
}

const NAV: { group: string | null; items: Item[] }[] = [
  { group: null, items: [{ to: '/', label: '대시보드' }] },
  {
    group: '기초 데이터',
    items: [
      { to: '/workspaces', label: '작업공간' },
      { to: '/students', label: '학생정보' },
      { to: '/eligibility', label: '지원대상자' },
      { to: '/departments', label: '부서정보' },
    ],
  },
  {
    group: '수강 관리',
    items: [
      { to: '/roster', label: '수강생 명단' },
      { to: '/capacity', label: '부서별 수강현황' },
      { to: '/student-detail', label: '학생 상세정보' },
      { to: '/adjustments', label: '추가·취소 관리', badge: 'adjustment' },
      { to: '/changes', label: '변경 이력' },
    ],
  },
  {
    group: '정산 관리',
    items: [
      { to: '/settlement', label: '정산 데이터 생성' },
      { to: '/self-pay', label: '수익자' },
      { to: '/voucher', label: '방과후 이용권' },
      { to: '/free-voucher', label: '자유수강권' },
    ],
  },
  {
    group: '행정자료',
    items: [
      { to: '/proposal', label: '품의 양식 받기' },
      { to: '/fee-report', label: '학생별 징수 내역' },
    ],
  },
  {
    group: '시스템',
    items: [
      { to: '/policy', label: '학년도 지원금 설정' },
      { to: '/system', label: '백업·복원·업데이트' },
    ],
  },
]

export function AppShell() {
  const app = useApp()

  // 추가·취소 관리에서 확인해야 할 것이 몇 건인지. **화면 필터와 무관한 전체**다
  // — 걸러 놓은 바깥에 확인할 것이 있는데 0이면 그것을 영영 못 본다.
  const needs = useQuery({
    queryKey: ['adjustment-needs-check', app.workspaceId],
    queryFn: () => api.adjustmentNeedsCheck(app.workspaceId!),
    enabled: app.workspaceId !== null,
  })
  const needsCheck = needs.data ?? 0

  return (
    <div className="shell">
      <nav className="sidebar">
        <div className="sidebar__brand">
          방과후 통합 매니저
          <small>{app.year ? app.year.name : '학년도 없음'}</small>
        </div>

        {NAV.map((g, i) => (
          <div className="sidebar__group" key={i}>
            {g.group && <div className="sidebar__groupLabel">{g.group}</div>}
            {g.items.map((it) => (
              <NavLink
                key={it.to}
                to={it.to}
                end={it.to === '/'}
                className={({ isActive }) =>
                  isActive ? 'sidebar__link sidebar__link--on' : 'sidebar__link'
                }
              >
                {it.label}
                {it.soon && <span className="sidebar__soon">{it.soon}</span>}
                {it.badge === 'adjustment' && needsCheck > 0 && (
                  <span
                    className="tag tag--warn"
                    style={{ marginLeft: 6, padding: '0 6px' }}
                    title="금액이 달라져 확인이 필요한 내역"
                  >
                    ● {needsCheck}
                  </span>
                )}
              </NavLink>
            ))}
          </div>
        ))}
      </nav>

      <div className="main">
        <header className="topbar">
          <span className="topbar__year">{app.year ? app.year.name : '학년도를 만들어 주세요'}</span>

          {app.boot.years.length > 1 && (
            <Select
              value={app.year?.id ?? ''}
              onChange={(e) => app.setYear(Number(e.target.value))}
              style={{ width: 130 }}
              aria-label="학년도 선택"
            >
              {app.boot.years.map((y) => (
                <option key={y.id} value={y.id}>
                  {y.name}
                </option>
              ))}
            </Select>
          )}

          {app.workspaces.length > 0 ? (
            <div className="topbar__ws">
              <Select
                value={app.workspaceId ?? ''}
                onChange={(e) => app.setWorkspace(Number(e.target.value))}
                style={{ width: 170, height: 26 }}
                aria-label="작업공간 선택"
              >
                {app.workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </Select>
              {app.workspace && (
                <span className="topbar__wsDate">
                  {app.workspace.startDate} ~ {app.workspace.endDate}
                </span>
              )}
            </div>
          ) : (
            <span className="hint">작업공간이 없습니다 — [작업공간]에서 만들어 주세요.</span>
          )}

          <span className="topbar__spacer" />
        </header>

        <Outlet />
      </div>
    </div>
  )
}
