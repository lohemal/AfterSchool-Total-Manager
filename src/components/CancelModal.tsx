/**
 * 수강 취소 창 (v0.1.3, 최종 QA).
 *
 * **취소는 전액 0원이 아니다.** 중도에 그만두어도 이미 발생한 비용은 징수한다.
 * 교재비·재료비는 배부한 뒤라면 환불하지 않고, 강사료·수용비는 실제 수강한
 * 만큼 받는다. 그래서 취소할 때 항목별 **최종 징수금액**을 확인·수정한다.
 *
 * 기본값은 **지금 금액 그대로**다. 0을 기본값으로 하면 환불하지 않는 교재비를
 * 매번 다시 입력해야 한다. 프로그램은 강사료·수용비를 자동 계산하지 않는다 —
 * 실제 수강한 만큼이 얼마인지는 담당자만 안다.
 *
 * 금액·상태·이력은 한 번의 명령으로 함께 저장된다 (서버가 한 트랜잭션으로 처리).
 */

import { useMutation } from '@tanstack/react-query'
import { useState } from 'react'

import { Modal } from '@/components/Modal'
import { useToast } from '@/components/Toast'
import { Button, Field, Input, Notice, NumInput } from '@/components/ui'
import { api, errorMessage } from '@/ipc/api'
import type { CostItem, Enrollment, Fee } from '@/ipc/types'
import { won } from '@/lib/format'

import { AdjustmentToggle, adjustmentOf, todayISO } from './AdjustmentToggle'

/** 숫자만 남긴다. 빈 칸은 0으로 본다. */
function num(v: string): number {
  const digits = v.replace(/[^0-9]/g, '')
  return digits === '' ? 0 : Number(digits)
}

