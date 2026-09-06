-- 변경이력에 대상 학생·부서·비용항목을 따로 남긴다 (Phase 2).
--
-- `target`은 그때 화면에 보이던 문구를 그대로 담은 스냅샷이고,
-- 아래 세 컬럼은 나중에 "이 학생의 이력만" 같은 조회를 하기 위한 것이다.
--
-- **외래키를 걸지 않는다.** 학생이나 부서를 지워도 이력은 남아야 한다.
-- 이력이 원본과 함께 사라지면 감사 기록으로서 쓸모가 없다.

ALTER TABLE change_log ADD COLUMN student_id    INTEGER;
ALTER TABLE change_log ADD COLUMN department_id INTEGER;
ALTER TABLE change_log ADD COLUMN item_code     TEXT;

CREATE INDEX change_log_student_ix ON change_log(year_id, student_id);
CREATE INDEX change_log_ws_ix      ON change_log(workspace_id, at);
