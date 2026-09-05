/** 화면 단위 오류 울타리 — 한 화면의 오류가 앱 전체를 끄지 않게 한다 (§42-8). */

import { Component, type ErrorInfo, type ReactNode } from 'react'

import { Button } from './ui'

export class ErrorBoundary extends Component<
  { children: ReactNode; name?: string },
  { error: Error | null }
> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`화면 오류 (${this.props.name ?? '알 수 없음'})`, error, info)
  }

  render() {
    if (!this.state.error) return this.props.children
    return (
      <div className="page">
        <div className="notice notice--bad">
          <span aria-hidden>✕</span>
          <div>
            이 화면을 그리는 중 오류가 발생했습니다. 다른 화면은 그대로 쓸 수 있습니다.
            <div className="hint" style={{ marginTop: 6 }}>{this.state.error.message}</div>
          </div>
        </div>
        <Button variant="primary" onClick={() => this.setState({ error: null })}>
          다시 시도
        </Button>
      </div>
    )
  }
}
