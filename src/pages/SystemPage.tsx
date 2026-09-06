/**
 * 백업 · 복원 · 업데이트 (Phase 5).
 *
 * 이 화면이 지키는 것
 *
 * * 자료는 실행 파일과 **다른 곳**에 있다. 프로그램을 지우거나 업데이트해도
 *   `%APPDATA%\kr.school.afterschool\`는 손대지 않는다.
 * * 복원은 **지금 자료를 먼저 백업한 뒤에** 한다. 잘못된 파일을 고르면
 *   현재 자료를 건드리지 않고 멈춘다.
 * * 업데이트는 사람이 누를 때만 확인하고, 동의 없이 설치하지 않는다.
 * * 인터넷이 없어도 업무 기능은 그대로 쓸 수 있다 — 업데이트 확인만 실패한다.
 */

import { openUrl } from '@tauri-apps/plugin-opener'
import { open } from '@tauri-apps/plugin-dialog'

import { check, type Update } from '@tauri-apps/plugin-updater'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { cmp, DataTable, type Column } from '@/components/DataTable'
import { Confirm } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Card, Empty, Notice } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { BackupFile } from '@/ipc/types'
import { useApp } from '@/lib/useApp'

export function SystemPage() {
  const info = useQuery({ queryKey: ['app-info'], queryFn: api.appInfo })

  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">백업 · 복원 · 업데이트</h1>
          <p className="page__desc">
            업무 자료는 프로그램과 따로 보관됩니다. 업데이트해도 자료는 그대로 남습니다.
          </p>
        </div>
      </div>

      <AppInfoCard />
      <UpdateCard releaseUrl={info.data?.releaseUrl ?? ''} version={info.data?.version ?? ''} />
      <BackupCard />
    </div>
  )
}

// ─────────────────────────────────────────────── 앱 정보

