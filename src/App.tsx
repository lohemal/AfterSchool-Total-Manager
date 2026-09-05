import { useQueryClient } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { HashRouter, Route, Routes } from 'react-router-dom'

import { AppShell } from '@/components/AppShell'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { ToastProvider } from '@/components/Toast'
import { Button } from '@/components/ui'
import { errorMessage } from '@/ipc/api'
import { AppProvider, useApp } from '@/lib/useApp'
import { DashboardPage } from '@/pages/DashboardPage'
import { DepartmentsPage } from '@/pages/DepartmentsPage'
import { EligibilityPage } from '@/pages/EligibilityPage'
import { Soon } from '@/pages/Soon'
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

          <Route
            path="roster"
            element={
              <Soon
                title="수강생 명단"
                phase="Phase 2"
                plan={[
                  '수강 데이터 Excel 업로드와 수기 추가',
                  '행 선택 → 수정 팝업 (실제 적용 금액 하나만 보여 줍니다)',
                  '수강 취소 — 자료를 지우지 않고 취소 상태로 바꾸고 사유를 받습니다',
                  '부서정보 반영 — 손으로 고친 금액은 덮지 않습니다',
                  '변경 이력 기록',
                ]}
              />
            }
          />
          <Route
            path="student-detail"
            element={
              <Soon
                title="학생 상세정보"
                phase="Phase 2"
                plan={[
                  '학년·반·번호·이름 중 하나만 넣어도 조회',
                  '작업공간별 수강·금액 내역',
                  '지원금 사용내역 · 누적 · 잔액',
                  '자격이 없는 제도는 해당없음으로 분명히 표시',
                ]}
              />
            }
          />
          <Route
            path="settlement"
            element={
              <Soon
                title="정산 데이터 생성"
                phase="Phase 3"
                plan={[
                  '버튼을 눌렀을 때만 계산합니다 (화면을 열 때마다 계산하지 않습니다)',
                  '지원기간·이월정책을 반영한 사용 가능액 계산',
                  '부서 우선순위 + 비용항목 우선순위 차감',
                  '마지막 생성 시각과 낡음 여부 표시',
                  '항목 × 재원 요약표',
                ]}
              />
            }
          />
          <Route
            path="self-pay"
            element={<Soon title="수익자" phase="Phase 3" plan={['정산 결과 조회', 'Excel 내려받기']} />}
          />
          <Route
            path="voucher"
            element={
              <Soon
                title="방과후 이용권"
                phase="Phase 3"
                plan={[
                  '사용금액 · 초과금액',
                  '연간 한도 · 지원기간 한도 · 이월액 · 이전 사용 · 잔액',
                  'Excel 내려받기',
                ]}
              />
            }
          />
          <Route
            path="free-voucher"
            element={<Soon title="자유수강권" phase="Phase 3" plan={['정산 결과 조회', 'Excel 내려받기']} />}
          />
          <Route
            path="proposal"
            element={
              <Soon
                title="품의 양식 받기"
                phase="Phase 4"
                plan={[
                  '부서 × 재원 집계 (학생 기준이 아닙니다)',
                  '강사료 · 수용비 · 교재비 · 재료비 · 교재재료비 통합',
                  '열 이름표는 대상학년 설정에서 자동으로 만듭니다',
                ]}
              />
            }
          />
          <Route
            path="policy"
            element={
              <Soon
                title="학년도 지원금 설정"
                phase="Phase 3"
                plan={[
                  '연간 지원한도',
                  '지원기간(1학기·2학기 등)과 기간별 한도',
                  '미사용 지원금 처리 — 이월 / 소멸',
                  '대상 학년(복수 지정 가능)',
                  '학생별 예외 한도',
                  '부서 · 비용항목 차감 우선순위',
                ]}
              />
            }
          />
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
