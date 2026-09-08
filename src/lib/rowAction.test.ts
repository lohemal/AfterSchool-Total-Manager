/**
 * 줄 더블클릭이 수정창을 열어야 하는지 가리는 규칙 시험.
 *
 * `tagPath`가 모아 주는 사슬은 **눌린 곳부터 줄(`<tr>`)까지**다. 줄 자신은
 * 들어가지 않는다.
 */

import { describe, expect, it } from 'vitest'

import { isControlPath } from './rowAction'

describe('보통 칸을 두 번 누르면 수정창을 연다', () => {
  it('글자만 있는 칸', () => {
    expect(isControlPath(['TD'])).toBe(false)
  })

  it('굵게·꼬리표가 들어 있는 칸', () => {
    expect(isControlPath(['B', 'TD'])).toBe(false)
    expect(isControlPath(['SPAN', 'TD'])).toBe(false)
  })

  it('빈 사슬 — 줄 자체를 눌렀을 때', () => {
    expect(isControlPath([])).toBe(false)
  })
})

describe('조작 요소를 두 번 누르면 열지 않는다', () => {
  it('체크상자', () => {
    // 선택을 껐다 켰을 뿐인데 팝업이 뜨면 안 된다
    expect(isControlPath(['INPUT', 'TD'])).toBe(true)
  })

  it('버튼과 그 안의 글자', () => {
    expect(isControlPath(['BUTTON', 'TD'])).toBe(true)
    expect(isControlPath(['SPAN', 'BUTTON', 'TD'])).toBe(true)
  })

  it('링크 · 고르기 · 여러줄 입력 · 이름표', () => {
    expect(isControlPath(['A', 'TD'])).toBe(true)
    expect(isControlPath(['SELECT', 'TD'])).toBe(true)
    expect(isControlPath(['TEXTAREA', 'TD'])).toBe(true)
    expect(isControlPath(['LABEL', 'TD'])).toBe(true)
  })

  it('사슬 어디에 있어도 막는다', () => {
    expect(isControlPath(['TD', 'INPUT'])).toBe(true)
  })
})

describe('태그 이름 대소문자를 가리지 않는다', () => {
  it('HTML 은 대문자, SVG 안은 소문자로 올 수 있다', () => {
    expect(isControlPath(['input'])).toBe(true)
    expect(isControlPath(['Button'])).toBe(true)
    expect(isControlPath(['td'])).toBe(false)
  })
})

describe('비슷하지만 조작 요소가 아닌 것', () => {
  it('표·칸·줄은 막지 않는다', () => {
    for (const t of ['TABLE', 'TBODY', 'TR', 'TD', 'TH', 'DIV', 'P']) {
      expect(isControlPath([t])).toBe(false)
    }
  })
})
