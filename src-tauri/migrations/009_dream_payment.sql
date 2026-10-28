-- 방과후드림 결제관리 기반 (P1)
--
-- 방과후드림 결제파일(학생 × 강좌 × 항목 = 한 줄, O 개인부담금 · P 이용권 ·
-- Q 자유수강권)을 만들 때 필요한 **설정과 처리 이력**만 담는다. 금액의 기준은
-- 여전히 최신 정산(settlement_alloc)이고, 이 표들은 그것을 읽기만 한다.
--
-- ## 기존 표를 고치지 않는다
--
-- 새 표를 만들기만 한다. 기존 표에 칸을 더하지 않고, 기존 자료판 트리거에 이
-- 표들을 잇지 않는다. 그래서 방과후드림 설정을 아무리 바꿔도 작업공간·학년도
-- 자료판이 오르지 않고, 이미 만든 정산이 낡음이 되지 않는다. 기존 정산
-- 우선순위(dept_priority · item_priority)와도 따로다.
--
-- ## 지우기를 막지 않는다
--
-- 학년도 · 작업공간 · 학생 · 부서를 지울 때 이 표들 때문에 실패하면 안 된다.
-- 그래서 기존 표를 가리키는 외래키는 모두 CASCADE(설정·이력이 함께 사라짐)
-- 이거나 SET NULL(30일 이력 줄은 남기고 연결만 끊음)이다. RESTRICT 는 없다.
--
-- ## 개인정보는 평문으로 두지 않는다
--
-- 개인참조번호 · 생년월일은 AES-256-GCM 암호문과 nonce 만 둔다. 암호 키는
-- dream_key 에 Windows DPAPI(현재 사용자)로 감싼 사본만 있다. 개인참조번호는
-- 정확히 찾기 위한 HMAC-SHA256 값을 따로 둔다 — 원문은 이것으로 되찾을 수 없다.
-- 처리 이력에는 이름 · 개인참조번호 · 생년월일을 적지 않는다.

-- ─────────────────────────────────────────────── 암호 키
--
-- 키 평문은 DB 에 없다. dpapi_blob 은 이 PC · 이 Windows 사용자에서만 풀린다.
-- 키 줄을 지우는 기능은 두지 않는다(지우면 암호문을 되돌릴 수 없다).