export function CancelModal({
  target,
  items,
  onClose,
  onDone,
}: {
  target: Enrollment
  items: CostItem[]
  onClose: () => void
  onDone: () => void
}) {
  const toast = useToast()
  const [reason, setReason] = useState('')

  const before = (code: string) =>
    target.fees.find((f) => f.itemCode === code)?.amount ?? 0

  // 기본값은 지금 금액 그대로
  const [draft, setDraft] = useState<Record<string, number>>(() =>
    Object.fromEntries(items.map((it) => [it.code, before(it.code)])),
  )

  const beforeTotal = items.reduce((s, it) => s + before(it.code), 0)
  const afterTotal = items.reduce((s, it) => s + (draft[it.code] ?? 0), 0)
  const changed = items.some((it) => (draft[it.code] ?? 0) !== before(it.code))

  const fees: Fee[] = items.map((it) => ({
    itemCode: it.code,
    amount: draft[it.code] ?? 0,
  }))

  // 환불 대상 (v0.1.5). 환불액 = 취소 직전 금액 - 취소 후 최종 금액.
  const [refund, setRefund] = useState(false)
  const [occurredOn, setOccurredOn] = useState(todayISO())
  const [adjNote, setAdjNote] = useState('')

  const 환불액 = (code: string) => before(code) - (draft[code] ?? 0)
  const 환불합계 = items.reduce((s, it) => s + 환불액(it.code), 0)
  // 취소 후 금액이 취소 전보다 큰 항목 — 환불액이 음수가 된다
  const 음수 = items.filter((it) => 환불액(it.code) < 0)

  const act = useMutation({
    mutationFn: () =>
      api.enrollmentCancel(
        target.id,
        fees,
        reason.trim(),
        adjustmentOf(refund, occurredOn, adjNote),
      ),
    onSuccess: () => {
      toast.ok(
        refund
          ? `취소 처리하고 환불 ${won(환불합계)}원을 등록했습니다.`
          : `취소 처리되었습니다. 징수금액 ${won(afterTotal)}원`,
      )
      onDone()
    },
    onError: (e) => toast.bad(errorMessage(e)),
  })

  return (
    <Modal
      title="수강 취소"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>닫기</Button>
          <Button
            variant="danger"
            onClick={() => act.mutate()}
            disabled={act.isPending || reason.trim() === '' || (refund && 음수.length > 0)}
            title={
              refund && 음수.length > 0
                ? '환불액이 음수가 되는 항목이 있습니다'
                : undefined
            }
          >
            {act.isPending ? '처리 중…' : refund ? '취소하고 환불 등록' : '취소 처리'}
          </Button>
        </>
      }
    >
      <div style={{ lineHeight: 1.8, marginBottom: 12 }}>
        {target.grade}학년 {target.classNo}반 {target.studentNo}번 <b>{target.name}</b>
        <br />
        <b>{target.deptLabel}</b>
      </div>

      <div className="tableWrap" style={{ border: '1px solid var(--gray-200)' }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 110 }}>항목</th>
              <th className="num" style={{ width: 110 }}>
                기존 금액
              </th>
              <th className="num">취소 후 최종 징수금액</th>
            </tr>
          </thead>
          <tbody>
            {items.map((it) => (
              <tr key={it.code}>
                <td>{it.name}</td>
                <td className="num">{won(before(it.code))}</td>
                <td className="num">
                  <NumInput
                    style={{ width: 120, textAlign: 'right' }}
                    value={draft[it.code] === 0 ? '' : won(draft[it.code] ?? 0)}
                    placeholder="0"
                    onChange={(e) =>
                      setDraft({ ...draft, [it.code]: num(e.target.value) })
                    }
                  />
                </td>
              </tr>
            ))}
            <tr>
              <td>
                <b>합계</b>
              </td>
              <td className="num">{won(beforeTotal)}</td>
              <td className="num">
                <b style={{ color: 'var(--navy-800)' }}>{won(afterTotal)}</b>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div className="toolbar" style={{ marginTop: 10 }}>
        <Button
          small
          onClick={() =>
            setDraft(Object.fromEntries(items.map((it) => [it.code, before(it.code)])))
          }
        >
          전액 징수
        </Button>
        <Button small onClick={() => setDraft(Object.fromEntries(items.map((it) => [it.code, 0])))}>
          전액 면제
        </Button>
      </div>
      <div className="hint" style={{ marginTop: 4 }}>
        이미 발생한 비용은 취소해도 징수합니다. 교재비·재료비는 배부했다면 그대로 두고,
        강사료·수용비는 실제 수강한 만큼으로 고쳐 주세요.
      </div>

      <Field label="변경사유 (필수)" hint="예: 5월 중도 포기, 교재·재료 배부 완료">
        <Input
          autoFocus
          value={reason}
          placeholder="개인 사정 · 전학 · 착오 등"
          onChange={(e) => setReason(e.target.value)}
        />
      </Field>

      {afterTotal === 0 && (
        <div className="hint" style={{ marginTop: 8 }}>
          전액 면제이므로 이 수강은 <b>정산에 들어가지 않습니다.</b>
        </div>
      )}
      {!changed && beforeTotal > 0 && (
        <div className="hint" style={{ marginTop: 8 }}>
          금액을 그대로 두면 <b>{won(beforeTotal)}원 전액을 징수</b>합니다.
        </div>
      )}
      <div className="hint" style={{ marginTop: 8 }}>
        취소해도 자료를 지우지 않습니다. 되돌릴 수 있고, 변경이력에 남습니다.
        금액을 바꾸면 정산은 <b>재정산 필요</b>가 됩니다.
      </div>

      <AdjustmentToggle
        label="환불 대상"
        hint={
          <>
            이미 이 기간 수강료를 걷은 뒤의 취소라서 <b>돌려줄 돈이 있으면</b> 체크하세요.
            환불액은 <b>취소 직전 금액 − 취소 후 최종 징수금액</b>으로 프로그램이 셉니다.
            <br />
            아직 걷기 전이라면 체크하지 않습니다 — 취소만 처리됩니다.
          </>
        }
        on={refund}
        setOn={setRefund}
        occurredOn={occurredOn}
        setOccurredOn={setOccurredOn}
        note={adjNote}
        setNote={setAdjNote}
      >
        {음수.length > 0 ? (
          <Notice tone="bad">
            <b>{음수.map((it) => it.name).join(' · ')}</b>의 취소 후 금액이 취소 전보다 많아
            환불액이 음수가 됩니다. 금액을 확인하시거나 [환불 대상] 체크를 풀어 주세요.
            <div className="hint" style={{ marginTop: 4 }}>
              체크를 풀면 지금까지처럼 취소만 처리됩니다.
            </div>
          </Notice>
        ) : (
          <div className="tableWrap" style={{ marginTop: 10 }}>
            <table className="table">
              <thead>
                <tr>
                  <th style={{ width: 90 }}>항목</th>
                  <th className="num">취소 직전</th>
                  <th className="num">취소 후 징수</th>
                  <th className="num">환불액</th>
                </tr>
              </thead>
              <tbody>
                {items.map((it) => (
                  <tr key={it.code}>
                    <td>{it.name}</td>
                    <td className="num">{won(before(it.code))}</td>
                    <td className="num">{won(draft[it.code] ?? 0)}</td>
                    <td className="num">
                      <b>{won(환불액(it.code))}</b>
                    </td>
                  </tr>
                ))}
                <tr style={{ background: 'var(--blue-50)' }}>
                  <td>
                    <b>합계</b>
                  </td>
                  <td className="num">{won(beforeTotal)}</td>
                  <td className="num">{won(afterTotal)}</td>
                  <td className="num">
                    <b style={{ color: 'var(--navy-800)' }}>{won(환불합계)}</b>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        )}
        {환불합계 === 0 && 음수.length === 0 && (
          <div className="hint" style={{ marginTop: 6 }}>
            돌려줄 금액이 0원입니다. 그래도 기록으로 남겨 두면 나중에 확인할 수 있습니다.
          </div>
        )}
      </AdjustmentToggle>
    </Modal>
  )
}
