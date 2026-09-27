/**
 * 여러 열로 동시에 정렬하는 규칙 (v0.1.6, 설계안 27장).
 *
 * 열 제목을 누르면 `없음 → 오름차순 → 내림차순 → 없음` 으로 돈다. 이미 정렬 중인
 * 열을 다시 누르면 **그 자리(우선순위)를 지킨 채** 방향만 바뀐다 — 방향을 바꿨다고
 * 1순위로 튀어 오르면 2·3순위를 다시 잡아야 한다.
 *
 * 중간 조건을 풀면 뒤 조건이 **한 칸씩 당겨진다.** 화면에 붙는 번호는 배열의
 * 자리(index)라서 저절로 다시 매겨진다.
 *
 * 화면과 떨어뜨려 두었으므로 DOM 없이 시험할 수 있다.
 */

export type SortDir = 'ASC' | 'DESC'

export interface SortSpec {
  key: string
  dir: SortDir
}

/** 동시에 쓸 수 있는 정렬 조건 수. 서버 쪽 `MAX_SORT` 와 같은 값이다. */
export const MAX_SORT = 5

export interface ToggleResult {
  sort: SortSpec[]
  /** 막혔으면 사람에게 알릴 말. 없으면 `null` */
  message: string | null
}

/**
 * 열 하나를 눌렀다.
 *
 * 이미 다섯 개가 차 있는데 **새** 열을 누르면 아무것도 바꾸지 않고 알린다 —
 * 기존 조건을 말없이 밀어내면 방금 잡아 둔 차례가 사라진다.
 */
export function toggleSort(sort: SortSpec[], key: string): ToggleResult {
  const at = sort.findIndex((s) => s.key === key)

  if (at === -1) {
    if (sort.length >= MAX_SORT) {
      return {
        sort,
        message: `정렬 기준은 최대 ${MAX_SORT}개까지 설정할 수 있습니다.`,
      }
    }
    return { sort: [...sort, { key, dir: 'ASC' }], message: null }
  }

  if (sort[at].dir === 'ASC') {
    // 자리를 지킨 채 방향만 뒤집는다
    const next = [...sort]
    next[at] = { key, dir: 'DESC' }
    return { sort: next, message: null }
  }

  // 내림차순이었으면 푼다. 뒤 조건이 한 칸씩 당겨진다.
  return { sort: sort.filter((s) => s.key !== key), message: null }
}

/** 이 열이 몇 순위인가. 정렬 중이 아니면 `null`. */
export function priorityOf(sort: SortSpec[], key: string): number | null {
  const at = sort.findIndex((s) => s.key === key)
  return at === -1 ? null : at + 1
}

export function dirOf(sort: SortSpec[], key: string): SortDir | null {
  return sort.find((s) => s.key === key)?.dir ?? null
}

/** `↑¹` · `↓²`. 정렬 중이 아니면 빈 문자열. */
export function sortMark(sort: SortSpec[], key: string): string {
  const dir = dirOf(sort, key)
  if (!dir) return ''
  const n = priorityOf(sort, key)!
  const 위첨자 = ['¹', '²', '³', '⁴', '⁵']
  return `${dir === 'ASC' ? '↑' : '↓'}${위첨자[n - 1] ?? n}`
}

/**
 * 화면에서 직접 정렬할 때 쓰는 비교기.
 *
 * `compare[key]` 는 그 열의 오름차순 비교기다. 없는 열은 건너뛴다.
 * 어느 조건으로도 가릴 수 없으면 `tieBreak` 이 차례를 정한다 — 그러지 않으면
 * 같은 값끼리 차례가 실행마다 흔들린다.
 */
export function applySort<T>(
  rows: T[],
  sort: SortSpec[],
  compare: Record<string, (a: T, b: T) => number>,
  tieBreak: (a: T, b: T) => number,
): T[] {
  if (sort.length === 0) return rows
  const out = [...rows]
  out.sort((a, b) => {
    for (const s of sort.slice(0, MAX_SORT)) {
      const cmp = compare[s.key]
      if (!cmp) continue
      const v = cmp(a, b)
      if (v !== 0) return s.dir === 'ASC' ? v : -v
    }
    return tieBreak(a, b)
  })
  return out
}
