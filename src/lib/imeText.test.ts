/**
 * 한글 조합 중 입력 규칙 시험.
 *
 * 실제 한글 입력 순서를 그대로 재현한다. `김하나`를 치면 Chromium 은
 * 이런 차례로 이벤트를 보낸다.
 *
 * ```text
 * compositionstart
 * input "ㄱ"   isComposing=true
 * input "기"   isComposing=true
 * input "김"   isComposing=true
 * compositionend "김"
 * input "김"   isComposing=false   ← 조합이 끝난 뒤 한 번 더
 * ```
 *
 * 여기서 확인하는 것은 **부모에게 몇 번, 무엇을 알리는가**다.
 * 중간 글자(`ㄱ`, `기`)로 조회가 돌면 안 된다.
 */

import { describe, expect, it } from 'vitest'

import { onCompositionEnd, onInput, shouldSync } from './imeText'

/** 입력칸 하나를 흉내 낸다 — 화면 값(draft)과 부모가 받은 값들. */
function 입력칸(처음 = '') {
  let draft = 처음
  let composing = false
  const 알림: string[] = []

  return {
    get draft() {
      return draft
    },
    get 알림() {
      return 알림
    },
    조합시작() {
      composing = true
    },
    입력(value: string) {
      const next = onInput(value, composing)
      draft = next.draft
      if (next.propagate !== null) 알림.push(next.propagate)
    },
    조합끝(value: string) {
      composing = false
      const next = onCompositionEnd(value)
      draft = next.draft
      알림.push(next.propagate)
    },
    /** 부모가 값을 바꿨을 때 (초기화 등) */
    밖에서(value: string) {
      if (shouldSync(value, draft, composing)) draft = value
    },
  }
}

describe('영문·숫자는 곧바로 알린다', () => {
  it('한 글자씩 그대로', () => {
    const i = 입력칸()
    i.입력('a')
    i.입력('ab')
    expect(i.draft).toBe('ab')
    expect(i.알림).toEqual(['a', 'ab'])
  })

  it('지우는 것도 곧바로', () => {
    const i = 입력칸('ab')
    i.입력('a')
    i.입력('')
    expect(i.알림).toEqual(['a', ''])
  })
})

describe('한글은 글자가 완성될 때 알린다', () => {
  it('김 — 중간 글자로는 알리지 않는다', () => {
    const i = 입력칸()
    i.조합시작()
    i.입력('ㄱ')
    i.입력('기')
    i.입력('김')
    // 조합 중에는 화면만 바뀐다
    expect(i.draft).toBe('김')
    expect(i.알림).toEqual([])

    i.조합끝('김')
    expect(i.알림).toEqual(['김'])
  })

  it('조합이 끝난 뒤 오는 input 도 값이 같다', () => {
    const i = 입력칸()
    i.조합시작()
    i.입력('ㄱ')
    i.입력('김')
    i.조합끝('김')
    i.입력('김') // Chromium 이 한 번 더 보낸다
    // 같은 값을 두 번 알리는 것은 해가 없다 — 조회 키가 같으면 다시 돌지 않는다
    expect(i.알림).toEqual(['김', '김'])
    expect(new Set(i.알림).size).toBe(1)
  })

  it('김하나 — 세 글자를 쳐도 알림은 세 번이다', () => {
    const i = 입력칸()
    for (const [글자, 조각들] of [
      ['김', ['ㄱ', '기', '김']],
      ['하', ['김ㅎ', '김하']],
      ['나', ['김하ㄴ', '김하나']],
    ] as [string, string[]][]) {
      i.조합시작()
      for (const 조각 of 조각들) i.입력(조각)
      i.조합끝(조각들[조각들.length - 1])
      expect(i.draft).toContain(글자)
    }
    expect(i.알림).toEqual(['김', '김하', '김하나'])
    expect(i.알림.length).toBe(3)
  })

  it('마지막 글자가 빠지지 않는다', () => {
    // 조합 도중 마우스로 다른 곳을 누르면 뒤따르는 input 이 오지 않는다.
    const i = 입력칸()
    i.조합시작()
    i.입력('ㄱ')
    i.입력('김')
    i.조합끝('김') // 여기서 끝. input 이 더 오지 않는다.
    expect(i.알림).toEqual(['김'])
    expect(i.draft).toBe('김')
  })

  it('글자가 두 번 들어가지 않는다', () => {
    const i = 입력칸()
    i.조합시작()
    i.입력('ㄱ')
    i.입력('김')
    i.조합끝('김')
    i.입력('김')
    expect(i.draft).toBe('김')
    expect(i.draft).not.toBe('김김')
  })
})

describe('밖에서 값을 바꿀 때', () => {
  it('초기화하면 입력칸도 비워진다', () => {
    const i = 입력칸()
    i.입력('김하나')
    i.밖에서('')
    expect(i.draft).toBe('')
  })

  it('조합 중에는 밖에서 덮어쓰지 않는다', () => {
    // 덮어쓰면 조합이 깨지고 글자가 사라진다.
    const i = 입력칸()
    i.조합시작()
    i.입력('기')
    i.밖에서('') // 부모 값은 아직 ''
    expect(i.draft).toBe('기')
  })

  it('같은 값이면 건드리지 않는다', () => {
    expect(shouldSync('김', '김', false)).toBe(false)
    expect(shouldSync('', '김', false)).toBe(true)
    expect(shouldSync('', '김', true)).toBe(false)
  })
})

describe('규칙 자체', () => {
  it('조합 중 입력은 알리지 않는다', () => {
    expect(onInput('기', true)).toEqual({ draft: '기', propagate: null })
  })

  it('조합이 아니면 알린다', () => {
    expect(onInput('기', false)).toEqual({ draft: '기', propagate: '기' })
  })

  it('조합 끝은 언제나 알린다', () => {
    expect(onCompositionEnd('김')).toEqual({ draft: '김', propagate: '김' })
    // 빈 값으로 끝나도(조합 취소) 알린다 — 부모가 뒤처진 값을 들고 있으면 안 된다
    expect(onCompositionEnd('')).toEqual({ draft: '', propagate: '' })
  })
})
