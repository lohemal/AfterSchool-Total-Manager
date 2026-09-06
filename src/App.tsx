import { useQueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { HashRouter, Route, Routes } from 'react-router-dom'

import { AppShell } from '@/components/AppShell'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { ToastProvider } from '@/components/Toast'
import { Button } from '@/components/ui'
import { errorMessage } from '@/ipc/api'
import { AppProvider, useApp } from '@/lib/useApp'
import { ChangeLogPage } from '@/pages/ChangeLogPage'
import { DashboardPage } from '@/pages/DashboardPage'
import { DepartmentsPage } from '@/pages/DepartmentsPage'
import { EligibilityPage } from '@/pages/EligibilityPage'
import { PolicyPage } from '@/pages/PolicyPage'
import { FreeVoucherPage, VoucherPage } from '@/pages/ProgramResultPage'
import { ProposalPage } from '@/pages/ProposalPage'
import { RosterPage } from '@/pages/RosterPage'
import { SelfPayPage } from '@/pages/SelfPayPage'
import { SettlementPage } from '@/pages/SettlementPage'
import { Soon } from '@/pages/Soon'
import { StudentDetailPage } from '@/pages/StudentDetailPage'
import { StudentsPage } from '@/pages/StudentsPage'
import { WelcomePage } from '@/pages/WelcomePage'
import { WorkspacesPage } from '@/pages/WorkspacesPage'

export default function App() {
  return (
    <ToastProvider>
      <ErrorBoundary name="앱">
        <Boot />
      </ErrorBoundary>
    </ToastProvider>
  )
}

function Boot() {
  const qc = useQueryClient()

  return (
    <AppProvider
      fallback={({ loading, error, retry }) => {
        if (loading) {
          return (
            <div style={{ display: 'grid', placeItems: 'center', height: '100%' }}>
              <span className="spin" />
            </div>
          )
        }
        if (error) {
          return (
            <div style={{ display: 'grid', placeItems: 'center', height: '100%' }}>
              <div className="card" style={{ padding: 20, maxWidth: 420 }}>
                <div className="notice notice--bad">
                  <span aria-hidden>✕</span>
                  <div>{errorMessage(error)}</div>
                </div>
                <Button variant="primary" onClick={retry}>
                  다시 시도
                </Button>
              </div>
            </div>
          )
        }
        return null
      }}
    >
      <Shell onYearCreated={() => void qc.invalidateQueries()} />
    </AppProvider>
  )
}

function Shell({ onYearCreated }: { onYearCreated: () => void }) {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Guard>{<DashboardPage />}</Guard>} />
          <Route path="workspaces" element={<Guard>{<WorkspacesPage />}</Guard>} />
          <Route path="students" element={<Guard>{<StudentsPage />}</Guard>} />
          <Route path="eligibility" element={<Guard>{<EligibilityPage />}</Guard>} />
          <Route path="departments" element={<Guard>{<DepartmentsPage />}</Guard>} />

          <Route path="roster" element={<Guard>{<RosterPage />}</Guard>} />
          <Route path="student-detail" element={<Guard>{<StudentDetailPage />}</Guard>} />
          <Route path="changes" element={<Guard>{<ChangeLogPage />}</Guard>} />
          <Route path="settlement" element={<Guard>{<SettlementPage />}</Guard>} />
          <Route path="self-pay" element={<Guard>{<SelfPayPage />}</Guard>} />
          <Route path="voucher" element={<Guard>{<VoucherPage />}</Guard>} />
          <Route path="free-voucher" element={<Guard>{<FreeVoucherPage />}</Guard>} />
          <Route path="policy" element={<Guard>{<PolicyPage />}</Guard>} />
          <Route path="proposal" element={<Guard>{<ProposalPage />}</Guard>} />
          <Route
            path="system"
            element={
              <Soon
                title="백업 · 복원 · 업데이트"
                phase="Phase 5"
                plan={['전체 백업과 복원', '업데이트 확인', '오류 로그']}
              />
            }
          />
        </Route>
      </Routes>
      <YearGate onCreated={onYearCreated} />
    </HashRouter>
  )
}

/** 학년도가 없으면 화면 대신 시작 화면을 덮어씌운다. */
function YearGate({ onCreated }: { onCreated: () => void }) {
  const app = useApp()
  if (app.year) return null
  return (
    <div style={{ position: 'fixed', inset: 0, zIndex: 30 }}>
      <WelcomePage onCreated={onCreated} />
    </div>
  )
}

function Guard({ children }: { children: ReactNode }) {
  return <ErrorBoundary name="화면">{children}</ErrorBoundary>
}
