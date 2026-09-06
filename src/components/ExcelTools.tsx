/**
 * Excel 양식 받기 / 업로드 / 내려받기 묶음.
 *
 * 업로드는 두 단계다 — 미리보기에서 **정상 · 오류 · 경고**를 보여 주고,
 * 사용자가 확인한 뒤에야 저장한다. 오류가 있는 줄 때문에 정상 줄까지
 * 버리지 않는다 (요구사항 §28).
 */

import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'

import { api, errorMessage } from '@/ipc/api'
import type {
  EnrollmentFilter,
  ImportKind,
  ImportPreview,
  ProgramCode,
  RowIssue,
  StudentFilter,
} from '@/ipc/types'

import { Modal } from './Modal'
import { useToast } from './Toast'
import { Button } from './ui'

export function ExcelTools({
  kind,
  yearId,
  workspaceId,
  program,
  filter,
  enrollmentFilter,
  disabled,
  onDone,
}: {
  kind: ImportKind
  yearId: number
  workspaceId?: number | null
  program?: ProgramCode
  filter?: StudentFilter
  enrollmentFilter?: EnrollmentFilter
  disabled?: boolean
  onDone: () => void
}) {
  const toast = useToast()
  const [preview, setPreview] = useState<ImportPreview | null>(null)
  const [busy, setBusy] = useState(false)

  async function template() {
    try {
      const r = await api.excelTemplate(kind)
      toast.ok(`${r.name} 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    } catch (e) {
      toast.bad(errorMessage(e))
    }
  }

  async function pick() {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: 'Excel 파일', extensions: ['xlsx', 'xls', 'xlsm'] }],
      })
      if (typeof path !== 'string') return
      setBusy(true)
      const p = await api.excelPreview({ kind, path, yearId, workspaceId, program })
      setPreview(p)
    } catch (e) {
      toast.bad(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  async function commit() {
    if (!preview) return
    try {
      setBusy(true)
      const r = await api.excelCommit(preview.token, yearId, workspaceId)
      setPreview(null)
      toast.ok(`새로 ${r.added}건, 갱신 ${r.updated}건을 저장했습니다.`)
      onDone()
    } catch (e) {
      toast.bad(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  async function download() {
    try {
      const r = await api.excelExport({ kind, yearId, workspaceId, program, filter, enrollmentFilter })
      toast.ok(`${r.name} (${r.rows}건) 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    } catch (e) {
      toast.bad(errorMessage(e))
    }
  }

  async function downloadIssues(issues: RowIssue[]) {
    if (!preview) return
    try {
      const r = await api.excelExportIssues(issues, preview.headers)
      toast.ok(`${r.name} 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    } catch (e) {
      toast.bad(errorMessage(e))
    }
  }

  return (
    <>
      <Button onClick={template} disabled={disabled}>
        업로드 양식 받기
      </Button>
      <Button onClick={pick} disabled={disabled || busy}>
        {busy && !preview ? '읽는 중…' : 'Excel 업로드'}
      </Button>
      <Button onClick={download} disabled={disabled}>
        Excel 내려받기
      </Button>

      {preview && (
        <Modal
          wide
          title="업로드 확인"
          onClose={() => setPreview(null)}
          footer={
            <>
              <Button onClick={() => setPreview(null)} disabled={busy}>
                취소
              </Button>
              <Button
                variant="primary"
                onClick={commit}
                disabled={busy || preview.okCount === 0}
              >
                {busy ? '저장 중…' : `정상 ${preview.okCount}건 저장`}
              </Button>
            </>
          }
        >
          <div className="hint" style={{ marginBottom: 10 }}>
            <b>{preview.fileName}</b> — 모두 {preview.total}건을 읽었습니다.
          </div>

          <div className="stat" style={{ marginBottom: 14 }}>
            <div className="stat__item">
              <div className="stat__label">저장할 줄</div>
              <div className="stat__value">{preview.okCount.toLocaleString('ko-KR')}</div>
            </div>
            <div className="stat__item">
              <div className="stat__label">오류 (저장하지 않음)</div>
              <div className="stat__value" style={{ color: preview.errors.length ? 'var(--red-600)' : undefined }}>
                {preview.errors.length.toLocaleString('ko-KR')}
              </div>
            </div>
            <div className="stat__item">
              <div className="stat__label">경고 (저장은 됨)</div>
              <div className="stat__value" style={{ color: preview.warnings.length ? 'var(--amber-600)' : undefined }}>
                {preview.warnings.length.toLocaleString('ko-KR')}
              </div>
            </div>
          </div>

          {preview.errors.length > 0 && (
            <IssueTable
              tone="bad"
              title="이 줄은 저장하지 않습니다"
              issues={preview.errors}
              headers={preview.headers}
              onDownload={() => downloadIssues(preview.errors)}
            />
          )}

          {preview.warnings.length > 0 && (
            <IssueTable
              tone="warn"
              title="저장은 되지만 확인이 필요합니다"
              issues={preview.warnings}
              headers={preview.headers}
              onDownload={() => downloadIssues(preview.warnings)}
            />
          )}

          {preview.preview.length > 0 && (
            <>
              <div className="card__title" style={{ margin: '14px 0 6px' }}>
                미리보기 (앞 {preview.preview.length}줄)
              </div>
              <div className="tableWrap" style={{ maxHeight: 220, border: '1px solid var(--gray-200)' }}>
                <table className="table">
                  <thead>
                    <tr>
                      {preview.headers.map((h, i) => (
                        <th key={i}>{h}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {preview.preview.map((row, i) => (
                      <tr key={i}>
                        {preview.headers.map((_, c) => (
                          <td key={c}>{row[c] ?? ''}</td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </Modal>
      )}
    </>
  )
}

function IssueTable({
  tone,
  title,
  issues,
  headers,
  onDownload,
}: {
  tone: 'bad' | 'warn'
  title: string
  issues: RowIssue[]
  headers: string[]
  onDownload: () => void
}) {
  const shown = issues.slice(0, 50)
  return (
    <div style={{ marginBottom: 14 }}>
      <div className={`notice notice--${tone}`} style={{ marginBottom: 6 }}>
        <span aria-hidden>{tone === 'bad' ? '✕' : '⚠'}</span>
        <div>
          {title} — {issues.length}건
          {issues.length > shown.length && ` (아래에는 ${shown.length}건만 보입니다)`}
        </div>
        <div className="notice__actions">
          <Button small onClick={onDownload}>
            목록 내려받기
          </Button>
        </div>
      </div>
      <div className="tableWrap" style={{ maxHeight: 200, border: '1px solid var(--gray-200)' }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 48 }}>행</th>
              {headers.map((h, i) => (
                <th key={i}>{h}</th>
              ))}
              <th className="left">사유</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((it, i) => (
              <tr key={i}>
                <td className="muted">{it.row}</td>
                {headers.map((_, c) => (
                  <td key={c}>{it.cells[c] ?? ''}</td>
                ))}
                <td className="left" style={{ color: tone === 'bad' ? 'var(--red-600)' : 'var(--amber-600)' }}>
                  {it.reason}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
