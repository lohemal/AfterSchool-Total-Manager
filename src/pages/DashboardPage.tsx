/** 대시보드 — 지금 어디까지 준비되었는지와 다음에 할 일을 보여 준다. */

import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'

import { Card } from '@/components/ui'
import { api } from '@/ipc/api'
import { won } from '@/lib/format'
import { useApp } from '@/lib/useApp'

export function DashboardPage() {
  const app = useApp()

  const students = useQuery({
    queryKey: ['students-all', app.yearId],
    queryFn: () => api.studentList(app.yearId, {}),
  })
  const voucher = useQuery({
    queryKey: ['eligibility', app.yearId, 'VOUCHER', ''],
    queryFn: () => api.eligibilityList(app.yearId, 'VOUCHER'),
  })
  const free = useQuery({
    queryKey: ['eligibility', app.yearId, 'FREE_VOUCHER', ''],
    queryFn: () => api.eligibilityList(app.yearId, 'FREE_VOUCHER'),
  })
  const departments = useQuery({
    queryKey: ['departments', app.workspaceId, ''],
    queryFn: () => api.departmentList(app.workspaceId!),
    enabled: app.workspaceId !== null,
  })
  const enrollments = useQuery({
    queryKey: ['enrollments', app.workspaceId, { status: 'ACTIVE' }],
    queryFn: () => api.enrollmentList(app.workspaceId!, { status: 'ACTIVE' }),
    enabled: app.workspaceId !== null,
  })

  const steps: { done: boolean; label: string; to: string; hint: string }[] = [
    {
      done: app.workspaces.length > 0,
      label: '작업공간 만들기',
      to: '/workspaces',
      hint: '월별·분기별·기수별 등 학교가 쓰는 단위',
    },
    {
      done: (students.data?.length ?? 0) > 0,
      label: '전교생 학생정보 넣기',
      to: '/students',
      hint: '학년도에 한 번만 올리면 모든 작업공간이 함께 씁니다',
    },
    {
      done: (voucher.data?.rows.length ?? 0) + (free.data?.rows.length ?? 0) > 0,
      label: '지원대상자 명단 넣기',
      to: '/eligibility',
      hint: '방과후 이용권 · 자유수강권',
    },
    {
      done: (departments.data?.length ?? 0) > 0,
      label: '부서정보와 수강료 넣기',
      to: '/departments',
      hint: '작업공간마다 따로 넣습니다',
    },
    {
      done: (enrollments.data?.length ?? 0) > 0,
      label: '수강 등록하기',
      to: '/roster',
      hint: 'Excel 업로드 또는 수기 추가. 부서 기준금액으로 금액이 만들어집니다',
    },
  ]

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">대시보드</h1>
          <p className="page__desc">
            {app.year?.name}
            {app.workspace ? ` · ${app.workspace.name} (${app.workspace.startDate} ~ ${app.workspace.endDate})` : ''}
          </p>
        </div>
      </div>

      <div className="stat" style={{ marginBottom: 14 }}>
        <div className="stat__item">
          <div className="stat__label">전교생</div>
          <div className="stat__value">{won(students.data?.length ?? 0)}</div>
          <div className="stat__sub">학년도 전체</div>
        </div>
        <div className="stat__item">
          <div className="stat__label">방과후 이용권 대상</div>
          <div className="stat__value">{won(voucher.data?.rows.length ?? 0)}</div>
          <div className="stat__sub">
            {voucher.data?.mismatchCount ? `대상학년 확인 필요 ${voucher.data.mismatchCount}명` : ' '}
          </div>
        </div>
        <div className="stat__item">
          <div className="stat__label">자유수강권 대상</div>
          <div className="stat__value">{won(free.data?.rows.length ?? 0)}</div>
          <div className="stat__sub">&nbsp;</div>
        </div>
        <div className="stat__item">
          <div className="stat__label">부서</div>
          <div className="stat__value">{won(departments.data?.length ?? 0)}</div>
          <div className="stat__sub">{app.workspace?.name ?? '작업공간 없음'}</div>
        </div>
        <div className="stat__item">
          <div className="stat__label">수강중</div>
          <div className="stat__value">{won(enrollments.data?.length ?? 0)}</div>
          <div className="stat__sub">{app.workspace?.name ?? '작업공간 없음'}</div>
        </div>
      </div>

      <Card title="준비 순서">
        <ol style={{ margin: 0, paddingLeft: 20, lineHeight: 2.1 }}>
          {steps.map((s) => (
            <li key={s.to}>
              <span
                style={{
                  color: s.done ? 'var(--teal-600)' : 'var(--gray-400)',
                  fontWeight: 700,
                  marginRight: 6,
                }}
              >
                {s.done ? '✓' : '○'}
              </span>
              <Link to={s.to} style={{ color: 'var(--navy-600)', fontWeight: 600 }}>
                {s.label}
              </Link>
              <span className="hint" style={{ marginLeft: 8 }}>
                {s.hint}
              </span>
            </li>
          ))}
        </ol>
      </Card>

      <Card title="다음 단계">
        <div className="hint" style={{ lineHeight: 1.9 }}>
          지원금 설정과 정산 엔진은 <b>Phase 3</b>, 품의자료는 <b>Phase 4</b>에서 만듭니다.
          <br />
          지금은 수강 자료를 넣고 금액을 다듬는 단계입니다. 지원금 사용액과 잔액은 정산을
          실행해야 나오므로, 그 전까지는 화면에 표시하지 않습니다.
        </div>
      </Card>
    </div>
  )
}
