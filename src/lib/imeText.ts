/**
 * 한글 조합(IME) 중인 입력을 안전하게 다루는 규칙.
 *
 * ## 왜 필요한가
 *
 * 한글은 한 글자를 여러 번에 걸쳐 만든다. `김`을 치면 `ㄱ → 기 → 김` 으로
 * 세 번 입력 이벤트가 온다. 검색창이 이것을 그대로 부모에게 알리면
 * **아직 만들어지지도 않은 `ㄱ`, `기` 로 DB 조회가 돈다.** 결과가 틀리지는
 * 않지만 쓸데없이 세 번 돌고 화면이 깜빡인다.
 *
 * 그래서 **조합 중에는 화면만 갱신하고, 글자가 완성될 때 알린다.**
 * `ㄱ` 으로는 사람도 검색할 뜻이 없다.
 *
 * ## 왜 값을 두 벌 두는가
 *
 * 조합 중에 부모 상태를 갱신하지 않으면, 부모가 내려주는 `value` 는 잠시
 * 뒤처진다. React 가 controlled input 의 값을 되돌려 쓰면 **조합이 깨지거나
 * 글자가 사라진다.** 그래서 화면에 보여 줄 값(`draft`)을 따로 들고,
 * 밖에서 값이 바뀔 때만(초기화 등) 맞춘다 — 조합 중에는 손대지 않는다.
 *
 * `isComposing` 은 표준 `InputEvent` 의 값이라 브라우저가 알려 준다.
 * 직접 세거나 키 코드로 짐작하지 않는다.
 */

export interface TextChange {
  /** 화면(입력칸)에 둘 값 */
  draft: string
  /** 부모에게 알릴 값. `null` 이면 아직 알리지 않는다 */
  propagate: string | null
}

/** 입력 이벤트가 왔을 때. 조합 중이면 알리지 않는다. */
export function onInput(value: string, isComposing: boolean): TextChange {
  return { draft: value, propagate: isComposing ? null : value }
}

/**
 * 조합이 끝났을 때는 언제나 알린다.
 *
 * Chromium 은 `compositionend` 다음에 `input` 을 한 번 더 보내므로 보통
 * 그때 알려도 된다. 그런데 조합을 취소하거나 마우스로 다른 곳을 누르면
 * 그 `input` 이 오지 않는다 — 그러면 마지막 글자가 부모에게 가지 않는다.
 * 그래서 여기서도 알린다. 같은 값을 두 번 알리는 것은 해가 없다.
 */
export function onCompositionEnd(value: string): TextChange & { propagate: string } {
  return { draft: value, propagate: value }
}

/**
 * 밖에서 값이 바뀌었을 때 입력칸을 따라가야 하는가.
 *
 * [초기화] 를 누르면 부모가 값을 비우므로 화면도 비워야 한다.
 * **조합 중에는 따라가지 않는다** — 그때 값을 덮어쓰면 조합이 깨진다.
 */
export function shouldSync(external: string, draft: string, composing: boolean): boolean {
  return !composing && external !== draft
}
