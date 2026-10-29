-- 방과후드림 결제관리 — 적용 학년 · 공통 항목 차례 (P6)
--
-- ## 적용 학년
--
-- 방과후드림을 쓰는 학년. 이 작업공간의 정산 학생 가운데 이 학년만 결제파일과 맞대어
-- 본다(그 학년에서 금액이 남은 학생은 모두 파일에 있어야 하고, 다른 학년 학생이 파일에
-- 있으면 오류). 작업공간마다 사람이 고른다. 없으면 '미설정'이다 — 업데이트했다고 전 학년을
-- 몰래 채워 넣지 않는다. 작업공간을 지우면 함께 지워진다(같은 id 가 다시 쓰여도 옛 설정이
-- 따라오지 않게 하려고 app_setting 이 아니라 외래키가 있는 표에 둔다).
--
-- ## 공통 항목 차례
--
-- 지원금을 담을 항목 차례(강사료 · 수용비 · 교재비 · 재료비). 모든 부서에 같은 차례를
-- 쓴다(실제 업무에서 부서마다 다르게 두지 않는다). 프로그램 전체에 한 벌이다.
-- 009 의 부서별 `dream_item_priority` 는 지우지 않는다 — 더는 배분에 쓰지 않고, 공통
-- 차례를 처음 정할 때 기본값을 제안하는 데만 읽는다. 옮겨 담지도 않는다(부서마다 다르면
-- 어느 것을 고를지 사람이 정해야 한다).
--
-- 기존 표 · 기존 트리거는 고치지 않는다. 자료판 트리거에 잇지 않으므로 바꿔도 정산이
-- 낡음이 되지 않는다.

CREATE TABLE dream_applied_grade (
  workspace_id  INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  grade         INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 9),
  PRIMARY KEY (workspace_id, grade)
);

CREATE TABLE dream_item_order (
  item_code   TEXT    PRIMARY KEY REFERENCES cost_item(code),
  sort_order  INTEGER NOT NULL UNIQUE CHECK (sort_order >= 1)
);
