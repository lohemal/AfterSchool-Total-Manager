/**
 * 표의 줄을 두 번 눌렀을 때 수정창을 열어도 되는지 가리는 규칙.
 *
 * 줄 안의 체크상자를 두 번 눌러 선택을 껐다 켰을 뿐인데 팝업이 뜨면 놀란다.
 * 그래서 **눌린 곳이 조작 요소이거나 그 안이면 수정창을 열지 않는다.**
 *
 * 판단은 DOM 없이 되도록 태그 이름 사슬만 받는다 — 그래야 시험할 수 있다.
 */

/** 눌러도 수정창을 열지 않는 요소들. */
export const CONTROL_TAGS = new Set([
  'button',
  'a',
  'input',
  'select',
  'textarea',
  'label',
])

/**
 * 눌린 곳부터 줄까지 올라가며 모은 태그 이름 사슬에 조작 요소가 있는가.
 *
 * 대소문자를 가리지 않는다 — `tagName`은 HTML에서 대문자로 온다.
 */
export function isControlPath(tags: string[]): boolean {
  return tags.some((t) => CONTROL_TAGS.has(t.toLowerCase()))
}
