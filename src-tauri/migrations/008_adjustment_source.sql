-- 추가징수·환불의 발생 유형과 부서 일괄 묶음 (v0.1.9, 설계안 31장)
--
-- ## 금액 모델은 바꾸지 않는다
--
-- 학생이 최종으로 내는 돈은 **charge 하나**다. 정산·학생별 징수 내역·품의는
-- 모두 charge 만 읽고, 이 표들은 읽지 않는다. 추가징수·환불 기록은 '이미 걷은
-- 뒤에 얼마를 더 걷거나 돌려줄지'를 굳혀 둔 **행정기록**이다. 그래서 금액을
-- 고치면서 환불을 함께 적어도 한 번만 반영된다 (35,000 → 30,000 + 환불 5,000
-- 이면 최종 30,000).
--
-- ## 왜 발생 유형(source)이 꼭 필요한가 — 이름표가 아니다
--
-- [현재 금액 반영]은 기록의 금액을 지금 charge 로 다시 잡는다. 그 공식이 지금은
-- `kind` 만 보고 정해진다:
--
-- ```text
-- REFUND      amount = 취소 직전(base) - 지금 charge
-- ADDITIONAL  amount = 지금 charge            ← 중도에 들어온 학생의 '전액'
-- ```
--
-- 이번에 생기는 '차액' 추가징수(35,000 → 40,000 에서 5,000 더 걷기)를 이 꼴에
-- 그대로 담으면, 반영을 누르는 순간 금액이 **차액이 아니라 전액(40,000)** 으로
-- 바뀐다. 학부모에게 갈 추가징수액이 여덟 배로 부푼다. 그래서 차액 기록은
-- 공식이 다르다는 것을 표에 적어 두어야 한다.
--
-- ```text
-- 차액(DEPT_BATCH · INDIVIDUAL)
--   REFUND      amount = base - 지금 charge
--   ADDITIONAL  amount = 지금 charge - base   ← 새 공식
-- ```
--
-- ## 기존 기록은 추측하지 않는다
--
-- 이미 있는 기록은 모두 `LEGACY` 로 둔다. 코드상 지금까지 추가징수는 중도 추가,
-- 환불은 수강 취소에서만 만들어졌지만, 그렇다고 기록에 '수강취소'라고 적어
-- 넣지 않는다. `LEGACY` 는 예전 공식을 그대로 따르므로 계산이 바뀌지 않는다.
--
-- ## 정산과 무관하다
--
-- 이 표들에는 자료판 트리거가 없다(005 부터 그렇다). 기록을 만드는 일은 정산의
-- 원본을 바꾸는 일이 아니다. 차액이 생겼다면 그것은 charge 를 고친 쪽에서
-- 이미 정산을 낡음으로 만든다.

-- ─────────────────────────────────────────────── 부서 일괄 묶음
--
-- '이 18건이 왜 한꺼번에 생겼는가'를 나중에 알 수 있게 부모 줄을 둔다.
-- 합계는 저장하지 않는다 — 딸린 기록을 더해서 낸다.
CREATE TABLE billing_batch (
  id            INTEGER PRIMARY KEY,
  workspace_id  INTEGER NOT NULL REFERENCES workspace(id)  ON DELETE RESTRICT,
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE RESTRICT,
  occurred_on   TEXT    NOT NULL CHECK (length(occurred_on) = 10),
  -- 사유는 비워 둘 수 없다. 까닭 없는 돈 기록은 남기지 않는다.
  note          TEXT    NOT NULL CHECK (length(trim(note)) > 0),
  dept_label    TEXT    NOT NULL DEFAULT '',
  created_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX billing_batch_ws_ix ON billing_batch(workspace_id, occurred_on);

-- ─────────────────────────────────────────────── 발생 유형 · 묶음 연결
--
-- LEGACY      이 갱신 이전의 기록 (예전 공식 그대로)
-- ENROLL      중도에 들어온 학생의 전액 추가징수
-- CANCEL      수강 취소에서 생긴 환불
-- DEPT_BATCH  부서 일괄 차액
-- INDIVIDUAL  학생 개별 차액
ALTER TABLE billing_adjustment ADD COLUMN source TEXT NOT NULL DEFAULT 'LEGACY'
  CHECK (source IN ('LEGACY', 'ENROLL', 'CANCEL', 'DEPT_BATCH', 'INDIVIDUAL'));

ALTER TABLE billing_adjustment ADD COLUMN batch_id INTEGER
  REFERENCES billing_batch(id) ON DELETE RESTRICT;

CREATE INDEX billing_adj_batch_ix  ON billing_adjustment(batch_id);
CREATE INDEX billing_adj_source_ix ON billing_adjustment(workspace_id, source);
