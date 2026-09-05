/**
 * 팝업. **모든 팝업은 ErrorBoundary로 감싼다** — 팝업 하나가 깨져도 앱이 꺼지면
 * 안 되기 때문이다 (요구사항 §38, §42-8).
 */

import { Component, useEffect, type ErrorInfo, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

import { Button } from './ui'

class ModalBoundary extends Component<
  { children: ReactNode; onClose: () => void },
  { error: Error | null }
> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('팝업 오류', error, info)
  }

  render() {
    if (this.state.error) {
      return (
        <div className="modal__body">
          <div className="notice notice--bad">
            <span aria-hidden>✕</span>
            <div>
              이 창을 그리는 중 오류가 발생했습니다. 창을 닫고 다시 시도해 주세요.
              <div className="hint" style={{ marginTop: 6 }}>
                {this.state.error.message}
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Button onClick={this.props.onClose}>닫기</Button>
          </div>
        </div>
      )
    }
    return this.props.children
  }
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  wide,
}: {
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return createPortal(
    <div
      className="modalBack"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div className={wide ? 'modal modal--wide' : 'modal'} role="dialog" aria-modal>
        <header className="modal__head">
          <span className="modal__title">{title}</span>
          <button type="button" className="modal__x" onClick={onClose} aria-label="닫기">
            ✕
          </button>
        </header>
        <ModalBoundary onClose={onClose}>
          <div className="modal__body">{children}</div>
          {footer && <footer className="modal__foot">{footer}</footer>}
        </ModalBoundary>
      </div>
    </div>,
    document.body,
  )
}

/** 되돌릴 수 없는 작업 앞에 세우는 확인창 (요구사항 §30). */
export function Confirm({
  title,
  message,
  confirmText = '삭제',
  danger,
  onConfirm,
  onClose,
  busy,
}: {
  title: string
  message: ReactNode
  confirmText?: string
  danger?: boolean
  onConfirm: () => void
  onClose: () => void
  busy?: boolean
}) {
  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            취소
          </Button>
          <Button variant={danger ? 'danger' : 'primary'} onClick={onConfirm} disabled={busy}>
            {busy ? '처리 중…' : confirmText}
          </Button>
        </>
      }
    >
      <div style={{ lineHeight: 1.7 }}>{message}</div>
    </Modal>
  )
}
