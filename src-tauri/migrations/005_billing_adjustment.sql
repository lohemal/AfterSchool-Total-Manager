-- 최초 징수 이후에 생긴 추가징수 · 환불 기록 (v0.1.5, 설계안 26장).
--
-- ## 수강생 명단과 무엇이 다른가
--
-- 수강생 명단(enrollment + charge)은 **지금 이 학생이 얼마를 내는가**를 관리한다.
-- 이 표는 **이미 한 번 걷은 뒤에 얼마를 더 걷거나 돌려주어야 하는가**라는,
-- 특정 시점에 일어난 행정처리를 관리한다. 둘은 서로 다른 자료다.
--
-- 그래서 금액을 charge 에서 그때그때 읽어 오지 않고 **만든 시점의 값으로 굳혀
-- 둔다.** 나중에 수강생 명단에서 금액을 고쳐도 이 기록이 저절로 따라 바뀌면
-- 안 된다 — 이미 학부모에게 그 금액으로 안내했을 수 있기 때문이다.
--
-- ## 자료판 트리거를 걸지 않는다
--
-- 이 표는 정산의 원본이 아니다. 추가징수·환불 기록을 만들었다고 해서 이미 만든
-- 정산이 낡음이 되어서는 안 되므로 `ws_bump_*` 같은 트리거를 두지 않는다.
--
-- ## 지우지 못하게 막는다
--
-- 돈이 오간 행정기록이므로 학생·수강·부서·작업공간을 지울 때 조용히 함께
-- 사라지면 안 된다. `ON DELETE RESTRICT` 로 막고, 응용 계층에서 사람이 읽을
-- 수 있는 까닭을 먼저 알려 준다.

CREATE TABLE billing_adjustment (
  id            INTEGER PRIMARY KEY,
  workspace_id  INTEGER NOT NULL REFERENCES workspace(id)   ON DELETE RESTRICT,
  student_id    INTEGER NOT NULL REFERENCES student(id)     ON DELETE RESTRICT,
  -- 어느 수강 건에서 생긴 일인지. 같은 학생·부서라도 취소 후 다시 수강하면
  -- 수강 건이 다르므로, 학생+부서가 아니라 **수강 건**에 묶는다.
  enrollment_id INTEGER NOT NULL REFERENCES enrollment(id)  ON DELETE RESTRICT,
  department_id INTEGER NOT NULL REFERENCES department(id)  ON DELETE RESTRICT,
  kind          TEXT    NOT NULL CHECK (kind IN ('ADDITIONAL_CHARGE', 'REFUND')),
  -- 발생일 'YYYY-MM-DD'
  occurred_on   TEXT    NOT NULL CHECK (length(occurred_on) = 10),
  note          TEXT    NOT NULL DEFAULT '',
  -- 만든 시점의 표시 이름. 나중에 반이나 부서명이 바뀌어도 그때 무엇이었는지
  -- 알 수 있어야 한다. 목록·필터는 현재 정보를 쓰고, 상세에 이 값을 함께 적는다.
  student_label TEXT    NOT NULL DEFAULT '',
  dept_label    TEXT    NOT NULL DEFAULT '',
  created_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  updated_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX billing_adj_ws_ix      ON billing_adjustment(workspace_id, kind, occurred_on);
CREATE INDEX billing_adj_student_ix ON billing_adjustment(student_id);
CREATE INDEX billing_adj_enroll_ix  ON billing_adjustment(enrollment_id);
CREATE INDEX billing_adj_dept_ix    ON billing_adjustment(department_id);

-- 항목별 금액. 합계는 저장하지 않고 이 표를 더해서 낸다 — 합계를 따로 두면
-- 언젠가 둘이 어긋나고, 그때 어느 쪽이 맞는지 아무도 모른다.
CREATE TABLE billing_adjustment_amount (
  id            INTEGER PRIMARY KEY,
  adjustment_id INTEGER NOT NULL REFERENCES billing_adjustment(id) ON DELETE CASCADE,
  item_code     TEXT    NOT NULL REFERENCES cost_item(code),

  -- 이 조정의 금액. 추가징수면 더 걷을 돈, 환불이면 돌려줄 돈.
  amount         INTEGER NOT NULL DEFAULT 0 CHECK (amount >= 0),

  -- 환불의 기준이 되는 **취소 직전** 금액. 추가징수에서는 쓰지 않아 0이다.
  -- 나중에 취소자의 최종 금액을 고쳐도 환불 기준을 잃지 않으려고 굳혀 둔다.
  base_amount    INTEGER NOT NULL DEFAULT 0 CHECK (base_amount >= 0),

  -- **마지막으로 사람이 확인한 그때의 원본 charge.**
  --
  -- 확인이 필요한지는 깃발이 아니라 이 값과 지금 charge 를 견주어 정한다.
  -- 깃발을 쓰면 금액을 고치는 모든 경로가 빠짐없이 깃발을 세워야 하고, 같은
  -- 값을 다시 쓰는 것(no-op)까지 확인 필요로 잡힌다. 값을 견주면 두 문제가
  -- 함께 사라진다.
  checked_charge INTEGER NOT NULL DEFAULT 0 CHECK (checked_charge >= 0)
);

CREATE UNIQUE INDEX billing_adj_amount_uq ON billing_adjustment_amount(adjustment_id, item_code);
