/** 표시용 포맷. 금액은 어디서나 같은 규칙으로 찍는다 (요구사항 §36). */

export function won(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return ''
  return n.toLocaleString('ko-KR')
}

export function classLabel(grade: number, classNo: number): string {
  return `${grade}-${classNo}`
}

export function studentLabel(s: {
  grade: number
  classNo: number
  studentNo: number
  name: string
}): string {
  return `${s.grade}학년 ${s.classNo}반 ${s.studentNo}번 ${s.name}`
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
