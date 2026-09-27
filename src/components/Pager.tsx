/**
 * 쪽 넘기기 (v0.1.6).
 *
 * 담당자가 **지금 어디를 보고 있는지**와 **전체가 얼마인지**를 늘 알 수 있어야
 * 한다. 그래서 현재 쪽 · 전체 쪽 · 전체 건수 · 쪽당 개수를 한 줄에 함께 둔다.
 *
 * 첫 쪽에서 [이전]·[처음], 마지막 쪽에서 [다음]·[마지막]은 잠긴다 — 눌러도
 * 아무 일이 없는 단추는 고장으로 보인다.
 */

import { Select } from './ui'
import { Button } from './ui'

/** 고를 수 있는 쪽당 개수. */
export const PAGE_SIZES = [50, 100, 200] as const
export const DEFAULT_PAGE_SIZE = 100

/** 저장된 값이 이상하면 기본값으로 돌린다. */
export function normalizePageSize(v: unknown): number {
  const n = Number(v)
  return (PAGE_SIZES as readonly number[]).includes(n) ? n : DEFAULT_PAGE_SIZE
}

export function Pager({
  page,
  pageCount,
  total,
  pageSize,
  onPage,
  onPageSize,
  unit = '건',
}: {
  page: number
  pageCount: number
  /** 필터에 걸린 전체 건수 — 지금 쪽의 줄 수가 아니다 */
  total: number
  pageSize: number
  onPage: (p: number) => void
  onPageSize: (n: number) => void
  unit?: string
}) {
  const first = page <= 1
  const last = page >= pageCount

  return (
    <div className="toolbar" style={{ marginTop: 8 }}>
      <span>
        총 <b>{total.toLocaleString('ko-KR')}</b>
        {unit}
      </span>
      <Select
        value={pageSize}
        onChange={(e) => onPageSize(Number(e.target.value))}
        style={{ width: 128 }}
        aria-label="쪽당 표시 개수"
      >
        {PAGE_SIZES.map((n) => (
          <option key={n} value={n}>
            {n}개씩 보기
          </option>
        ))}
      </Select>

      <span className="toolbar__spacer" />

      <Button small disabled={first} onClick={() => onPage(1)} title="처음 쪽">
        처음
      </Button>
      <Button small disabled={first} onClick={() => onPage(page - 1)}>
        이전
      </Button>
      <span style={{ minWidth: 88, textAlign: 'center' }}>
        <b>{page.toLocaleString('ko-KR')}</b> / {pageCount.toLocaleString('ko-KR')}
      </span>
      <Button small disabled={last} onClick={() => onPage(page + 1)}>
        다음
      </Button>
      <Button small disabled={last} onClick={() => onPage(pageCount)} title="마지막 쪽">
        마지막
      </Button>
    </div>
  )
}
