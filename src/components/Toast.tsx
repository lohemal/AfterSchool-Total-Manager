/** 저장·삭제·Excel 생성 결과를 알리는 토스트 (요구사항 §39). */

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'

type Tone = 'ok' | 'warn' | 'bad' | 'info'

interface Item {
  id: number
  tone: Tone
  text: string
  action?: { label: string; run: () => void }
}

interface Ctx {
  ok: (text: string, action?: Item['action']) => void
  warn: (text: string) => void
  bad: (text: string) => void
}

const ToastCtx = createContext<Ctx | null>(null)

export function useToast(): Ctx {
  const ctx = useContext(ToastCtx)
  if (!ctx) throw new Error('ToastProvider 안에서만 쓸 수 있습니다.')
  return ctx
}

const MARK: Record<Tone, string> = { ok: '✓', warn: '⚠', bad: '✕', info: 'ℹ' }

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Item[]>([])
  const seq = useRef(0)

  const push = useCallback((tone: Tone, text: string, action?: Item['action']) => {
    const id = ++seq.current
    setItems((prev) => [...prev, { id, tone, text, action }])
    // 오류는 사람이 읽고 닫게 두고, 나머지는 스스로 사라진다.
    if (tone !== 'bad') {
      window.setTimeout(() => setItems((prev) => prev.filter((i) => i.id !== id)), 4200)
    }
  }, [])

  const value = useMemo<Ctx>(
    () => ({
      ok: (text, action) => push('ok', text, action),
      warn: (text) => push('warn', text),
      bad: (text) => push('bad', text),
    }),
    [push],
  )

  return (
    <ToastCtx.Provider value={value}>
      {children}
      <div className="toasts">
        {items.map((i) => (
          <div key={i.id} className={`toast toast--${i.tone}`}>
            <span aria-hidden>{MARK[i.tone]}</span>
            <div>
              {i.text}
              {i.action && (
                <div style={{ marginTop: 4 }}>
                  <button
                    type="button"
                    className="btn btn--ghost btn--sm"
                    onClick={() => {
                      i.action?.run()
                      setItems((prev) => prev.filter((x) => x.id !== i.id))
                    }}
                  >
                    {i.action.label}
                  </button>
                </div>
              )}
            </div>
            <button
              type="button"
              className="toast__x"
              aria-label="닫기"
              onClick={() => setItems((prev) => prev.filter((x) => x.id !== i.id))}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  )
}