function AppInfoCard() {
  const toast = useToast()
  const info = useQuery({ queryKey: ['app-info'], queryFn: api.appInfo })
  const d = info.data

  const openFolder = (which: 'data' | 'backups' | 'exports' | 'logs') =>
    api.openFolder(which).catch((e) => toast.bad(errorMessage(e)))

  return (
    <Card
      title="프로그램 정보"
      actions={
        <>
          <Button small onClick={() => openFolder('data')}>
            자료 폴더 열기
          </Button>
          <Button small onClick={() => openFolder('exports')}>
            Excel 폴더 열기
          </Button>
          <Button small onClick={() => openFolder('logs')}>
            로그 폴더 열기
          </Button>
        </>
      }
    >
      {!d ? (
        <div className="hint">불러오는 중…</div>
      ) : (
        <>
          <div className="stat" style={{ marginBottom: 12 }}>
            <div className="stat__item">
              <div className="stat__label">프로그램 버전</div>
              <div className="stat__value" style={{ fontSize: 20 }}>
                {d.version}
              </div>
            </div>
            <div className="stat__item">
              <div className="stat__label">자료구조 버전</div>
              <div className="stat__value" style={{ fontSize: 20 }}>
                {d.schemaVersion}
              </div>
              <div className="stat__sub">백업 호환 판단에 쓰입니다</div>
            </div>
            <div className="stat__item">
              <div className="stat__label">자료 파일 크기</div>
              <div className="stat__value" style={{ fontSize: 20 }}>
                {size(d.dbSize)}
              </div>
            </div>
          </div>

          <table className="table" style={{ border: '1px solid var(--gray-200)' }}>
            <tbody>
              {[
                ['업무 자료', d.dbPath],
                ['백업', d.backupDir],
                ['Excel 출력', d.exportDir],
                ['로그', d.logDir],
              ].map(([label, path]) => (
                <tr key={label}>
                  <td style={{ width: 120 }}>{label}</td>
                  <td className="left" style={{ fontFamily: 'var(--font-num)', fontSize: 12 }}>
                    {path}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          <div className="hint" style={{ marginTop: 10 }}>
            이 폴더는 프로그램 설치 폴더와 떨어져 있습니다. 프로그램을 업데이트하거나 다시
            설치해도 여기 있는 자료는 지워지지 않습니다.
          </div>
        </>
      )}
    </Card>
  )
}

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

// ─────────────────────────────────────────────── 업데이트

type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'none' }
  | { kind: 'found'; update: Update }
  | { kind: 'downloading'; got: number; total: number }
  | { kind: 'installing' }
  | { kind: 'failed'; message: string }

/**
 * 프로그램 업데이트.
 *
 * ## 한 번에 끝난다
 *
 * `check()`가 돌려준 `Update` 객체 하나로 `download()` → `install()`까지 이어서
 * 간다. 앱을 다시 켠 뒤 처음부터 다시 확인해야 하는 구조가 되면 안 된다.
 *
 * ## `relaunch()`를 부르지 않는다
 *
 * Windows에서 `install()`은 **돌아오지 않는다.** 플러그인이 설치 프로그램을
 * `/P /UPDATE /R`로 띄운 뒤 `std::process::exit(0)`으로 앱을 스스로 끝내고,
 * 설치가 끝나면 NSIS가 `/R` 때문에 앱을 다시 켠다. 여기서 화면이 따로
 * `relaunch()`를 부르면 **방금 뜬 설치 프로그램과 경쟁**한다. 앱이 먼저 다시
 * 켜지면 NSIS가 실행 중인 파일을 바꾸지 못해 겉으로는 "업데이트했는데 그대로"가
 * 된다. 그래서 [다시 시작] 버튼을 두지 않는다.
 *
 * ## 사람이 눌러야 시작한다
 *
 * 확인만으로 설치하지 않는다. 업무 중에 프로그램이 갑자기 닫히면 안 된다.
 */
function UpdateCard({ releaseUrl, version }: { releaseUrl: string; version: string }) {
  const toast = useToast()
  const [state, setState] = useState<UpdateState>({ kind: 'idle' })

  /** 확인은 사람이 누를 때만 한다. 실패해도 다른 기능에 영향을 주지 않는다. */
  async function checkNow() {
    setState({ kind: 'checking' })
    try {
      const update = await check()
      if (update) {
        setState({ kind: 'found', update })
      } else {
        setState({ kind: 'none' })
      }
    } catch (e) {
      setState({
        kind: 'failed',
        message: errorMessage(e),
      })
    }
  }

  /** 동의를 받은 뒤에만 내려받고 설치한다. */
  async function install(update: Update) {
    let total = 0
    let got = 0
    setState({ kind: 'downloading', got: 0, total: 0 })
    try {
      await update.download((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0
          setState({ kind: 'downloading', got: 0, total })
        } else if (event.event === 'Progress') {
          got += event.data.chunkLength
          setState({ kind: 'downloading', got, total })
        }
      })
      // 여기서 앱이 스스로 닫히고, 설치가 끝나면 다시 켜진다.
      // 아래 줄 다음은 실행되지 않는다 (Windows).
      setState({ kind: 'installing' })
      await update.install()
    } catch (e) {
      setState({ kind: 'failed', message: errorMessage(e) })
      toast.bad(errorMessage(e))
    }
  }

  return (
    <Card
      title="업데이트"
      actions={
        <>
          <Button
            variant="primary"
            small
            onClick={checkNow}
            disabled={state.kind === 'checking' || state.kind === 'downloading'}
          >
            {state.kind === 'checking' ? '확인 중…' : '업데이트 확인'}
          </Button>
          <Button small onClick={() => openUrl(releaseUrl).catch(() => {})}>
            Release 페이지 열기
          </Button>
        </>
      }
    >
      <div className="hint" style={{ marginBottom: 10 }}>
        현재 버전 <b style={{ color: 'var(--navy-800)' }}>{version}</b>
      </div>

      {state.kind === 'idle' && (
        <div className="hint">
          [업데이트 확인]을 누르면 새 버전이 있는지 확인합니다. 확인하지 않아도 프로그램을
          쓰는 데는 문제가 없습니다.
        </div>
      )}

      {state.kind === 'none' && <Notice tone="info">현재 최신 버전을 사용하고 있습니다.</Notice>}

      {state.kind === 'found' && (
        <Notice
          tone="warn"
          actions={
            <>
              <Button small variant="primary" onClick={() => install(state.update)}>
                내려받아 설치
              </Button>
              <Button small onClick={() => setState({ kind: 'idle' })}>
                나중에
              </Button>
            </>
          }
        >
          <b>새 버전 v{state.update.version}을 사용할 수 있습니다.</b>
          <div className="hint" style={{ marginTop: 4 }}>
            설치해도 업무 자료는 그대로 남습니다. 설치가 끝나면 프로그램을 다시 시작합니다.
            {state.update.date && <> · 배포일 {state.update.date.slice(0, 10)}</>}
          </div>
          {state.update.body && (
            <div className="hint" style={{ marginTop: 6, whiteSpace: 'pre-wrap' }}>
              {state.update.body}
            </div>
          )}
        </Notice>
      )}

      {state.kind === 'downloading' && (
        <Notice tone="info">
          <b>내려받는 중…</b>{' '}
          {state.total > 0 ? (
            <>
              {size(state.got)} / {size(state.total)} (
              {Math.round((state.got / state.total) * 100)}%)
            </>
          ) : (
            <>{size(state.got)}</>
          )}
          <div
            style={{
              marginTop: 6,
              height: 6,
              background: 'var(--gray-200)',
              borderRadius: 3,
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                width: state.total > 0 ? `${(state.got / state.total) * 100}%` : '30%',
                height: '100%',
                background: 'var(--navy-500)',
              }}
            />
          </div>
        </Notice>
      )}

      {state.kind === 'installing' && (
        <Notice tone="info">
          <b>설치하는 중…</b>
          <div className="hint" style={{ marginTop: 4 }}>
            프로그램이 곧 닫히고, 설치가 끝나면 새 버전으로 다시 열립니다.
            잠시 기다려 주세요.
          </div>
        </Notice>
      )}

      {state.kind === 'failed' && (
        <Notice tone="warn">
          <b>업데이트를 확인하지 못했습니다.</b>
          <div className="hint" style={{ marginTop: 4 }}>
            인터넷 연결이나 학교 방화벽 때문일 수 있습니다. 업무 기능은 그대로 쓸 수 있으니
            나중에 다시 확인하거나 [Release 페이지 열기]에서 직접 내려받으세요.
            <div style={{ marginTop: 4 }}>{state.message}</div>
          </div>
        </Notice>
      )}
    </Card>
  )
}

// ─────────────────────────────────────────────── 백업 · 복원

function BackupCard() {
  const qc = useQueryClient()
  const toast = useToast()
  const app = useApp()
  const [restoring, setRestoring] = useState<BackupFile | null>(null)
  const [removing, setRemoving] = useState<BackupFile | null>(null)

  const list = useQuery({ queryKey: ['backups'], queryFn: api.backupList })

  const create = useMutation({
    mutationFn: api.backupCreate,
    onSuccess: (f) => {
      toast.ok(`${f.name} 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('backups'),
      })
      void qc.invalidateQueries({ queryKey: ['backups'] })
      void qc.invalidateQueries({ queryKey: ['app-info'] })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const restore = useMutation({
    mutationFn: (f: BackupFile) => api.backupRestore(f.name),
    onSuccess: (r) => {
      setRestoring(null)
      toast.ok(
        r.migrated
          ? `복원했습니다. 자료구조를 ${r.fromVersion} → ${r.toVersion}으로 올렸습니다.`
          : '복원했습니다.',
      )
      // 모든 화면이 새 자료를 읽도록 캐시를 비운다
      void qc.invalidateQueries()
      app.reload()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const remove = useMutation({
    mutationFn: (f: BackupFile) => api.backupDelete(f.name),
    onSuccess: () => {
      setRemoving(null)
      toast.ok('백업 파일을 지웠습니다.')
      void qc.invalidateQueries({ queryKey: ['backups'] })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  /** 다른 PC에서 가져온 백업 — 파일을 직접 고른다. */
  const restoreFile = useMutation({
    mutationFn: async () => {
      const path = await open({
        multiple: false,
        filters: [{ name: '백업 파일', extensions: ['db'] }],
      })
      if (typeof path !== 'string') return null
      // 먼저 살펴본 뒤 사람에게 확인을 받는다
      const info = await api.backupInspect(path)
      const ok = window.confirm(
        `이 백업을 복원할까요?\n\n` +
          `학년도 ${info.yearCount}개 · 학생 ${info.studentCount}명 · ` +
          `수강 ${info.enrollmentCount}건 · 정산 ${info.settlementCount}건\n` +
          `자료구조 버전 ${info.userVersion} (현재 프로그램 ${info.appVersion})\n\n` +
          `지금 자료는 복원 직전에 자동으로 백업됩니다.`,
      )
      if (!ok) return null
      return api.backupRestoreFile(path)
    },
    onSuccess: (r) => {
      if (!r) return
      toast.ok(
        r.migrated
          ? `복원했습니다. 자료구조를 ${r.fromVersion} → ${r.toVersion}으로 올렸습니다.`
          : '복원했습니다.',
      )
      void qc.invalidateQueries()
      app.reload()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  const rows = list.data ?? []

  const columns: Column<BackupFile>[] = [
    {
      key: 'created',
      head: '만든 시각',
      width: 160,
      sort: cmp.text((f) => f.createdAt),
      render: (f) => f.createdAt || <span className="muted">—</span>,
    },
    {
      key: 'kind',
      head: '종류',
      width: 180,
      render: (f) => (
        <span className={`tag ${f.kindLabel === '수동 백업' ? 'tag--voucher' : 'tag--plain'}`}>
          {f.kindLabel}
        </span>
      ),
    },
    { key: 'name', head: '파일', align: 'left', render: (f) => f.name },
    {
      key: 'size',
      head: '크기',
      align: 'num',
      width: 90,
      sort: cmp.num((f) => f.size),
      render: (f) => size(f.size),
    },
    {
      key: 'act',
      head: '작업',
      width: 140,
      render: (f) => (
        <span style={{ display: 'inline-flex', gap: 4 }}>
          <Button small onClick={() => setRestoring(f)}>
            복원
          </Button>
          <Button small variant="danger" onClick={() => setRemoving(f)}>
            삭제
          </Button>
        </span>
      ),
    },
  ]

  return (
    <Card
      flush
      title="백업"
      actions={
        <>
          <Button
            variant="primary"
            small
            onClick={() => create.mutate()}
            disabled={create.isPending}
          >
            {create.isPending ? '만드는 중…' : '지금 백업'}
          </Button>
          <Button small onClick={() => restoreFile.mutate()} disabled={restoreFile.isPending}>
            파일에서 복원
          </Button>
          <Button small onClick={() => void api.openFolder('backups')}>
            백업 폴더 열기
          </Button>
        </>
      }
    >
      <div style={{ padding: 12 }}>
        <Notice tone="info">
          앱을 켤 때와 자료구조를 갱신할 때, 그리고 전체 삭제·복원 직전에 <b>자동으로</b>{' '}
          백업합니다. 자동 백업은 종류별로 최근 10개만 남기고,{' '}
          <b>직접 만든 백업과 안전백업은 지우지 않습니다.</b>
        </Notice>
      </div>

      <DataTable
        rows={rows}
        columns={columns}
        getId={(f) => f.name.length * 1000 + rows.indexOf(f)}
        empty={
          list.isLoading ? (
            '불러오는 중…'
          ) : (
            <Empty title="백업이 없습니다">[지금 백업]을 눌러 하나 만들어 두세요.</Empty>
          )
        }
        foot={
          <>
            <span>
              모두 <b>{rows.length}</b>개 ·{' '}
              {rows.filter((f) => f.kindLabel === '수동 백업').length}개는 직접 만든 백업
            </span>
            <span className="toolbar__spacer" />
            <span>합계 {size(rows.reduce((s, f) => s + f.size, 0))}</span>
          </>
        }
      />

      {restoring && (
        <Confirm
          title="백업 복원"
          confirmText="복원"
          message={
            <>
              <b>{restoring.name}</b> 으로 되돌립니다.
              <div style={{ marginTop: 8, lineHeight: 1.9 }}>
                · 만든 시각 {restoring.createdAt}
                <br />· 종류 {restoring.kindLabel}
              </div>
              <div style={{ marginTop: 10 }}>
                지금 자료는 <b>복원 직전에 자동으로 백업</b>됩니다. 잘못 복원했더라도 그
                백업으로 다시 되돌릴 수 있습니다.
              </div>
            </>
          }
          busy={restore.isPending}
          onConfirm={() => restore.mutate(restoring)}
          onClose={() => setRestoring(null)}
        />
      )}

      {removing && (
        <Confirm
          danger
          title="백업 파일 삭제"
          message={
            <>
              <b>{removing.name}</b> 을(를) 지웁니다. 이 백업으로는 더 이상 되돌릴 수 없습니다.
            </>
          }
          busy={remove.isPending}
          onConfirm={() => remove.mutate(removing)}
          onClose={() => setRemoving(null)}
        />
      )}
    </Card>
  )
}
