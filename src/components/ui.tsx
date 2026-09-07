/** 공용 입력·버튼. 높이와 정렬을 한곳에서 정해 화면마다 어긋나지 않게 한다. */

import { useEffect, useRef, useState } from 'react'
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
} from 'react'

import { onCompositionEnd, onInput, shouldSync } from '@/lib/imeText'

type Variant = 'primary' | 'default' | 'ghost' | 'danger'

export function Button({
  variant = 'default',
  small,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; small?: boolean }) {
  const cls = ['btn', `btn--${variant}`, small ? 'btn--sm' : '', className ?? '']
    .filter(Boolean)
    .join(' ')
  return (
    <button type="button" className={cls} {...rest}>
      {children}
    </button>
  )
}

export function Field({
  label,
  children,
  hint,
  className,
}: {
  label?: string
  children: ReactNode
  hint?: string
  className?: string
}) {
  return (
    <div className={['field', className ?? ''].filter(Boolean).join(' ')}>
      {label && <span className="field__label">{label}</span>}
      {children}
      {hint && <span className="hint">{hint}</span>}
    </div>
  )
}

/**
 * 공용 텍스트 입력칸.
 *
 * ## 한글을 치는 칸이라는 전제
 *
 * 학교 업무에서 자유 입력값은 거의 모두 한글이다(이름·부서명·사유·비고).
 * 그래서 한글에 방해가 되는 브라우저 기능을 끈다.
 *
 * * `spellCheck={false}` — Chromium 에 한글 사전이 없어 이름마다 빨간 밑줄이
 *   그어진다. 고칠 것이 없는데 틀린 것처럼 보인다.
 * * `autoComplete="off"` — 자동완성 목록이 입력칸을 덮어 조합 중인 글자를 가린다.
 * * `autoCapitalize`/`autoCorrect` — 모바일 기본값이 한글 입력을 건드리지 않게.
 *
 * **한/영 상태를 한글로 바꾸는 웹 표준은 없다.** `lang`·`inputmode` 는 언어
 * 힌트일 뿐이고 `ime-mode` 는 Blink 에 없다. 실제 앱에서 재어 확인했다
 * (설계안 24장). 그래서 여기서 IME 를 강제하지 않는다.
 *
 * 넘긴 속성이 뒤에 오므로 필요하면 화면에서 덮어쓸 수 있다.
 */
export function Input({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={['input', className ?? ''].filter(Boolean).join(' ')}
      spellCheck={false}
      autoComplete="off"
      autoCapitalize="off"
      autoCorrect="off"
      {...rest}
    />
  )
}

/**
 * 숫자만 넣는 칸 (학년 · 번호 · 금액 · 한도 · 우선순위).
 *
 * `inputMode="numeric"` 으로 **숫자 칸이라는 뜻을 명시한다.** 이것은 IME 를
 * 바꾸지 않는다 — 터치 자판에서 숫자판이 먼저 뜨게 하는 힌트다. 한/영 상태를
 * 바꾸는 웹 표준은 없다 (설계안 24장).
 *
 * `Input` 과 달리 한글 관련 속성을 붙이지 않는다. 여기 한글을 넣을 일이 없다.
 */
export function NumInput({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={['input', 'input--num', className ?? ''].filter(Boolean).join(' ')}
      inputMode="numeric"
      autoComplete="off"
      {...rest}
    />
  )
}

/** 금액 입력 — 오른쪽 정렬, 숫자만 받는다. */
export function MoneyInput({
  value,
  onValue,
  ...rest
}: Omit<InputHTMLAttributes<HTMLInputElement>, 'value' | 'onChange'> & {
  value: number
  onValue: (n: number) => void
}) {
  return (
    <input
      className="input input--num"
      inputMode="numeric"
      value={value === 0 ? '' : value.toLocaleString('ko-KR')}
      placeholder="0"
      onChange={(e) => {
        const digits = e.target.value.replace(/[^0-9]/g, '')
        onValue(digits === '' ? 0 : Number(digits))
      }}
      {...rest}
    />
  )
}

export function Select({
  className,
  children,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={['select', className ?? ''].filter(Boolean).join(' ')} {...rest}>
      {children}
    </select>
  )
}

/**
 * 검색창 — 비어 있을 때 회색 '검색' 안내가 보인다 (요구사항 §29).
 *
 * ## 한글이 완성될 때 검색한다
 *
 * `김`을 치면 `ㄱ → 기 → 김` 으로 입력 이벤트가 세 번 온다. 그대로 알리면
 * 아직 만들어지지도 않은 `ㄱ`, `기` 로 조회가 돈다. 그래서 **조합 중에는
 * 화면만 갱신하고 글자가 완성될 때 알린다** (`lib/imeText`).
 *
 * 그 동안 부모 값이 잠시 뒤처지므로 화면용 값을 따로 든다 — controlled input
 * 의 값을 조합 중에 되돌려 쓰면 글자가 깨지거나 사라진다.
 */
export function Search({
  value,
  onValue,
  placeholder = '검색',
  width,
}: {
  value: string
  onValue: (v: string) => void
  placeholder?: string
  width?: number
}) {
  const [draft, setDraft] = useState(value)
  const composing = useRef(false)

  // 밖에서 값이 바뀌면([초기화] 등) 입력칸도 따라간다. 조합 중에는 손대지 않는다.
  useEffect(() => {
    if (shouldSync(value, draft, composing.current)) setDraft(value)
    // draft 를 의존성에 넣으면 타이핑마다 부모 값으로 되돌아간다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value])

  return (
    <div className="search">
      <svg className="search__icon" viewBox="0 0 16 16" fill="none" aria-hidden>
        <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.6" />
        <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
      <input
        className="input"
        style={width ? { width } : undefined}
        value={draft}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(e) => {
          const next = onInput(e.target.value, (e.nativeEvent as InputEvent).isComposing)
          setDraft(next.draft)
          if (next.propagate !== null) onValue(next.propagate)
        }}
        onCompositionStart={() => {
          composing.current = true
        }}
        onCompositionEnd={(e) => {
          composing.current = false
          const next = onCompositionEnd(e.currentTarget.value)
          setDraft(next.draft)
          onValue(next.propagate)
        }}
      />
    </div>
  )
}

export function Notice({
  tone = 'info',
  children,
  actions,
}: {
  tone?: 'info' | 'warn' | 'bad'
  children: ReactNode
  actions?: ReactNode
}) {
  const mark = tone === 'bad' ? '✕' : tone === 'warn' ? '⚠' : 'ℹ'
  return (
    <div className={`notice notice--${tone}`}>
      <span aria-hidden>{mark}</span>
      <div>{children}</div>
      {actions && <div className="notice__actions">{actions}</div>}
    </div>
  )
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty__title">{title}</div>
      {children && <div className="hint">{children}</div>}
    </div>
  )
}

export function Card({
  title,
  actions,
  children,
  flush,
}: {
  title?: string
  actions?: ReactNode
  children: ReactNode
  flush?: boolean
}) {
  return (
    <section className="card">
      {(title || actions) && (
        <header className="card__head">
          {title && <span className="card__title">{title}</span>}
          {actions && <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>{actions}</div>}
        </header>
      )}
      <div className={flush ? 'card__body card__body--flush' : 'card__body'}>{children}</div>
    </section>
  )
}
