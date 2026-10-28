-- 방과후드림 결제관리 — 작업공간별 결제 기준월 (P2)
--
-- 방과후드림 결제파일의 I열(수강월, YYYYMM)과 맞대어 볼 값이다. 작업공간의 시작일 ·
-- 끝일로 짐작하지 않고 사람이 정한다. 작업공간이 달력 한 달이 아닐 수 있기 때문이다.
--
-- 결제 기준월은 정산의 원본이 아니라 방과후드림 내보내기 설정이다. 그래서 workspace
-- 본체에 칸을 더하지 않고 방과후드림 전용 표에 둔다. 이 표에는 자료판 트리거가 없어
-- 기준월을 바꿔도 작업공간 · 학년도 자료판이 오르지 않고 정산이 낡음이 되지 않는다.
-- 작업공간을 지우면 함께 지워진다(지우기를 막지 않는다).
--
-- 기존 표 · 기존 트리거는 고치지 않는다. 자료를 옮겨 넣지 않는다.

CREATE TABLE dream_workspace_setting (
  workspace_id   INTEGER PRIMARY KEY REFERENCES workspace(id) ON DELETE CASCADE,
  billing_month  TEXT    NOT NULL
                 CHECK (length(billing_month) = 6
                        AND billing_month GLOB '[0-9][0-9][0-9][0-9][0-9][0-9]'
                        AND substr(billing_month, 5, 2) BETWEEN '01' AND '12'),
  updated_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);
