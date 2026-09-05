/** 공용 입력·버튼. 높이와 정렬을 한곳에서 정해 화면마다 어긋나지 않게 한다. */

import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
} from 'react'

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

export function Input({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={['input', className ?? ''].filter(Boolean).join(' ')} {...rest} />
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

/** 검색창 — 비어 있을 때 회색 '검색' 안내가 보인다 (요구사항 §29). */
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
  return (
    <div className="search">
      <svg className="search__icon" viewBox="0 0 16 16" fill="none" aria-hidden>
        <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.6" />
        <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
      <input
        className="input"
        style={width ? { width } : undefined}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onValue(e.target.value)}
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
