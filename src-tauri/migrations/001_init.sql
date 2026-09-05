-- 방과후 통합 매니저 — 최초 스키마 (설계안 v2 4장)
--
-- 이 파일은 한 번 배포된 뒤에는 절대 수정하지 않는다.
-- 스키마를 바꿔야 하면 002_*.sql 을 새로 만든다.
--
-- 금액은 모두 INTEGER(원). 나눗셈이 없으므로 반올림 오차가 생기지 않는다.
-- 날짜는 모두 TEXT 'YYYY-MM-DD', 일시는 TEXT 'YYYY-MM-DD HH:MM:SS'.

-- ─────────────────────────────────────────────── 학년도 · 작업공간

CREATE TABLE academic_year (
  id           INTEGER PRIMARY KEY,
  year         INTEGER NOT NULL UNIQUE,           -- 2026
  name         TEXT    NOT NULL,                  -- '2026학년도'
  start_date   TEXT    NOT NULL,
  end_date     TEXT    NOT NULL,
  data_version INTEGER NOT NULL DEFAULT 0,        -- 정산에 영향을 주는 학년도 자료 변경 카운터
  is_current   INTEGER NOT NULL DEFAULT 0 CHECK (is_current IN (0, 1)),
  created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- 작업공간의 순서는 저장하지 않는다.
--
-- 화면에 보이는 순서와 지원금 누적 순서가 갈리면 조용히 틀린 정산이 나온다.
-- 그래서 둘 다 언제나 (start_date, end_date, id) 하나만 쓴다. 사람이 바꿀 수 있는
-- 순서 컬럼을 두지 않는 것이 이 표에서 가장 중요한 결정이다.
CREATE TABLE workspace (
  id           INTEGER PRIMARY KEY,
  year_id      INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,                  -- '2026년 4월', '여름방학'
  start_date   TEXT    NOT NULL,
  end_date     TEXT    NOT NULL,
  data_version INTEGER NOT NULL DEFAULT 0,
  is_current   INTEGER NOT NULL DEFAULT 0 CHECK (is_current IN (0, 1)),
  note         TEXT    NOT NULL DEFAULT '',
  created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  CHECK (start_date <= end_date)
);
CREATE INDEX workspace_order_ix ON workspace(year_id, start_date, end_date, id);

-- ─────────────────────────────────────────────── 학생

CREATE TABLE student (
  id         INTEGER PRIMARY KEY,
  year_id    INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  grade      INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 9),
  class_no   INTEGER NOT NULL CHECK (class_no BETWEEN 1 AND 99),
  student_no INTEGER NOT NULL CHECK (student_no BETWEEN 1 AND 99),
  name       TEXT    NOT NULL,
  note       TEXT    NOT NULL DEFAULT '',
  created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX student_key_uq ON student(year_id, grade, class_no, student_no);
CREATE INDEX student_name_ix  ON student(year_id, name);
CREATE INDEX student_class_ix ON student(year_id, grade, class_no);

-- ─────────────────────────────────────────────── 지원정책 · 지원기간 · 자격

-- program: 'VOUCHER'(방과후 이용권) | 'FREE_VOUCHER'(자유수강권)
CREATE TABLE support_policy (
  id            INTEGER PRIMARY KEY,
  year_id       INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  program       TEXT    NOT NULL CHECK (program IN ('VOUCHER', 'FREE_VOUCHER')),
  annual_limit  INTEGER NOT NULL DEFAULT 0 CHECK (annual_limit >= 0),
  carryover     INTEGER NOT NULL DEFAULT 0 CHECK (carryover IN (0, 1)),
  target_grades TEXT    NOT NULL DEFAULT '',      -- '3' 또는 '3,4'. 빈 값이면 전 학년
  label_fund    TEXT    NOT NULL DEFAULT '',      -- Excel 이름표 직접 지정(비우면 자동 생성)
  label_over    TEXT    NOT NULL DEFAULT '',
  updated_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX support_policy_uq ON support_policy(year_id, program);

-- 행이 0개면 '학년도 전체가 하나의 기간'으로 동작한다 (설계안 5-1)
CREATE TABLE support_period (
  id           INTEGER PRIMARY KEY,
  policy_id    INTEGER NOT NULL REFERENCES support_policy(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,                  -- '1학기'
  start_date   TEXT    NOT NULL,
  end_date     TEXT    NOT NULL,
  limit_amount INTEGER NOT NULL DEFAULT 0 CHECK (limit_amount >= 0),
  seq          INTEGER NOT NULL,
  CHECK (start_date <= end_date)
);
CREATE UNIQUE INDEX support_period_seq_uq ON support_period(policy_id, seq);

-- 학생별 예외 한도. period_id가 NULL이면 연간 한도 override
CREATE TABLE support_grant (
  id         INTEGER PRIMARY KEY,
  year_id    INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  student_id INTEGER NOT NULL REFERENCES student(id) ON DELETE CASCADE,
  program    TEXT    NOT NULL CHECK (program IN ('VOUCHER', 'FREE_VOUCHER')),
  period_id  INTEGER REFERENCES support_period(id) ON DELETE CASCADE,
  amount     INTEGER NOT NULL CHECK (amount >= 0),
  reason     TEXT    NOT NULL DEFAULT '',
  updated_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX support_grant_uq
  ON support_grant(year_id, student_id, program, IFNULL(period_id, 0));

-- 지원자격. valid_from/valid_to가 NULL이면 학년도 내내 유효 (설계안 0-2)
CREATE TABLE support_eligibility (
  id         INTEGER PRIMARY KEY,
  year_id    INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  student_id INTEGER NOT NULL REFERENCES student(id) ON DELETE CASCADE,
  program    TEXT    NOT NULL CHECK (program IN ('VOUCHER', 'FREE_VOUCHER')),
  valid_from TEXT,
  valid_to   TEXT,
  source     TEXT    NOT NULL DEFAULT 'MANUAL' CHECK (source IN ('MANUAL', 'EXCEL')),
  note       TEXT    NOT NULL DEFAULT '',
  created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_from <= valid_to)
);
CREATE INDEX support_elig_student_ix ON support_eligibility(year_id, student_id, program);
CREATE INDEX support_elig_program_ix ON support_eligibility(year_id, program);

-- ─────────────────────────────────────────────── 비용항목 · 부서

-- 4개를 컬럼이 아니라 행으로 둔다 (설계안 4-2)
CREATE TABLE cost_item (
  code       TEXT    PRIMARY KEY,
  name       TEXT    NOT NULL,
  sort_order INTEGER NOT NULL,
  is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
);
INSERT INTO cost_item (code, name, sort_order) VALUES
  ('INSTRUCTOR', '강사료', 1),
  ('OPERATION',  '수용비', 2),
  ('TEXTBOOK',   '교재비', 3),
  ('MATERIAL',   '재료비', 4);

CREATE TABLE department (
  id           INTEGER PRIMARY KEY,
  workspace_id INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,                  -- '로봇과학'
  class_name   TEXT    NOT NULL DEFAULT '',       -- 'A반'
  teacher      TEXT    NOT NULL DEFAULT '',
  days         TEXT    NOT NULL DEFAULT '',       -- '월,수'
  note         TEXT    NOT NULL DEFAULT '',
  created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX department_uq ON department(workspace_id, name, class_name);

-- 부서의 '기준' 수강료. 실제 청구액은 charge에 따로 있다
CREATE TABLE department_fee (
  id            INTEGER PRIMARY KEY,
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  item_code     TEXT    NOT NULL REFERENCES cost_item(code),
  amount        INTEGER NOT NULL DEFAULT 0 CHECK (amount >= 0)
);
CREATE UNIQUE INDEX department_fee_uq ON department_fee(department_id, item_code);

-- ─────────────────────────────────────────────── 수강 · 청구액

CREATE TABLE enrollment (
  id            INTEGER PRIMARY KEY,
  workspace_id  INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  student_id    INTEGER NOT NULL REFERENCES student(id) ON DELETE CASCADE,
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  status        TEXT    NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'CANCELLED')),
  change_reason TEXT    NOT NULL DEFAULT '',
  created_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  updated_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
-- 취소 후 재등록은 되고, 동시 중복만 막는다
CREATE UNIQUE INDEX enrollment_active_uq
  ON enrollment(workspace_id, student_id, department_id) WHERE status = 'ACTIVE';
CREATE INDEX enrollment_ws_ix      ON enrollment(workspace_id, status);
CREATE INDEX enrollment_student_ix ON enrollment(student_id);
CREATE INDEX enrollment_dept_ix    ON enrollment(department_id);

-- 정산의 유일한 원본 금액
CREATE TABLE charge (
  id            INTEGER PRIMARY KEY,
  enrollment_id INTEGER NOT NULL REFERENCES enrollment(id) ON DELETE CASCADE,
  item_code     TEXT    NOT NULL REFERENCES cost_item(code),
  amount        INTEGER NOT NULL DEFAULT 0 CHECK (amount >= 0),
  is_overridden INTEGER NOT NULL DEFAULT 0 CHECK (is_overridden IN (0, 1))
);
CREATE UNIQUE INDEX charge_uq ON charge(enrollment_id, item_code);

-- ─────────────────────────────────────────────── 차감 우선순위 (둘 다 작업공간 소속)

CREATE TABLE dept_priority (
  id            INTEGER PRIMARY KEY,
  workspace_id  INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  sort_order    INTEGER NOT NULL
);
CREATE UNIQUE INDEX dept_priority_uq     ON dept_priority(workspace_id, department_id);
CREATE UNIQUE INDEX dept_priority_ord_uq ON dept_priority(workspace_id, sort_order);

CREATE TABLE item_priority (
  id           INTEGER PRIMARY KEY,
  workspace_id INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  item_code    TEXT    NOT NULL REFERENCES cost_item(code),
  sort_order   INTEGER NOT NULL
);
CREATE UNIQUE INDEX item_priority_uq     ON item_priority(workspace_id, item_code);
CREATE UNIQUE INDEX item_priority_ord_uq ON item_priority(workspace_id, sort_order);

-- ─────────────────────────────────────────────── 정산 스냅샷

CREATE TABLE settlement (
  id                INTEGER PRIMARY KEY,
  workspace_id      INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  created_at        TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  ws_data_version   INTEGER NOT NULL,
  year_data_version INTEGER NOT NULL,
  prior_key         TEXT    NOT NULL DEFAULT '',
  program_order     TEXT    NOT NULL DEFAULT 'VOUCHER,FREE_VOUCHER',
  is_latest         INTEGER NOT NULL DEFAULT 1 CHECK (is_latest IN (0, 1)),
  warning_json      TEXT    NOT NULL DEFAULT '[]'
);
CREATE UNIQUE INDEX settlement_latest_uq ON settlement(workspace_id) WHERE is_latest = 1;
CREATE INDEX settlement_ws_ix ON settlement(workspace_id, created_at);

CREATE TABLE settlement_alloc (
  id            INTEGER PRIMARY KEY,
  settlement_id INTEGER NOT NULL REFERENCES settlement(id) ON DELETE CASCADE,
  student_id    INTEGER NOT NULL,
  department_id INTEGER NOT NULL,
  enrollment_id INTEGER NOT NULL,
  item_code     TEXT    NOT NULL,
  fund          TEXT    NOT NULL
                CHECK (fund IN ('SELF_PAY', 'VOUCHER', 'VOUCHER_OVER', 'FREE_VOUCHER')),
  origin        TEXT    NOT NULL DEFAULT 'PLAIN'
                CHECK (origin IN ('PLAIN', 'VOUCHER_EXHAUSTED', 'FREE_EXHAUSTED')),
  amount        INTEGER NOT NULL CHECK (amount > 0)
);
CREATE INDEX alloc_student_ix ON settlement_alloc(settlement_id, student_id, fund);
CREATE INDEX alloc_dept_ix    ON settlement_alloc(settlement_id, department_id, item_code, fund);
CREATE INDEX alloc_fund_ix    ON settlement_alloc(settlement_id, fund);

-- ─────────────────────────────────────────────── 변경이력 · 설정

CREATE TABLE change_log (
  id           INTEGER PRIMARY KEY,
  year_id      INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  workspace_id INTEGER REFERENCES workspace(id) ON DELETE SET NULL,
  at           TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  kind         TEXT    NOT NULL,
  target       TEXT    NOT NULL DEFAULT '',
  before_value TEXT    NOT NULL DEFAULT '',
  after_value  TEXT    NOT NULL DEFAULT '',
  reason       TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX change_log_ix ON change_log(year_id, at);

CREATE TABLE app_setting (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO app_setting (key, value) VALUES ('program_order', 'VOUCHER,FREE_VOUCHER');

-- ─────────────────────────────────────────────── 자료 버전 트리거
-- 응용 코드가 아니라 트리거에 두는 이유: 나중에 기능을 추가하다 버전 올리기를
-- 빠뜨리면 사용자가 조용히 낡은 정산 숫자를 보게 된다. 트리거는 잊을 수가 없다.

CREATE TRIGGER ws_bump_dept_i AFTER INSERT ON department BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_dept_u AFTER UPDATE ON department BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_dept_d AFTER DELETE ON department BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = OLD.workspace_id;
END;

CREATE TRIGGER ws_bump_fee_i AFTER INSERT ON department_fee BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM department WHERE id = NEW.department_id);
END;
CREATE TRIGGER ws_bump_fee_u AFTER UPDATE ON department_fee BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM department WHERE id = NEW.department_id);
END;
CREATE TRIGGER ws_bump_fee_d AFTER DELETE ON department_fee BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM department WHERE id = OLD.department_id);
END;

CREATE TRIGGER ws_bump_enr_i AFTER INSERT ON enrollment BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_enr_u AFTER UPDATE ON enrollment BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_enr_d AFTER DELETE ON enrollment BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = OLD.workspace_id;
END;

CREATE TRIGGER ws_bump_chg_i AFTER INSERT ON charge BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM enrollment WHERE id = NEW.enrollment_id);
END;
CREATE TRIGGER ws_bump_chg_u AFTER UPDATE ON charge BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM enrollment WHERE id = NEW.enrollment_id);
END;
CREATE TRIGGER ws_bump_chg_d AFTER DELETE ON charge BEGIN
  UPDATE workspace SET data_version = data_version + 1
   WHERE id = (SELECT workspace_id FROM enrollment WHERE id = OLD.enrollment_id);
