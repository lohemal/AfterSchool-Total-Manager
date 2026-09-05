/**
 * 앱 전역 상태 — 지금 보고 있는 학년도와 작업공간.
 *
 * 두 값은 화면마다 다시 고르는 것이 아니라 상단에서 한 번 고르면 모든 화면이
 * 따라간다 (요구사항 §4).
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createContext, useContext, useMemo, type ReactNode } from 'react'

import { api } from '@/ipc/api'
import type { Bootstrap, Workspace, Year } from '@/ipc/types'

interface AppState {
  boot: Bootstrap
  year: Year | null
  yearId: number
  workspace: Workspace | null
  workspaceId: number | null
  workspaces: Workspace[]
  setYear: (id: number) => void
  setWorkspace: (id: number) => void
  reload: () => void
}

const Ctx = createContext<AppState | null>(null)

export function useApp(): AppState {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('AppProvider 안에서만 쓸 수 있습니다.')
  return ctx
}

/** 학년도가 아직 없을 때를 화면이 스스로 판단할 수 있게 별도 훅으로 둔다. */
export function useHasYear(): boolean {
  return useApp().year !== null
}

export function AppProvider({
  children,
  fallback,
}: {
  children: ReactNode
  fallback: (state: { loading: boolean; error: unknown; retry: () => void }) => ReactNode
}) {
  const qc = useQueryClient()
  const boot = useQuery({ queryKey: ['bootstrap'], queryFn: api.bootstrap })

  const setYear = useMutation({
    mutationFn: (id: number) => api.yearSetCurrent(id),
    onSuccess: () => qc.invalidateQueries(),
  })
  const setWorkspace = useMutation({
    mutationFn: (id: number) => api.workspaceSetCurrent(id),
    onSuccess: () => qc.invalidateQueries(),
  })

  const value = useMemo<AppState | null>(() => {
    if (!boot.data) return null
    const b = boot.data
    return {
      boot: b,
      year: b.currentYear,
      yearId: b.currentYear?.id ?? 0,
      workspace: b.currentWorkspace,
      workspaceId: b.currentWorkspace?.id ?? null,
      workspaces: b.workspaces,
      setYear: (id) => setYear.mutate(id),
      setWorkspace: (id) => setWorkspace.mutate(id),
      reload: () => void qc.invalidateQueries(),
    }
  }, [boot.data, qc, setYear, setWorkspace])

  if (!value) {
    return (
      <>
        {fallback({
          loading: boot.isLoading,
          error: boot.error,
          retry: () => void boot.refetch(),
        })}
      </>
    )
  }

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}
