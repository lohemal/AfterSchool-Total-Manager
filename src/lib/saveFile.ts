/**
 * 업무파일을 사람이 고른 자리에 저장한다 (v0.1.5).
 *
 * 그전에는 만들어진 파일이 `%APPDATA%\…\exports\` 로 들어가서, 담당자가 그
 * 파일을 찾으려면 안내받은 경로를 따라가야 했다. 이제는 **Windows 저장 창**을
 * 띄워 원하는 자리에 바로 둔다.
 *
 * ## 만드는 서비스는 그대로 둔다
 *
 * 파일 이름은 만드는 쪽(Rust)이 자료를 보고 정한다 — 조건이 붙은 이름,
 * 학년도·작업공간이 붙은 이름이 이미 그렇게 나온다. 그 이름을 저장 창의
 * 기본값으로 쓰려면 파일을 먼저 만들어야 하므로, **만든 뒤 옮긴다.**
 * 그래서 `export_*` 명령과 그 시험은 한 줄도 바뀌지 않았다.
 *
 * 저장을 그만두면 만들어 둔 임시 파일을 지운다 — 아무것도 남지 않고, 오류도
 * 보여 주지 않는다.
 *
 * ## 앱이 스스로 관리하는 파일에는 쓰지 않는다
 *
 * 자동백업 · 복원용 안전백업 · 업데이터 파일은 앱이 제자리에 두어야 하므로
 * 이 길을 타지 않는다.
 */

import { save } from '@tauri-apps/plugin-dialog'

import { api, errorMessage } from '@/ipc/api'
import type { ExportResult } from '@/ipc/types'

/** 화면이 넘겨 주는 알림창. `Toast` 와 모양을 맞춘다. */
export interface SaveToast {
  ok: (msg: string, action?: { label: string; run: () => void }) => void
  bad: (msg: string) => void
}

function extOf(name: string): string {
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(i + 1) : 'xlsx'
}

/**
 * 파일을 만들고, 저장할 자리를 물어보고, 옮긴다.
 *
 * @param make  파일을 만드는 명령 (`api.feeReportExport` 등)
 * @param toast 결과를 알릴 곳
 * @param what  알림에 덧붙일 말 — 예: `학생 4명`
 */
export async function saveExport(
  make: () => Promise<ExportResult>,
  toast: SaveToast,
  what?: (r: ExportResult) => string,
): Promise<boolean> {
  let made: ExportResult
  try {
    made = await make()
  } catch (e) {
    toast.bad(errorMessage(e))
    return false
  }

  let target: string | null = null
  try {
    const dir = await api.exportLastDir()
    target = await save({
      defaultPath: dir ? `${dir}\\${made.name}` : made.name,
      filters: [{ name: 'Excel 파일', extensions: [extOf(made.name)] }],
    })
  } catch (e) {
    // 저장 창 자체가 열리지 않은 것은 알려 주어야 한다
    await api.exportDiscard(made.path).catch(() => undefined)
    toast.bad(errorMessage(e))
    return false
  }

  if (!target) {
    // 그만두었다 — 조용히 치운다
    await api.exportDiscard(made.path).catch(() => undefined)
    return false
  }

  try {
    const saved = await api.exportDeliver(made.path, target)
    const 덧말 = what ? ` (${what(made)})` : ''
    toast.ok(`파일을 저장했습니다${덧말}.\n${saved}`, {
      label: '폴더 열기',
      run: () => void api.revealFile(saved),
    })
    return true
  } catch (e) {
    toast.bad(errorMessage(e))
    return false
  }
}