END;

CREATE TRIGGER ws_bump_dp_i AFTER INSERT ON dept_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_dp_u AFTER UPDATE ON dept_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_dp_d AFTER DELETE ON dept_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = OLD.workspace_id;
END;

CREATE TRIGGER ws_bump_ip_i AFTER INSERT ON item_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_ip_u AFTER UPDATE ON item_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;
CREATE TRIGGER ws_bump_ip_d AFTER DELETE ON item_priority BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = OLD.workspace_id;
END;

-- 학년도 자료: 지원정책 · 지원기간 · 예외한도 · 자격

CREATE TRIGGER yr_bump_pol_i AFTER INSERT ON support_policy BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_pol_u AFTER UPDATE ON support_policy BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_pol_d AFTER DELETE ON support_policy BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = OLD.year_id;
END;

CREATE TRIGGER yr_bump_per_i AFTER INSERT ON support_period BEGIN
  UPDATE academic_year SET data_version = data_version + 1
   WHERE id = (SELECT year_id FROM support_policy WHERE id = NEW.policy_id);
END;
CREATE TRIGGER yr_bump_per_u AFTER UPDATE ON support_period BEGIN
  UPDATE academic_year SET data_version = data_version + 1
   WHERE id = (SELECT year_id FROM support_policy WHERE id = NEW.policy_id);
END;
CREATE TRIGGER yr_bump_per_d AFTER DELETE ON support_period BEGIN
  UPDATE academic_year SET data_version = data_version + 1
   WHERE id = (SELECT year_id FROM support_policy WHERE id = OLD.policy_id);
END;

CREATE TRIGGER yr_bump_grant_i AFTER INSERT ON support_grant BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_grant_u AFTER UPDATE ON support_grant BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_grant_d AFTER DELETE ON support_grant BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = OLD.year_id;
END;

CREATE TRIGGER yr_bump_elig_i AFTER INSERT ON support_eligibility BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_elig_u AFTER UPDATE ON support_eligibility BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
CREATE TRIGGER yr_bump_elig_d AFTER DELETE ON support_eligibility BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = OLD.year_id;
END;

-- 학생은 '학년'이 바뀔 때만 올린다. 이름·반·번호 수정은 금액에 영향이 없는데
-- 그때마다 '다시 정산하세요' 경고가 뜨면 사람이 경고를 무시하게 된다.
CREATE TRIGGER yr_bump_student_grade AFTER UPDATE OF grade ON student
WHEN OLD.grade <> NEW.grade BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
