/**
 * 업무용 표. 열 제목 클릭 정렬 · 선택 · 빈 상태를 한곳에서 처리한다.
 *
 * 정렬 규칙 (요구사항 §36)
 *   · `align: 'num'` 인 열은 오른쪽 정렬 + 천 단위 구분
 *   · 그 밖의 값은 가로·세로 가운데
 */

import { useMemo, useState, type ReactNode } from 'react'

export interface Column<T> {
  key: string
  head: string
  align?: 'center' | 'left' | 'num'
  width?: number
  /** 주면 열 제목을 눌러 정렬할 수 있다 */
  sort?: (a: T, b: T) => number
  render: (row: T, index: number) => ReactNode
}

export function DataTable<T>({
  rows,
  columns,
  getId,
  selected,
  onSelected,
  onRowClick,
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
  activeId?: number | null
  empty?: ReactNode
  maxHeight?: number
  foot?: ReactNode
}) {
  const [sortKey, setSortKey] = useState<string | null>(null)
  const [desc, setDesc] = useState(false)

  const sorted = useMemo(() => {
    const col = columns.find((c) => c.key === sortKey)
    if (!col?.sort) return rows
    const out = [...rows].sort(col.sort)
    return desc ? out.reverse() : out
  }, [rows, columns, sortKey, desc])

  const selectable = selected !== undefined && onSelected !== undefined
  const allIds = sorted.map(getId)
  const allChecked = selectable && allIds.length > 0 && allIds.every((id) => selected.includes(id))

  function toggleSort(col: Column<T>) {
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
                  className={[c.align === 'num' ? 'num' : '', c.align === 'left' ? 'left' : '', c.sort ? 'sortable' : '']
                    .filter(Boolean)
                    .join(' ')}
                  style={c.width ? { width: c.width } : undefined}
                  onClick={() => toggleSort(c)}
                >
                  {c.head}
                  {sortKey === c.key && <span className="sortMark">{desc ? '▼' : '▲'}</span>}
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
                  style={onRowClick ? { cursor: 'pointer' } : undefined}
                >
                  {selectable && (
                    <td className="check" onClick={(e) => e.stopPropagation()}>
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

/** 오름차순 비교기 몇 개 — 화면마다 다시 쓰지 않도록. */
export const cmp = {
  num: <T,>(pick: (r: T) => number) => (a: T, b: T) => pick(a) - pick(b),
  text: <T,>(pick: (r: T) => string) => (a: T, b: T) => pick(a).localeCompare(pick(b), 'ko'),
}
