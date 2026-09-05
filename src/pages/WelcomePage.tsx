/** 학년도가 하나도 없을 때 — 여기서 시작한다. */

import { useState } from 'react'

import { useToast } from '@/components/Toast'
import { Button, Field, Input } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import { currentSchoolYear } from '@/lib/format'

export function WelcomePage({ onCreated }: { onCreated: () => void }) {
  const toast = useToast()
  const [year, setYear] = useState(currentSchoolYear())
  const [busy, setBusy] = useState(false)

  async function create() {
    try {
      setBusy(true)
      await api.yearCreate(year, `${year}학년도`)
      toast.ok(`${year}학년도를 만들었습니다.`)
      onCreated()
    } catch (e) {
      toast.bad(errorMessage(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div
      style={{
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--navy-900)',
      }}
    >
      <div className="card" style={{ width: 460, padding: 4 }}>
        <div className="card__body">
          <h1 style={{ margin: '0 0 6px', fontSize: 20, color: 'var(--navy-900)' }}>
            방과후 통합 매니저
          </h1>
          <p className="hint" style={{ marginBottom: 18 }}>
            수강생·수익자·방과후 이용권·자유수강권·정산·품의를 한곳에서 관리합니다.
            모든 자료는 이 컴퓨터에만 저장됩니다.
          </p>

          <Field label="학년도" hint="3월에 시작해 다음 해 2월에 끝나는 것으로 잡습니다. 나중에 고칠 수 있습니다.">
            <Input
              type="number"
              value={year}
              onChange={(e) => setYear(Number(e.target.value))}
              style={{ width: 120 }}
            />
          </Field>

          <div style={{ marginTop: 18, display: 'flex', justifyContent: 'flex-end' }}>
            <Button variant="primary" onClick={create} disabled={busy}>
              {busy ? '만드는 중…' : `${year}학년도 시작하기`}
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}
