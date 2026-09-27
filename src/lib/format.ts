/** 표시용 포맷. 금액은 어디서나 같은 규칙으로 찍는다 (요구사항 §36). */

export function won(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return ''
  return n.toLocaleString('ko-KR')
}

export function classLabel(grade: number, classNo: string): string {
  return `${grade}-${classNo}`
}

export function studentLabel(s: {
  grade: number
  classNo: string
  studentNo: number
  name: string
}): string {
  return `${s.grade}학년 ${s.classNo}반 ${s.studentNo}번 ${s.name}`
}

/**
 * 반 정렬용 열쇠.
 *
 * 학교마다 반 이름이 다르다. `1 2 3`을 쓰는 곳도 있고 `가 나 다`, `해 달 별`을
 * 쓰는 곳도 있어서 반은 문자다. 그런데 그냥 사전순으로 놓으면 `1, 10, 2, 3`이
 * 되어 숫자 반을 쓰는 학교의 쓰임새가 나빠진다.
 *
 * ```
 * 숫자 반  '1'  → "0000001"     '10' → "0000010"
 * 문자 반  '가' → "1가"
 * ```
 *
 * 앞자리 `0`/`1` 덕분에 숫자 반이 늘 문자 반보다 앞에 온다.
 *
 * **같은 규칙이 세 곳에 있다** — 여기, Rust의 `domain::class_no`,
 * 그리고 DB의 `student.class_sort` 열. 하나만 고치면 화면과 엑셀의 차례가
 * 달라지므로 함께 고쳐야 한다.
 */
export function classSortKey(classNo: string): string {
  return /^[0-9]+$/.test(classNo) ? `0${classNo.padStart(6, '0')}` : `1${classNo}`
}

/** 반 두 개의 차례를 견준다. `Array.prototype.sort`에 그대로 넘길 수 있다. */
export function compareClassNo(a: string, b: string): number {
  const ka = classSortKey(a)
  const kb = classSortKey(b)
  // 열쇠가 같은 경우('1'과 '01')까지 생각해 원래 값으로 한 번 더 가른다.
  return ka < kb ? -1 : ka > kb ? 1 : a < b ? -1 : a > b ? 1 : 0
}

export function dateRange(from: string, to: string): string {
  return `${from} ~ ${to}`
}

const PROGRAM_NAME: Record<string, string> = {
  VOUCHER: '방과후 이용권',
  FREE_VOUCHER: '자유수강권',
}

export function programName(code: string): string {
  return PROGRAM_NAME[code] ?? code
}

/** 지원유형 표시값 — 요구사항 §7의 네 가지. */
export function supportLabel(programs: string[]): {
  text: string
  tone: 'plain' | 'voucher' | 'free' | 'both'
} {
  const v = programs.includes('VOUCHER')
  const f = programs.includes('FREE_VOUCHER')
  if (v && f) return { text: '이용권+자유수강권', tone: 'both' }
  if (v) return { text: '방과후 이용권', tone: 'voucher' }
  if (f) return { text: '자유수강권', tone: 'free' }
  return { text: '일반', tone: 'plain' }
}

/** 오늘 날짜 `YYYY-MM-DD`. */
export function today(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

/** 그 달의 1일과 말일. 작업공간을 만들 때 기본값으로 쓴다. */
export function monthRange(year: number, month: number): [string, string] {
  const p = (n: number) => String(n).padStart(2, '0')
  const last = new Date(year, month, 0).getDate()
  return [`${year}-${p(month)}-01`, `${year}-${p(month)}-${p(last)}`]
}

/** 학년도(3월 시작)를 기준으로 지금이 몇 년도인지. */
export function currentSchoolYear(): number {
  const d = new Date()
  return d.getMonth() + 1 >= 3 ? d.getFullYear() : d.getFullYear() - 1
}

/** 내부 재원 코드를 사람이 읽는 말로. 화면에 코드를 그대로 쓰지 않는다. */
export function fundLabel(code: string): string {
  switch (code) {
    case 'SELF_PAY':
      return '수익자 부담금'
    case 'VOUCHER':
      return '이용권 지원'
    case 'VOUCHER_OVER':
      return '이용권 초과금'
    case 'FREE_VOUCHER':
      return '자유수강권 지원'
    default:
      return '기타'
  }
}

export function fundTone(code: string): string {
  switch (code) {
    case 'VOUCHER':
      return 'tag--voucher'
    case 'FREE_VOUCHER':
      return 'tag--free'
    case 'VOUCHER_OVER':
      return 'tag--warn'
    default:
      return 'tag--plain'
  }
}

/**
 * 요일 차례 — 가나다순이 아니라 **업무상 주간 차례**다.
 *
 * `금 · 월 · 수` 를 가나다순으로 놓으면 시간표를 읽는 눈과 어긋난다. 담당자는
 * 늘 월요일부터 보므로 `월 화 수 목 금 토 일` 로 놓는다. 내림차순은 이
 * 비교기를 뒤집은 것이라 저절로 `일 토 금 목 수 화 월` 이 된다.
 */
const DAY_ORDER = '월화수목금토일'

/**
 * 요일 칸의 글을 요일 목록으로 읽는다.
 *
 * 요일은 자유 입력 글자열이다(`월,수`). 쉼표뿐 아니라 가운뎃점·빗금·빈칸으로
 * 적는 사람도 있고 `월요일` 처럼 길게 적는 사람도 있어서, 한글이 아닌 글자를
 * 모두 구분자로 보고 토막마다 **첫 글자**만 본다. 요일이 아닌 토막은 버린다.
 */
export function parseDays(days: string): string[] {
  return days
    .split(/[^가-힣]+/)
    .filter(Boolean)
    .map((t) => t[0])
    .filter((c) => DAY_ORDER.includes(c))
}

/**
 * 요일 두 칸의 차례를 견준다. `Array.prototype.sort` 에 그대로 넘길 수 있다.
 *
 * 여러 요일이 적혀 있으면 **이른 요일부터** 견준다 — `월,수` 가 `화` 보다 앞,
 * `월` 이 `월,수` 보다 앞이다. 비어 있거나 요일로 읽히지 않는 칸은 오름차순에서
 * 맨 뒤에 둔다.
 */
export function compareDays(a: string, b: string): number {
  const ka = parseDays(a)
    .map((d) => DAY_ORDER.indexOf(d))
    .sort((x, y) => x - y)
  const kb = parseDays(b)
    .map((d) => DAY_ORDER.indexOf(d))
    .sort((x, y) => x - y)

  if (ka.length === 0 || kb.length === 0) {
    if (ka.length !== kb.length) return ka.length === 0 ? 1 : -1
    return a < b ? -1 : a > b ? 1 : 0
  }
  for (let i = 0; i < Math.min(ka.length, kb.length); i++) {
    if (ka[i] !== kb[i]) return ka[i] - kb[i]
  }
  if (ka.length !== kb.length) return ka.length - kb.length
  return a < b ? -1 : a > b ? 1 : 0
}
