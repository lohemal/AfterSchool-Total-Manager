-- 정산 시점의 학생별 지원금 상태를 스냅샷으로 남긴다 (Phase 3).
--
-- 왜 저장하는가: 이용권 탭이 보여 줘야 하는 값(기간 한도·이월액·이전 사용액·
-- 잔액)은 **그때 그 계산에 실제로 쓰인 값**이어야 한다. 화면을 열 때마다 다시
-- 계산하면, 정산 이후 정책이 바뀐 경우 저장된 배분액과 어긋난 숫자가 나란히
-- 보이게 된다. 정산 결과는 그 자체로 완결된 기록이어야 한다.
--
-- 외래키는 settlement에만 건다. 학생이 지워져도 과거 정산 기록은 남는다.

CREATE TABLE settlement_budget (
  id                    INTEGER PRIMARY KEY,
  settlement_id         INTEGER NOT NULL REFERENCES settlement(id) ON DELETE CASCADE,
  student_id            INTEGER NOT NULL,
  program               TEXT    NOT NULL CHECK (program IN ('VOUCHER', 'FREE_VOUCHER')),

  -- 그때의 정책값
  annual_limit          INTEGER NOT NULL DEFAULT 0,
  period_id             INTEGER,
  period_name           TEXT    NOT NULL DEFAULT '',
  period_limit          INTEGER NOT NULL DEFAULT 0,
  carryover             INTEGER NOT NULL DEFAULT 0 CHECK (carryover IN (0, 1)),

  -- 계산 과정 (설계안 5-2)
  carry_in              INTEGER NOT NULL DEFAULT 0,
  used_prior_periods    INTEGER NOT NULL DEFAULT 0,
  used_in_period_before INTEGER NOT NULL DEFAULT 0,
  used_outside_before   INTEGER NOT NULL DEFAULT 0,
  used_all_before       INTEGER NOT NULL DEFAULT 0,
  capped_by_annual      INTEGER NOT NULL DEFAULT 0 CHECK (capped_by_annual IN (0, 1)),

  -- 결과
  available             INTEGER NOT NULL DEFAULT 0,
  used_now              INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX settlement_budget_uq
  ON settlement_budget(settlement_id, student_id, program);
CREATE INDEX settlement_budget_ix ON settlement_budget(settlement_id, program);
