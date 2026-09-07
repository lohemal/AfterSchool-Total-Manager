/** 왼쪽 고정 Sidebar + 상단 학년도·작업공간 표시 (요구사항 §35). */

import { NavLink, Outlet } from 'react-router-dom'

import { useApp } from '@/lib/useApp'

import { Select } from './ui'

interface Item {
  to: string
  label: string
  /** 아직 만들지 않은 화면에 붙는 표시 */
  soon?: string
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
      { to: '/student-detail', label: '학생 상세정보' },
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
