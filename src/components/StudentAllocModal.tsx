/**
 * 학생 한 명의 부서별 정산 상세 (요구사항 §3·§4).
 *
 * 어느 부서의 어느 항목이 어떤 재원으로 갔는지 그대로 보여 준다.
 * `PLAIN` 같은 내부 코드는 화면에 쓰지 않고 한글 문구로 바꾼다.
 */

import { useQuery } from '@tanstack/react-query'

import { api } from '@/ipc/api'
import { fundTone, won } from '@/lib/format'

import { Modal } from './Modal'
import { Button, Empty } from './ui'

export function StudentAllocModal({
  workspaceId,
  studentId,
  title,
  onClose,
}: {
  workspaceId: number
  studentId: number
  title: string
  onClose: () => void
}) {
  const rows = useQuery({
    queryKey: ['student-allocs', workspaceId, studentId],
    queryFn: () => api.settlementStudentAllocs(workspaceId, studentId),
  })
  const list = rows.data ?? []
  const total = list.reduce((s, r) => s + r.amount, 0)

  return (
    <Modal
      wide
      title={`정산 상세 — ${title}`}
      onClose={onClose}
      footer={
        <>
          <span className="hint" style={{ marginRight: 'auto' }}>
            모두 <b>{won(total)}</b>원
          </span>
          <Button onClick={onClose}>닫기</Button>
        </>
      }
    >
      {rows.isLoading && <div className="hint">불러오는 중…</div>}

      {!rows.isLoading && list.length === 0 && (
        <Empty title="정산 내역이 없습니다">
          이 학생은 이 작업공간의 최신 정산에 들어 있지 않습니다.
        </Empty>
      )}

      {list.length > 0 && (
        <div className="tableWrap" style={{ border: '1px solid var(--gray-200)', maxHeight: 400 }}>
          <table className="table">
            <thead>
              <tr>
                <th className="left" style={{ width: 160 }}>
                  부서
                </th>
                <th style={{ width: 90 }}>항목</th>
                <th style={{ width: 130 }}>재원</th>
                <th className="num" style={{ width: 110 }}>
                  금액
                </th>
                <th className="left">발생원인</th>
              </tr>
            </thead>
            <tbody>
              {list.map((r, i) => (
                <tr key={i}>
                  <td className="left">{r.deptLabel}</td>
                  <td>{r.itemName}</td>
                  <td>
                    <span className={`tag ${fundTone(r.fund)}`}>{r.fundLabel}</span>
                  </td>
                  <td className="num">
                    <b>{won(r.amount)}</b>
                  </td>
                  <td className="left">
                    {r.fund === 'SELF_PAY' || r.fund === 'VOUCHER_OVER' ? (
                      r.originLabel
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="hint" style={{ marginTop: 10 }}>
        한 항목이 두 재원으로 나뉘어 있으면, 그 항목의 금액이 지원금 한도에서 끊긴 것입니다.
      </div>
    </Modal>
  )
}
