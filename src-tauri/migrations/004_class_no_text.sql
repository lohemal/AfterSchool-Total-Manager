-- 학생의 반을 숫자에서 문자로 바꾼다.
--
-- 학교마다 반 이름이 다르다. '1반 2반'을 쓰는 곳도 있고 '가 나 다'나
-- '해 달 별'을 쓰는 곳도 있다. 숫자로 못 박아 두면 뒤쪽 학교는 아예 쓸 수 없다.
--
-- ## 기존 자료는 뜻이 바뀌지 않는다
--
-- 숫자 1은 문자 '1'이 된다. 화면에 보이는 것도, 엑셀에 나가는 것도 그대로다.
-- 학생 id 를 그대로 옮기므로 수강·지원대상자·정산과의 연결도 끊기지 않는다.
--
-- ## 표를 다시 만드는 까닭
--
-- SQLite 는 컬럼 타입을 바꾸지 못한다. 게다가 INTEGER 자리에 '1'을 넣으면
-- 타입 친화성 때문에 숫자 1 로 되돌아가 버려서, 문자와 숫자가 섞인 자료가 된다.
-- 그래서 SQLite 공식 절차대로 새 표를 만들고 옮긴 뒤 갈아 끼운다.
--
-- **이 마이그레이션은 외래키를 끈 채로 돌아야 한다.** enrollment ·
-- support_eligibility · support_grant 가 student(id) 를 ON DELETE CASCADE 로
-- 참조하므로, 외래키를 켠 채 DROP TABLE 하면 수강 자료까지 함께 지워진다.
-- migrate.rs 의 `fk_off` 가 이것을 맡는다.
--
-- ## 정렬
--
-- class_sort 는 정렬만을 위한 값이다.
--   숫자 반  '1' → '0000001'   '10' → '0000010'   (자연 정렬)
--   문자 반  '가' → '1가'
-- 앞자리 0/1 덕분에 숫자 반이 항상 문자 반보다 앞에 오고, 숫자끼리는
-- 1, 2, 3, 10 순서가 된다. GENERATED 라 손으로 갱신할 일이 없어 어긋나지 않는다.

CREATE TABLE student_new (
  id         INTEGER PRIMARY KEY,
  year_id    INTEGER NOT NULL REFERENCES academic_year(id) ON DELETE CASCADE,
  grade      INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 9),
  -- 숫자도 문자도 된다. 앞뒤 공백은 넣지 않는다 ('1 '과 '1'이 다른 반이 되면 안 된다).
  class_no   TEXT    NOT NULL CHECK (
               length(class_no) BETWEEN 1 AND 10 AND class_no = trim(class_no)
             ),
  student_no INTEGER NOT NULL CHECK (student_no BETWEEN 1 AND 99),
  name       TEXT    NOT NULL,
  note       TEXT    NOT NULL DEFAULT '',
  created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
  class_sort TEXT    GENERATED ALWAYS AS (
               CASE WHEN class_no GLOB '[0-9]*' AND NOT class_no GLOB '*[^0-9]*'
                    THEN '0' || substr('000000' || class_no, -6)
                    ELSE '1' || class_no END
             ) STORED
);

INSERT INTO student_new (id, year_id, grade, class_no, student_no, name, note, created_at)
SELECT id, year_id, grade, CAST(class_no AS TEXT), student_no, name, note, created_at
  FROM student;

DROP TABLE student;
ALTER TABLE student_new RENAME TO student;

CREATE UNIQUE INDEX student_key_uq ON student(year_id, grade, class_no, student_no);
CREATE INDEX student_name_ix  ON student(year_id, name);
-- 목록은 거의 항상 이 차례로 뽑는다. class_no 까지 넣어 순서가 늘 하나로 정해진다.
CREATE INDEX student_class_ix ON student(year_id, grade, class_sort, class_no, student_no);

-- 표를 지우면 트리거도 함께 사라지므로 다시 만든다. 001 의 것과 같은 내용이다.
-- 학생은 '학년'이 바뀔 때만 올린다. 이름·반·번호 수정은 금액에 영향이 없는데
-- 그때마다 '다시 정산하세요' 경고가 뜨면 사람이 경고를 무시하게 된다.
CREATE TRIGGER yr_bump_student_grade AFTER UPDATE OF grade ON student
WHEN OLD.grade <> NEW.grade BEGIN
  UPDATE academic_year SET data_version = data_version + 1 WHERE id = NEW.year_id;
END;
