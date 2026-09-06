/**
 * 정산 결과 Excel 내려받기 (요구사항 §5).
 *
 * 기본은 **현재 작업공간의 전체 정산 결과**다. 화면 검색어 때문에 일부 학생이
 * 빠지는 일이 없어야 하기 때문이다.
 *
 * 최신 유효 정산이 아니면 버튼이 잠긴다 — 낡은 금액이 파일이 되면 되돌릴 수 없다.
 */

import { useMutation } from '@tanstack/react-query'

import { api, errorMessage } from '@/ipc/api'
import type { SettleExportKind } from '@/ipc/types'

import { useToast } from './Toast'
import { Button } from './ui'

export function ExportButton({
  kind,
  workspaceId,
  fresh,
}: {
  kind: SettleExportKind
  workspaceId: number
  /** 최신 유효 정산인가 */
  fresh: boolean
}) {
  const toast = useToast()

  const run = useMutation({
    mutationFn: () => api.settlementExport(workspaceId, kind),
    onSuccess: (r) => {
      toast.ok(`${r.name} (${r.rows}건) 을(를) 만들었습니다.`, {
        label: '폴더 열기',
        run: () => void api.openFolder('exports'),
      })
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Button
      variant="primary"
      onClick={() => run.mutate()}
      disabled={!fresh || run.isPending}
      title={
        fresh
          ? '이 작업공간의 전체 정산 결과를 내려받습니다 (화면 검색과 무관)'
          : '최신 유효 정산일 때만 내려받을 수 있습니다'
      }
    >
      {run.isPending ? '만드는 중…' : 'Excel 다운로드'}
    </Button>
  )
}