CREATE TABLE dream_key (
  id          INTEGER PRIMARY KEY,
  key_id      TEXT    NOT NULL UNIQUE CHECK (length(key_id) BETWEEN 1 AND 64),
  algorithm   TEXT    NOT NULL DEFAULT 'AES-256-GCM' CHECK (algorithm = 'AES-256-GCM'),
  dpapi_blob  BLOB    NOT NULL CHECK (length(dpapi_blob) > 0),
  created_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- ─────────────────────────────────────────────── 학생 개인참조번호 · 생년월일
--
-- student 는 이미 학년도별 행이다. 그 student.id 에 매단다. 학생 원본 표에 칸을
-- 더하지 않으므로 학생 업로드 · 전출 · 삭제 흐름은 그대로다.
--
-- 같은 학년도에서 같은 개인참조번호를 두 학생에게 줄 수 없다 — 평문이 아니라
-- ref_lookup(HMAC)으로 막는다. 한 학생에게 번호는 하나다(student_id 가 열쇠).

CREATE TABLE dream_student_identity (
  student_id    INTEGER PRIMARY KEY REFERENCES student(id) ON DELETE CASCADE,
  year_id       INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  key_id        TEXT    NOT NULL REFERENCES dream_key(key_id),
  ref_nonce     BLOB,
  ref_cipher    BLOB,
  ref_lookup    TEXT,
  birth_nonce   BLOB,
  birth_cipher  BLOB,
  updated_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  -- 셋은 함께 있거나 함께 없다
  CHECK ((ref_nonce IS NULL) = (ref_cipher IS NULL) AND (ref_cipher IS NULL) = (ref_lookup IS NULL)),
  CHECK ((birth_nonce IS NULL) = (birth_cipher IS NULL)),
  CHECK (ref_nonce IS NULL OR length(ref_nonce) = 12),
  CHECK (birth_nonce IS NULL OR length(birth_nonce) = 12),
  CHECK (ref_lookup IS NULL OR length(ref_lookup) = 64),
  -- 아무것도 없는 줄은 두지 않는다 (둘 다 지우면 줄을 지운다)
  CHECK (ref_cipher IS NOT NULL OR birth_cipher IS NOT NULL)
);
CREATE UNIQUE INDEX dream_identity_ref_uq
  ON dream_student_identity(year_id, ref_lookup) WHERE ref_lookup IS NOT NULL;

-- 학년도는 그 학생의 학년도여야 한다. 다르면 다른 학년도의 번호 중복 검사를
-- 빠져나간다.
CREATE TRIGGER dream_identity_year_i BEFORE INSERT ON dream_student_identity
WHEN NEW.year_id IS NOT (SELECT year_id FROM student WHERE id = NEW.student_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_student_identity.year_id 가 학생의 학년도와 다릅니다');
END;
CREATE TRIGGER dream_identity_year_u BEFORE UPDATE ON dream_student_identity
WHEN NEW.year_id IS NOT (SELECT year_id FROM student WHERE id = NEW.student_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_student_identity.year_id 가 학생의 학년도와 다릅니다');
END;

-- ─────────────────────────────────────────────── 강좌코드 ↔ 부서
--
-- 부서는 작업공간마다 새 id 이므로 매핑도 작업공간 단위다. 강좌코드 하나는
-- 부서 하나에만, 부서 하나도 강좌코드 하나에만 잇는다(초기 정책).

CREATE TABLE dream_course_map (
  id             INTEGER PRIMARY KEY,
  workspace_id   INTEGER NOT NULL REFERENCES workspace(id)  ON DELETE CASCADE,
  department_id  INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  course_code    TEXT    NOT NULL CHECK (course_code = trim(course_code) AND length(course_code) BETWEEN 1 AND 64),
  confirmed_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX dream_course_code_uq ON dream_course_map(workspace_id, course_code);
CREATE UNIQUE INDEX dream_course_dept_uq ON dream_course_map(department_id);

CREATE TRIGGER dream_course_ws_i BEFORE INSERT ON dream_course_map
WHEN NEW.workspace_id IS NOT (SELECT workspace_id FROM department WHERE id = NEW.department_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_course_map.workspace_id 가 부서의 작업공간과 다릅니다');
END;
CREATE TRIGGER dream_course_ws_u BEFORE UPDATE ON dream_course_map
WHEN NEW.workspace_id IS NOT (SELECT workspace_id FROM department WHERE id = NEW.department_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_course_map.workspace_id 가 부서의 작업공간과 다릅니다');
END;

-- ─────────────────────────────────────────────── 방과후드림 전용 우선순위
--
-- 기존 정산 우선순위와 따로다. 항목 순서는 부서마다 다르게 둘 수 있다.

CREATE TABLE dream_dept_priority (
  id             INTEGER PRIMARY KEY,
  workspace_id   INTEGER NOT NULL REFERENCES workspace(id)  ON DELETE CASCADE,
  department_id  INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  sort_order     INTEGER NOT NULL CHECK (sort_order >= 1)
);
CREATE UNIQUE INDEX dream_dept_priority_uq  ON dream_dept_priority(department_id);
CREATE UNIQUE INDEX dream_dept_priority_ord ON dream_dept_priority(workspace_id, sort_order);

CREATE TRIGGER dream_dept_priority_ws_i BEFORE INSERT ON dream_dept_priority
WHEN NEW.workspace_id IS NOT (SELECT workspace_id FROM department WHERE id = NEW.department_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_dept_priority.workspace_id 가 부서의 작업공간과 다릅니다');
END;
CREATE TRIGGER dream_dept_priority_ws_u BEFORE UPDATE ON dream_dept_priority
WHEN NEW.workspace_id IS NOT (SELECT workspace_id FROM department WHERE id = NEW.department_id)
BEGIN
  SELECT RAISE(ABORT, 'dream_dept_priority.workspace_id 가 부서의 작업공간과 다릅니다');
END;

-- 항목 순서는 부서에 매단다. 부서가 곧 작업공간을 정하므로 작업공간 칸을 따로
-- 두지 않는다(두면 어긋날 수 있는 값이 하나 더 생긴다).
CREATE TABLE dream_item_priority (
  id             INTEGER PRIMARY KEY,
  department_id  INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  item_code      TEXT    NOT NULL REFERENCES cost_item(code),
  sort_order     INTEGER NOT NULL CHECK (sort_order >= 1)
);
CREATE UNIQUE INDEX dream_item_priority_uq  ON dream_item_priority(department_id, item_code);
CREATE UNIQUE INDEX dream_item_priority_ord ON dream_item_priority(department_id, sort_order);

-- ─────────────────────────────────────────────── 처리 이력 (30일)
--
-- 원본 · 결과 파일은 DB 에 넣지 않는다. 파일 이름(경로 없음) · SHA-256 · 크기와,
-- 그때 기준으로 삼은 정산을 값으로 적어 둔다. 정산을 외래키로 묶지 않는 것은
-- 작업공간을 지우거나 다시 정산할 때 이 이력이 걸림돌이 되지 않게 하려는 것이다.

CREATE TABLE dream_job (
  id                  INTEGER PRIMARY KEY,
  workspace_id        INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  status              TEXT    NOT NULL DEFAULT 'OPEN'
                      CHECK (status IN ('OPEN', 'BLOCKED', 'READY', 'EXPORTED', 'FAILED', 'CANCELLED')),
  source_name         TEXT    NOT NULL CHECK (length(source_name) BETWEEN 1 AND 255),
  source_sha256       TEXT    NOT NULL CHECK (length(source_sha256) = 64),
  source_size         INTEGER NOT NULL CHECK (source_size >= 0),
  settlement_id       INTEGER,
  settlement_created  TEXT,
  ws_data_version     INTEGER,
  year_data_version   INTEGER,
  settle_state        TEXT,
  result_sha256       TEXT    CHECK (result_sha256 IS NULL OR length(result_sha256) = 64),
  app_version         TEXT    NOT NULL,
  created_at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  finished_at         TEXT
);
CREATE INDEX dream_job_ws_ix  ON dream_job(workspace_id, created_at);
CREATE INDEX dream_job_age_ix ON dream_job(created_at);

-- 파일 한 줄. 학생 · 부서가 지워져도 30일 이력 줄은 남기고 연결만 끊는다.
CREATE TABLE dream_job_row (
  id               INTEGER PRIMARY KEY,
  job_id           INTEGER NOT NULL REFERENCES dream_job(id) ON DELETE CASCADE,
  source_row       INTEGER NOT NULL CHECK (source_row >= 2),
  student_id       INTEGER REFERENCES student(id)    ON DELETE SET NULL,
  department_id    INTEGER REFERENCES department(id) ON DELETE SET NULL,
  dream_item_code  TEXT    NOT NULL DEFAULT '',
  item_code        TEXT    REFERENCES cost_item(code),
  amount_n         INTEGER,
  before_o         INTEGER,
  before_p         INTEGER,
  before_q         INTEGER,
  after_o          INTEGER,
  after_p          INTEGER,
  after_q          INTEGER,
  match_status     TEXT    NOT NULL DEFAULT 'PENDING'
                   CHECK (match_status IN ('PENDING', 'MATCHED', 'NEEDS_REVIEW', 'ERROR')),
  reason_code      TEXT    NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX dream_job_row_uq ON dream_job_row(job_id, source_row);

-- 일어난 일. detail 에는 건수 · 오류 코드 같은 것만 — 개인정보 금지.
CREATE TABLE dream_job_event (
  id      INTEGER PRIMARY KEY,
  job_id  INTEGER NOT NULL REFERENCES dream_job(id) ON DELETE CASCADE,
  at      TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  kind    TEXT    NOT NULL CHECK (length(kind) BETWEEN 1 AND 40),
  detail  TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX dream_job_event_ix ON dream_job_event(job_id, at);
