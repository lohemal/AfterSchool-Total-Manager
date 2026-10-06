-- 정원 · 수강 가능 학년 (v0.1.7, 설계안 28장)
--
-- 둘 다 **운영정보**다. 금액 정산에는 들어가지 않는다. 그래서 이 둘을 고쳐도
-- 이미 만든 정산이 낡음이 되어서는 안 된다 — 아래에서 자료판 트리거를 함께
-- 손본다.

-- ─────────────────────────────────────────────── 정원
--
-- NULL 이 '정원 미설정'이다. 0 은 받지 않는다 — '0명만 받는 반'은 업무에
-- 없고, 미설정을 0 으로 적어 넣으면 충원율이 거짓으로 100% 가 된다.
ALTER TABLE department ADD COLUMN capacity INTEGER
  CHECK (capacity IS NULL OR capacity >= 1);

-- ─────────────────────────────────────────────── 수강 가능 학년
--
-- `min~max` 가 아니라 **집합**이다. 1·2·4학년처럼 띄엄띄엄 받는 반이 실제로
-- 있기 때문이다. 문자열 한 칸에 뭉개지 않고 한 학년 한 줄로 둔 까닭은 둘이다.
--   · "3학년을 받는 반" 을 색인으로 바로 찾을 수 있다
--   · 1~6 밖의 값이 들어오는 것을 DB 가 막는다
--
-- **줄이 하나도 없으면 '미설정'이다.** '전 학년 불가'라는 상태는 두지 않는다 —
-- 아무도 못 듣는 반은 업무에 없고, 빈 집합을 '전 학년'으로도 '전 학년 불가'로도
-- 읽을 수 있게 두면 화면마다 해석이 갈린다.
--
-- 주의: `support_policy.target_grades` 는 **빈 값이 '전 학년'** 이다. 이름이
-- 비슷하지만 뜻이 반대이므로 그쪽 헬퍼(`domain::grade_matches`)를 여기에
-- 그대로 쓰면 안 된다.
CREATE TABLE department_allowed_grade (
  department_id INTEGER NOT NULL REFERENCES department(id) ON DELETE CASCADE,
  grade         INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 6),
  PRIMARY KEY (department_id, grade)
) WITHOUT ROWID;

-- 학년으로 부서를 거꾸로 찾는다 (수강 가능 부서 찾기).
CREATE INDEX dept_allowed_grade_ix ON department_allowed_grade(grade, department_id);

-- ─────────────────────────────────────────────── 자료판 트리거를 좁힌다
--
-- 본래 트리거는 `AFTER UPDATE ON department` 라서 **어느 칸을 고쳐도** 작업공간
-- 자료판이 올라갔다. 그대로 두면 정원만 고쳐도 정산이 낡음이 된다.
--
-- `UPDATE OF capacity` 를 빼는 방법도 있지만, SQLite 의 `UPDATE OF` 는 값이
-- 실제로 바뀌었는지와 무관하게 **SET 절에 그 칸이 적히기만 하면** 켜진다.
-- 부서 수정 화면은 모든 칸을 한꺼번에 쓰므로 그 방법으로는 막히지 않는다.
-- 그래서 **값을 견주는 WHEN** 으로 바꾼다. 덤으로, 아무것도 바뀌지 않은
-- 저장(no-op)이 자료판을 올리던 것도 함께 사라진다.
DROP TRIGGER ws_bump_dept_u;
CREATE TRIGGER ws_bump_dept_u AFTER UPDATE ON department
WHEN NEW.workspace_id IS NOT OLD.workspace_id
  OR NEW.name         IS NOT OLD.name
  OR NEW.class_name   IS NOT OLD.class_name
  OR NEW.teacher      IS NOT OLD.teacher
  OR NEW.days         IS NOT OLD.days
  OR NEW.note         IS NOT OLD.note
BEGIN
  UPDATE workspace SET data_version = data_version + 1 WHERE id = NEW.workspace_id;
END;

-- ─────────────────────────────────────────────── 집계용 색인
--
-- 부서별 수강현황은 `department_id` 로 묶어 ACTIVE 만 센다. 기존
-- `enrollment_dept_ix(department_id)` 는 상태를 모르므로 줄마다 본표를 다시
-- 읽어야 한다. 상태까지 넣으면 색인만으로 센다.
CREATE INDEX enrollment_dept_status_ix ON enrollment(department_id, status);
