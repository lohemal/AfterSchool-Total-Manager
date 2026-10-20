-- 학생 전출 · 맞춤형 프로그램 강사료 (v0.1.8, 설계안 29·30장)
--
-- 둘 다 **기존 금액 계산과 섞이지 않는다.** 전출은 학생의 재학 상태이고,
-- 맞춤형 강사료는 학교가 강사에게 지급하는 돈이다. 학생이 내는 수강료
-- (`charge`)와 그 정산에는 어느 쪽도 들어가지 않는다.
--
-- 그래서 이 갱신은 **기존 표를 하나도 고치지 않고** 칸과 표를 더하기만 한다.
-- 자료판 트리거도 건드리지 않으므로 이미 만든 정산이 낡음이 되지 않는다.

-- ─────────────────────────────────────────────── 학생 전출
--
-- 상태를 따로 저장하지 않고 **날짜 하나**만 둔다. `재학 / 전출 예정 / 전출`은
-- 오늘 날짜와 견주어 그때그때 판정한다. 상태 칸을 따로 두면 날짜를 고쳤을 때
-- 둘이 어긋나고, 전출 예정일이 지나도 아무도 상태를 갱신해 주지 않는다.
--
-- NULL 이 재학이다. 기존 학생은 모두 NULL 로 시작한다 — 이름이나 자료를 보고
-- 전출로 추측하지 않는다.
ALTER TABLE student ADD COLUMN transfer_out_date TEXT
  CHECK (transfer_out_date IS NULL OR
         transfer_out_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');

-- 학년도 안에서 전출 상태로 거를 때 쓴다.
CREATE INDEX student_transfer_ix ON student(year_id, transfer_out_date);

-- ─────────────────────────────────────────────── 맞춤형 기본 차시
--
-- **0 이 '맞춤형 프로그램 아님'이다.** `is_custom` 같은 참거짓 칸을 따로 두면
-- `is_custom = 1 인데 차시 0` 처럼 서로 어긋난 줄이 생길 수 있다. 칸 하나로
-- 두면 그런 상태가 아예 만들어지지 않는다.
--
-- 기존 부서는 모두 0 으로 시작한다 — 이름이나 금액을 보고 맞춤형이라고
-- 추측하지 않는다.
--
-- 주의: `ws_bump_dept_u` 트리거(006)는 바뀐 칸을 하나하나 적어 둔 WHEN 절을
-- 쓴다. 이 칸은 거기 없으므로 기본 차시를 고쳐도 작업공간 자료판이 오르지
-- 않는다. 맞춤형 강사료는 학생 수강료 정산과 다른 회계이기 때문이다.
ALTER TABLE department ADD COLUMN custom_sessions INTEGER NOT NULL DEFAULT 0
  CHECK (custom_sessions >= 0);

-- ─────────────────────────────────────────────── 차시당 단가
--
-- **학년도마다 둔다.** 단가는 해마다 바뀌는 정책값인데, 앱 전체에 하나만 두면
-- 올해 단가를 고치는 순간 지난 학년도의 강사료 자료까지 함께 바뀐다. 강사료
-- 합계를 따로 저장하지 않고 늘 다시 계산하기 때문이다(설계안 30-9).
--
-- `academic_year` 에는 트리거가 없으므로 단가를 고쳐도 자료판이 오르지 않는다.
ALTER TABLE academic_year ADD COLUMN custom_session_rate INTEGER NOT NULL DEFAULT 40000
  CHECK (custom_session_rate >= 0);

-- ─────────────────────────────────────────────── 작업공간 공통 휴업일
--
-- 공휴일 · 재량휴업일처럼 **그날 예정된 모든 반**이 쉬는 날.
CREATE TABLE workspace_closure (
  id           INTEGER PRIMARY KEY,
  workspace_id INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  on_date      TEXT    NOT NULL
               CHECK (on_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  reason       TEXT    NOT NULL DEFAULT '',
  created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX workspace_closure_uq ON workspace_closure(workspace_id, on_date);

-- ─────────────────────────────────────────────── 부서별 수업일 예외
--
-- ## 예정일을 통째로 복제해 두지 않는다
--
-- 예정 수업일은 `작업공간 기간 + 부서 요일` 에서 늘 다시 계산한다. 날짜를 미리
-- 전부 적어 두면 작업공간 기간이나 요일을 고쳤을 때 적어 둔 날짜와 실제 예정일이
-- 어긋나고, 어느 쪽이 맞는지 아무도 모르게 된다.
--
-- 그래서 **사람이 손댄 것만** 여기에 남는다. 한 날짜에 한 줄이고, 그 줄이 그
-- 날짜의 **최종 결정**이다.
--
-- ```text
-- 예정일인데 줄 없음          → 정상 · 부서 기본 차시
-- 예정일인데 공통 휴업일       → 휴강 · 0차시
-- 줄 있음 · sessions = 0      → 휴강 (공통 휴업일보다 이 줄이 우선)
-- 줄 있음 · sessions > 0      → 그 차시로 수업 (예정일이 아니면 보강)
-- ```
--
-- '휴강'과 '보강'을 칸으로 따로 적지 않는 까닭도 같다. 그 날짜가 예정일인지는
-- 기간과 요일에서 나오므로, 적어 두면 요일을 바꾼 날 거짓이 된다.
--
-- 줄을 지우면 그 날짜는 다시 기본 규칙으로 돌아간다 — '기본 차시로 되돌리기'와
-- '추가 수업일 삭제'가 같은 동작이다.
CREATE TABLE dept_session (
  id            INTEGER PRIMARY KEY,
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  on_date       TEXT    NOT NULL
                CHECK (on_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  sessions      INTEGER NOT NULL CHECK (sessions >= 0),
  reason        TEXT    NOT NULL DEFAULT '',
  created_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE UNIQUE INDEX dept_session_uq ON dept_session(department_id, on_date);
