/**
 * 업무용 표. 열 제목 클릭 정렬 · 선택 · 빈 상태를 한곳에서 처리한다.
 *
 * 정렬 규칙 (요구사항 §36)
 *   · `align: 'num'` 인 열은 오른쪽 정렬 + 천 단위 구분
 *   · 그 밖의 값은 가로·세로 가운데
 */

import { useMemo, useState, type ReactNode } from 'react'
import { compareClassNo, compareDays } from '@/lib/format'
import { isControlPath } from '@/lib/rowAction'
import { applySort, sortMark, toggleSort as toggle, type SortSpec } from '@/lib/sortSpec'

export interface Column<T> {
  key: string
  head: string
  align?: 'center' | 'left' | 'num'
  width?: number
  /** 주면 열 제목을 눌러 **화면에서** 정렬할 수 있다 */
  sort?: (a: T, b: T) => number
  /**
   * 비교기 없이 정렬할 수 있는 열. 서버가 `ORDER BY` 로 정렬해 주는 표에서
   * 쓴다 — 화면은 누르기와 표시만 맡는다.
   */
  sortable?: boolean
  render: (row: T, index: number) => ReactNode
}

export function DataTable<T>({
  rows,
  columns,
  getId,
  selected,
  onSelected,
  onRowClick,
  onRowDoubleClick,
  sort,
  onSort,
  activeId,
  empty,
  maxHeight,
  foot,
}: {
  rows: T[]
  columns: Column<T>[]
  getId: (row: T) => number
  /** 주면 맨 앞에 선택 열이 생긴다 */
  selected?: number[]
  onSelected?: (ids: number[]) => void
  onRowClick?: (row: T) => void
  /**
   * 줄을 두 번 누를 때. 화면마다 **기존 [수정] 버튼과 똑같은 것**을 건다.
   * 한 번 누르는 것은 그대로 선택이다.
   */
  onRowDoubleClick?: (row: T) => void
  /**
   * 다중 정렬 (v0.1.6). 주면 열 제목을 눌러 **최대 5개**까지 쌓을 수 있고,
   * 머리글에 `↑¹ ↓²` 로 방향과 순번이 붙는다.
   *
   * 화면이 스스로 정렬한다 — `sort` 를 받고 `col.sort` 비교기가 있는 열만.
   * 서버가 정렬해 주는 표(수강생 명단)는 비교기를 주지 않으면 된다. 그러면
   * 여기서는 줄을 건드리지 않고 머리글 표시와 누르기만 맡는다.
   */
  sort?: SortSpec[]
  onSort?: (next: SortSpec[], message: string | null) => void
  activeId?: number | null
  empty?: ReactNode
  maxHeight?: number
  foot?: ReactNode
}) {
  // 다중 정렬을 쓰지 않는 표를 위한 한 열 정렬 (예전 그대로)
  const [sortKey, setSortKey] = useState<string | null>(null)
  const [desc, setDesc] = useState(false)

  const sorted = useMemo(() => {
    if (sort) {
      // 비교기가 있는 열만 화면에서 정렬한다. 하나도 없으면 서버가 이미
      // 정렬해 준 것이므로 받은 차례를 그대로 쓴다.
      const compare: Record<string, (a: T, b: T) => number> = {}
      for (const c of columns) if (c.sort) compare[c.key] = c.sort
      if (Object.keys(compare).length === 0) return rows
      return applySort(rows, sort, compare, (a, b) => getId(a) - getId(b))
    }
    const col = columns.find((c) => c.key === sortKey)
    if (!col?.sort) return rows
    const out = [...rows].sort(col.sort)
    return desc ? out.reverse() : out
  }, [rows, columns, sortKey, desc, sort, getId])

  const selectable = selected !== undefined && onSelected !== undefined
  const allIds = sorted.map(getId)
  const allChecked = selectable && allIds.length > 0 && allIds.every((id) => selected.includes(id))

  /** 열 제목을 눌렀다. 다중 정렬을 쓰는 표면 바깥에 알리고, 아니면 예전처럼. */
  function toggleSortCol(col: Column<T>) {
    if (!col.sortable && !col.sort) return
    if (sort && onSort) {
      const r = toggle(sort, col.key)
      onSort(r.sort, r.message)
      return
    }
    if (!col.sort) return
    if (sortKey === col.key) {
      setDesc((d) => !d)
    } else {
      setSortKey(col.key)
      setDesc(false)
    }
  }

  return (
    <>
      <div className="tableWrap" style={maxHeight ? { maxHeight } : undefined}>
        <table className="table">
          <thead>
            <tr>
              {selectable && (
                <th className="check">
                  <input
                    type="checkbox"
                    aria-label="전체 선택"
                    checked={allChecked}
                    onChange={(e) => onSelected(e.target.checked ? allIds : [])}
                  />
                </th>
              )}
              {columns.map((c) => (
                <th
                  key={c.key}
                  className={[
                    c.align === 'num' ? 'num' : '',
                    c.align === 'left' ? 'left' : '',
                    c.sort || c.sortable ? 'sortable' : '',
                  ]
                    .filter(Boolean)
                    .join(' ')}
                  style={c.width ? { width: c.width } : undefined}
                  onClick={() => toggleSortCol(c)}
                >
                  {c.head}
                  {sort ? (
                    sortMark(sort, c.key) && (
                      <span className="sortMark">{sortMark(sort, c.key)}</span>
                    )
                  ) : (
                    sortKey === c.key && <span className="sortMark">{desc ? '▼' : '▲'}</span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.map((row, i) => {
              const id = getId(row)
              const on = selectable ? selected.includes(id) : activeId === id
              return (
                <tr
                  key={id}
                  className={on ? 'on' : undefined}
                  onClick={() => onRowClick?.(row)}
                  onDoubleClick={(e) => {
                    if (!onRowDoubleClick) return
                    if (isControlPath(tagPath(e.target, e.currentTarget))) return
                    onRowDoubleClick(row)
                  }}
                  style={onRowClick || onRowDoubleClick ? { cursor: 'pointer' } : undefined}
                >
                  {selectable && (
                    <td
                      className="check"
                      onClick={(e) => e.stopPropagation()}
                      onDoubleClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        aria-label="선택"
                        checked={selected.includes(id)}
                        onChange={(e) =>
                          onSelected(
                            e.target.checked
                              ? [...selected, id]
                              : selected.filter((x) => x !== id),
                          )
                        }
                      />
                    </td>
                  )}
                  {columns.map((c) => (
                    <td
                      key={c.key}
                      className={[c.align === 'num' ? 'num' : '', c.align === 'left' ? 'left' : '']
                        .filter(Boolean)
                        .join(' ')}
                    >
                      {c.render(row, i)}
                    </td>
                  ))}
                </tr>
              )
            })}
            {sorted.length === 0 && (
              <tr>
                <td colSpan={columns.length + (selectable ? 1 : 0)}>
                  <div className="table__empty">{empty ?? '자료가 없습니다.'}</div>
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {foot !== undefined ? (
        <div className="table__foot">{foot}</div>
      ) : (
        <div className="table__foot">
          <span>
            모두 <b>{rows.length.toLocaleString('ko-KR')}</b>건
          </span>
          {selectable && selected.length > 0 && (
            <span>
              · 선택 <b>{selected.length.toLocaleString('ko-KR')}</b>건
            </span>
          )}
        </div>
      )}
    </>
  )
}

/**
 * 눌린 곳부터 줄까지 올라가며 태그 이름을 모은다.
 *
 * 규칙 판단은 `isControlPath`가 한다 — 그쪽은 DOM 없이 시험할 수 있다.
 */
function tagPath(from: EventTarget | null, stop: Element): string[] {
  const out: string[] = []
  let el = from instanceof Element ? from : null
  while (el && el !== stop) {
    out.push(el.tagName)
    el = el.parentElement
  }
  return out
}

/** 오름차순 비교기 몇 개 — 화면마다 다시 쓰지 않도록. */
export const cmp = {
  num: <T,>(pick: (r: T) => number) => (a: T, b: T) => pick(a) - pick(b),
  text: <T,>(pick: (r: T) => string) => (a: T, b: T) => pick(a).localeCompare(pick(b), 'ko'),
  /**
   * 반 전용. 숫자 반은 `1, 2, 3, 10` 으로, 문자 반은 `가 나 다` 로 놓는다.
   * 그냥 `text` 를 쓰면 `1, 10, 2` 가 되어 숫자 반 학교의 쓰임새가 나빠진다.
   */
  classNo: <T,>(pick: (r: T) => string) => (a: T, b: T) => compareClassNo(pick(a), pick(b)),
  /** 요일 전용. 가나다순이 아니라 `월 화 수 목 금 토 일` 차례다. */
  days: <T,>(pick: (r: T) => string) => (a: T, b: T) => compareDays(pick(a), pick(b)),
}
