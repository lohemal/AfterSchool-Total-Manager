//! 부서별 학생 금액 Excel 일괄 수정 시험 (v0.1.4).
//!
//! 가장 중요한 것 둘.
//!
//! 1. **고르지 않은 부서는 절대 바뀌지 않는다.**
//! 2. **같은 파일을 다시 올리면 아무 일도 일어나지 않는다** — 자료판이 올라가지
//!    않고, 이미 만든 정산이 낡음이 되지 않는다.

use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::excel::{dept_fees, read, write};
use crate::model::{CostItem, DepartmentInput, EnrollmentInput, Fee, StudentInput, WorkspaceInput};
use crate::repo;

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
const 교재비: &str = "TEXTBOOK";
const 재료비: &str = "MATERIAL";

fn fee(code: &str, amount: i64) -> Fee {
    Fee {
        item_code: code.to_string(),
        amount,
    }
}

fn 원래_금액() -> Vec<Fee> {
    vec![
        fee(강사료, 30_000),
        fee(수용비, 3_000),
        fee(교재비, 15_000),
        fee(재료비, 10_000),
    ]
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-deptfee-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct F {
    db: Db,
    year: i64,
    ws: i64,
    items: Vec<CostItem>,
    dir: PathBuf,
}

impl F {
    fn new(tag: &str) -> Self {
        let db = Db::open_memory().unwrap();
        let year = db
            .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
            .unwrap();
        let ws = db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    year,
                    &WorkspaceInput {
                        name: "2026년 4월".into(),
                        start_date: "2026-04-01".into(),
                        end_date: "2026-04-30".into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        let items = db.read(|c| repo::cost_items(c)).unwrap();
        Self {
            db,
            year,
            ws,
            items,
            dir: tmp_dir(tag),
        }
    }

    fn student(&self, grade: i64, class_no: &str, no: i64, name: &str) -> i64 {
        self.db
            .write(|c| {
                repo::student::create(
                    c,
                    self.year,
                    &StudentInput {
                        grade,
                        class_no: class_no.into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn dept(&self, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        self.db
            .write(|c| {
                repo::department::create(
                    c,
                    self.ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class_name.into()),
                        teacher: None,
                        days: None,
                        note: None,
                        fees,
                    },
                )
            })
            .unwrap()
    }

    fn enroll(&self, student: i64, dept: i64) -> i64 {
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
                    &EnrollmentInput {
                        student_id: student,
                        department_id: dept,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap()
    }

    /// 한 수강 건의 항목 금액.
    fn 금액(&self, e: i64, code: &str) -> i64 {
        self.db
            .read(|c| {
                let row = repo::enrollment::get(c, e, &self.items)?;
                Ok(row
                    .fees
                    .iter()
                    .find(|f| f.item_code == code)
                    .map(|f| f.amount)
                    .unwrap_or(0))
            })
            .unwrap()
    }

    fn 합계(&self, e: i64) -> i64 {
        self.db
            .read(|c| Ok(repo::enrollment::get(c, e, &self.items)?.total))
            .unwrap()
    }

    /// 작업공간 자료판 — 올라가면 정산이 낡음이 된다.
    fn 자료판(&self) -> i64 {
        self.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT data_version FROM workspace WHERE id = ?1",
                    rusqlite::params![self.ws],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn 변경이력_수(&self) -> i64 {
        self.db
            .read(|c| {
                Ok(c.query_row("SELECT COUNT(*) FROM change_log", [], |r| r.get(0))?)
            })
            .unwrap()
    }

    fn 정산(&self) {
        self.db.write(|c| repo::settle::generate(c, self.ws)).unwrap();
    }

    fn 정산상태(&self) -> String {
        self.db
            .read(|c| repo::settle::status(c, self.ws))
            .unwrap()
            .state
    }

    fn 양식(&self, dept: i64) -> PathBuf {
        let made = self
            .db
            .read(|c| {
                dept_fees::template(c, self.ws, dept, &self.items, &["2026학년도"], &self.dir)
            })
            .unwrap();
        PathBuf::from(&made.path)
    }

    fn 미리보기(&self, dept: i64, path: &Path) -> (crate::model::FeePreview, Vec<repo::enrollment::StudentFeeEdit>) {
        self.db
            .read(|c| dept_fees::preview(c, self.ws, dept, &self.items, path))
            .unwrap()
    }

    fn 반영(
        &self,
        dept: i64,
        edits: &[repo::enrollment::StudentFeeEdit],
        reason: &str,
    ) -> crate::model::FeeApplyResult {
        self.db
            .write(|c| dept_fees::apply(c, self.ws, dept, edits, reason, &self.items))
            .unwrap()
    }
}

/// 사람이 손으로 친 파일 — **모든 칸이 문자 셀**이다.
///
/// 빈 칸과 `삼만원` 같은 값이 그대로 남아야 검증을 시험할 수 있다. 금액 열로
/// 넣으면 writer 가 숫자로 바꾸면서 빈 칸이 0이 되어 버린다.
///
/// 반이 문자 셀이라는 점이 특히 중요하다 — 숫자로 두면 `'01'`이 1이 되어
/// `'1'`반과 구별되지 않는다. 실제 양식도 반을 문자로 쓴다.
fn 입력파일(dir: &Path, name: &str, rows: &[Vec<&str>]) -> PathBuf {
    let head = ["학년", "반", "번호", "이름", "강사료", "수용비", "교재비", "재료비"];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    입력파일_칸(dir, name, &head, &body, &[])
}

/// 금액을 **숫자 셀**로 쓴 파일. 양식을 받아 고친 경우와 같다.
fn 입력파일_숫자(dir: &Path, name: &str, rows: &[Vec<&str>]) -> PathBuf {
    let head = ["학년", "반", "번호", "이름", "강사료", "수용비", "교재비", "재료비"];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    입력파일_칸(dir, name, &head, &body, &[4, 5, 6, 7])
}

fn 입력파일_칸(
    dir: &Path,
    name: &str,
    head: &[&str],
    body: &[Vec<String>],
    money: &[usize],
) -> PathBuf {
    let path = dir.join(format!("{name}.xlsx"));
    let widths = vec![write::DEFAULT_WIDTH; head.len()];
    write::write_sheet_sized(&path, "금액 수정", head, body, money, None, &widths, false).unwrap();
    path
}

// ─────────────────────────────────── 양식

#[test]
fn 양식에_지금_수강생과_지금_금액이_담긴다() {
    let f = F::new("template");
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "나리", 2, "이두리");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);
    f.enroll(b, d);

    let path = f.양식(d);
    let sheet = read::read_first_sheet(&path).unwrap();
    for name in ["학년", "반", "번호", "이름", "강사료", "수용비", "교재비", "재료비"] {
        assert!(sheet.require(name).is_ok(), "'{name}' 열이 없다");
    }
    assert_eq!(sheet.rows.len(), 2);
    let (_, c) = &sheet.rows[0];
    assert_eq!(sheet.cell(c, sheet.col("이름")), "김하나");
    assert_eq!(sheet.cell(c, sheet.col("반")), "가람");
    assert_eq!(sheet.cell(c, sheet.col("강사료")), "30000");

    let name = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.contains("금액수정양식"), "{name}");
    assert!(name.contains("로봇과학A반"), "{name}");
}

#[test]
fn 양식에는_고른_부서만_담긴다() {
    let f = F::new("template-scope");
    let a = f.student(1, "가람", 1, "김하나");
    let 로봇 = f.dept("로봇과학", "A반", 원래_금액());
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000)]);
    f.enroll(a, 로봇);
    f.enroll(a, 미술);

    let sheet = read::read_first_sheet(&f.양식(로봇)).unwrap();
    assert_eq!(sheet.rows.len(), 1, "로봇과학 수강 한 건만");
}

#[test]
fn 양식을_그대로_다시_올리면_바뀔_것이_없다() {
    let f = F::new("roundtrip");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);

    let path = f.양식(d);
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(p.unmatched.is_empty(), "{:?}", p.unmatched);
    assert_eq!(p.students, 0, "바뀔 학생이 없어야 한다");
    assert_eq!(p.cells, 0);
    assert_eq!(p.unchanged, 1);
    assert!(edits.is_empty());
}

// ─────────────────────────────────── 일부만 담긴 파일

#[test]
fn 한_명만_담긴_파일은_그_한_명만_고친다() {
    let f = F::new("partial");
    let 대상 = f.student(1, "가람", 1, "홍길동");
    let 그대로 = f.student(1, "가람", 2, "이두리");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e1 = f.enroll(대상, d);
    let e2 = f.enroll(그대로, d);

    let path = 입력파일(
        &f.dir,
        "one",
        &[vec!["1", "가람", "1", "홍길동", "25000", "3000", "15000", "15000"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert_eq!(p.total, 1);
    assert_eq!(p.students, 1);
    assert_eq!(p.cells, 2, "강사료와 재료비 두 칸");

    let 바뀜: Vec<(&str, i64, i64)> = p
        .changes
        .iter()
        .map(|c| (c.item_name.as_str(), c.before, c.after))
        .collect();
    assert!(바뀜.contains(&("강사료", 30_000, 25_000)), "{바뀜:?}");
    assert!(바뀜.contains(&("재료비", 10_000, 15_000)), "{바뀜:?}");

    let r = f.반영(d, &edits, "학생별 조정");
    assert_eq!(r.students, 1);
    assert_eq!(r.cells, 2);

    assert_eq!(f.금액(e1, 강사료), 25_000);
    assert_eq!(f.금액(e1, 재료비), 15_000);
    assert_eq!(f.금액(e1, 수용비), 3_000, "안 바꾼 칸은 그대로");
    // 파일에 없던 학생은 손대지 않는다
    assert_eq!(f.합계(e2), 58_000, "다른 학생이 바뀌었다");
}

#[test]
fn 여러_학생을_한꺼번에_고친다() {
    let f = F::new("many");
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    let c = f.student(1, "나리", 1, "박세찬");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e1 = f.enroll(a, d);
    let e2 = f.enroll(b, d);
    let e3 = f.enroll(c, d);

    let path = 입력파일_숫자(
        &f.dir,
        "many",
        &[
            vec!["1", "가람", "1", "김하나", "10000", "0", "0", "0"],
            vec!["1", "가람", "2", "이두리", "20000", "3000", "15000", "10000"],
            vec!["1", "나리", "1", "박세찬", "30000", "3000", "15000", "10000"],
        ],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert_eq!(p.total, 3);
    assert_eq!(p.students, 2, "박세찬은 값이 같다");
    assert_eq!(p.unchanged, 1);

    f.반영(d, &edits, "일괄 조정");
    assert_eq!(f.합계(e1), 10_000);
    assert_eq!(f.합계(e2), 48_000);
    assert_eq!(f.합계(e3), 58_000, "값이 같은 학생은 그대로");
}

// ─────────────────────────────────── 아무 일도 없어야 하는 경우

#[test]
fn 같은_파일을_다시_올리면_자료판이_올라가지_않는다() {
    let f = F::new("noop");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "noop",
        &[vec!["1", "가람", "1", "김하나", "25000", "3000", "15000", "10000"]],
    );

    // 첫 번째 — 실제로 바뀐다
    let (_, edits) = f.미리보기(d, &path);
    f.반영(d, &edits, "조정");

    // 정산을 만들어 '최신'으로 둔다
    f.정산();
    assert_eq!(f.정산상태(), "FRESH");
    let 판 = f.자료판();
    let 이력 = f.변경이력_수();

    // 두 번째 — 같은 파일
    let (p, edits) = f.미리보기(d, &path);
    assert_eq!(p.students, 0, "바뀔 것이 없어야 한다");
    assert_eq!(p.cells, 0);
    assert_eq!(p.unchanged, 1);
    assert!(edits.is_empty());
    assert!(p.token.is_empty(), "반영할 것이 없으면 토큰을 주지 않는다");

    // 빈 목록으로 반영해도 아무 일이 없어야 한다
    let r = f.반영(d, &edits, "조정");
    assert_eq!(r.students, 0);
    assert_eq!(r.cells, 0);

    assert_eq!(f.자료판(), 판, "자료판이 올라갔다");
    assert_eq!(f.정산상태(), "FRESH", "정산이 낡음이 되었다");
    assert_eq!(f.변경이력_수(), 이력, "이력이 늘었다");
}

#[test]
fn 실제로_바뀌면_정산이_낡음이_되고_이력이_남는다() {
    let f = F::new("stale");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);
    f.정산();
    assert_eq!(f.정산상태(), "FRESH");
    let 판 = f.자료판();
    let 이력 = f.변경이력_수();

    let path = 입력파일(
        &f.dir,
        "stale",
        &[vec!["1", "가람", "1", "김하나", "40000", "3000", "15000", "10000"]],
    );
    let (_, edits) = f.미리보기(d, &path);
    f.반영(d, &edits, "강사료 인상");

    assert!(f.자료판() > 판, "자료판이 올라가야 한다");
    assert_ne!(f.정산상태(), "FRESH", "재정산 필요가 되어야 한다");
    assert_eq!(f.변경이력_수(), 이력 + 1, "이력이 한 줄 남아야 한다");

    let (_, before, after, reason) = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT target, before_value, after_value, reason
                   FROM change_log ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )?)
        })
        .unwrap();
    assert!(before.contains("30,000"), "{before}");
    assert!(after.contains("40,000"), "{after}");
    assert_eq!(reason, "강사료 인상");
}

// ─────────────────────────────────── 걸러 내는 것

#[test]
fn 이름이_다르면_짝을_짓지_않는다() {
    let f = F::new("name");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    // 학년·반·번호는 맞지만 이름이 다르다 — 추측하지 않는다
    let path = 입력파일(
        &f.dir,
        "name",
        &[vec!["1", "가람", "1", "김하나111", "1000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert_eq!(p.unmatched.len(), 1, "매칭 실패로 표시해야 한다");
    assert!(p.unmatched[0].message.contains("로봇과학A반"), "{:?}", p.unmatched[0]);
    assert!(edits.is_empty());
    assert_eq!(f.합계(e), 58_000, "건드리면 안 된다");
}

#[test]
fn 다른_부서_학생은_짝을_짓지_않는다() {
    let f = F::new("other-dept");
    let a = f.student(1, "가람", 1, "김하나");
    let 로봇 = f.dept("로봇과학", "A반", 원래_금액());
    let 미술 = f.dept("미술", "A반", vec![fee(강사료, 20_000)]);
    let e미술 = f.enroll(a, 미술);

    // 로봇과학을 고른 채 미술만 듣는 학생을 올린다
    let path = 입력파일(
        &f.dir,
        "other",
        &[vec!["1", "가람", "1", "김하나", "1000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(로봇, &path);
    assert_eq!(p.unmatched.len(), 1);
    assert!(edits.is_empty());
    assert_eq!(f.합계(e미술), 20_000, "미술 금액이 바뀌었다");
}

#[test]
fn 같은_학생이_두_번_있으면_오류다() {
    let f = F::new("dup");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "dup",
        &[
            vec!["1", "가람", "1", "김하나", "10000", "0", "0", "0"],
            vec!["1", "가람", "1", "김하나", "20000", "0", "0", "0"],
        ],
    );
    let (p, _) = f.미리보기(d, &path);
    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert!(p.errors[0].message.contains("두 번"), "{:?}", p.errors[0]);
    assert!(p.token.is_empty(), "오류가 있으면 반영할 수 없다");
    assert_eq!(f.합계(e), 58_000);
}

#[test]
fn 빈_금액_칸은_오류다() {
    let f = F::new("blank");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    // 재료비를 비웠다 — 0원인지 그대로인지 짐작하지 않는다
    let path = 입력파일(
        &f.dir,
        "blank",
        &[vec!["1", "가람", "1", "김하나", "25000", "3000", "15000", ""]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert!(p.errors[0].message.contains("재료비"), "{:?}", p.errors[0]);
    assert!(p.errors[0].message.contains("0을 적어"), "{:?}", p.errors[0]);
    assert!(edits.is_empty());
    assert_eq!(f.합계(e), 58_000);
}

#[test]
fn 음수_금액은_오류다() {
    let f = F::new("neg");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "neg",
        &[vec!["1", "가람", "1", "김하나", "-1000", "0", "0", "0"]],
    );
    let (p, _) = f.미리보기(d, &path);
    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert!(p.errors[0].message.contains("0원 이상"), "{:?}", p.errors[0]);
    assert_eq!(f.합계(e), 58_000);
}

#[test]
fn 숫자가_아닌_금액은_오류다() {
    let f = F::new("nan");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "nan",
        &[vec!["1", "가람", "1", "김하나", "삼만원", "0", "0", "0"]],
    );
    let (p, _) = f.미리보기(d, &path);
    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert!(p.errors[0].message.contains("강사료"), "{:?}", p.errors[0]);
}

#[test]
fn 오류가_한_줄이라도_있으면_다른_줄도_반영하지_않는다() {
    let f = F::new("allornothing");
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e1 = f.enroll(a, d);
    let e2 = f.enroll(b, d);

    let path = 입력파일(
        &f.dir,
        "mixed",
        &[
            vec!["1", "가람", "1", "김하나", "10000", "0", "0", "0"],
            vec!["1", "가람", "2", "이두리", "", "0", "0", "0"],
        ],
    );
    let (p, _) = f.미리보기(d, &path);
    assert_eq!(p.errors.len(), 1);
    assert_eq!(p.students, 1, "정상인 줄은 세어 두지만");
    assert!(p.token.is_empty(), "반영은 막는다");
    // 화면이 토큰 없이는 반영을 부를 수 없다. DB 는 그대로다.
    assert_eq!(f.합계(e1), 58_000);
    assert_eq!(f.합계(e2), 58_000);
}

// ─────────────────────────────────── 반은 문자다

#[test]
fn 한글_반도_그대로_짝을_짓는다() {
    let f = F::new("hangul");
    let a = f.student(1, "해", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "hangul",
        &[vec!["1", "해", "1", "김하나", "12000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(p.unmatched.is_empty(), "{:?}", p.unmatched);
    f.반영(d, &edits, "조정");
    assert_eq!(f.금액(e, 강사료), 12_000);
}

#[test]
fn 영일반과_일반은_서로_다른_반이다() {
    let f = F::new("zero-one");
    let 영일 = f.student(1, "01", 1, "김하나");
    let 일 = f.student(1, "1", 1, "이두리");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e영일 = f.enroll(영일, d);
    let e일 = f.enroll(일, d);

    // '01' 반만 고친다
    let path = 입력파일(
        &f.dir,
        "zero-one",
        &[vec!["1", "01", "1", "김하나", "11000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert_eq!(p.students, 1);
    f.반영(d, &edits, "조정");

    assert_eq!(f.금액(e영일, 강사료), 11_000, "'01' 반이 바뀌어야 한다");
    assert_eq!(f.금액(e일, 강사료), 30_000, "'1' 반이 바뀌면 안 된다");
}

#[test]
fn 반_앞뒤_공백만_다듬는다() {
    let f = F::new("trim");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "trim",
        &[vec!["1", " 가람 ", "1", " 김하나 ", "13000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert!(p.unmatched.is_empty(), "{:?}", p.unmatched);
    f.반영(d, &edits, "조정");
    assert_eq!(f.금액(e, 강사료), 13_000);
}

// ─────────────────────────────────── 다른 부서 · 취소 건

#[test]
fn 고르지_않은_부서는_한_칸도_바뀌지_않는다() {
    let f = F::new("scope");
    let a = f.student(1, "가람", 1, "김하나");
    let 로봇 = f.dept("로봇과학", "A반", 원래_금액());
    let 미술 = f.dept("미술", "A반", 원래_금액());
    let e로봇 = f.enroll(a, 로봇);
    let e미술 = f.enroll(a, 미술);

    let path = 입력파일(
        &f.dir,
        "scope",
        &[vec!["1", "가람", "1", "김하나", "1000", "0", "0", "0"]],
    );
    let (_, edits) = f.미리보기(로봇, &path);
    f.반영(로봇, &edits, "조정");

    assert_eq!(f.합계(e로봇), 1_000);
    assert_eq!(f.합계(e미술), 58_000, "미술은 그대로여야 한다");
}

#[test]
fn 다른_부서_수강_건을_넘기면_거절한다() {
    let f = F::new("guard");
    let a = f.student(1, "가람", 1, "김하나");
    let 로봇 = f.dept("로봇과학", "A반", 원래_금액());
    let 미술 = f.dept("미술", "A반", 원래_금액());
    f.enroll(a, 로봇);
    let e미술 = f.enroll(a, 미술);

    // 미술 수강 건을 로봇과학 반영에 섞어 넣는다 (화면에서 부서를 바꾼 상황)
    let 섞음 = vec![repo::enrollment::StudentFeeEdit {
        enrollment_id: e미술,
        fees: vec![fee(강사료, 1)],
    }];
    let err = f
        .db
        .write(|c| dept_fees::apply(c, f.ws, 로봇, &섞음, "조정", &f.items))
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("고른 부서"),
        "부서 밖 자료를 막아야 한다: {err:?}"
    );
    assert_eq!(f.합계(e미술), 58_000, "되돌아가야 한다");
}

#[test]
fn 취소한_수강은_양식과_대상에서_빠진다() {
    let f = F::new("cancelled");
    let a = f.student(1, "가람", 1, "김하나");
    let b = f.student(1, "가람", 2, "이두리");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e취소 = f.enroll(a, d);
    f.enroll(b, d);
    f.db
        .write(|c| {
            repo::enrollment::cancel(
                c,
                e취소,
                Some(&[fee(강사료, 15_000), fee(수용비, 0), fee(교재비, 15_000), fee(재료비, 10_000)]),
                "중도 포기",
                &f.items,
            )
        })
        .unwrap();

    // 양식에는 수강중인 학생만 담긴다
    let sheet = read::read_first_sheet(&f.양식(d)).unwrap();
    assert_eq!(sheet.rows.len(), 1);
    assert_eq!(sheet.cell(&sheet.rows[0].1, sheet.col("이름")), "이두리");

    // 취소한 학생을 파일에 적어도 짝을 짓지 않는다
    let path = 입력파일(
        &f.dir,
        "cancelled",
        &[vec!["1", "가람", "1", "김하나", "1000", "0", "0", "0"]],
    );
    let (p, edits) = f.미리보기(d, &path);
    assert_eq!(p.unmatched.len(), 1, "취소 건은 대상이 아니다");
    assert!(edits.is_empty());
    // 취소할 때 확정한 금액이 그대로 남아 있다
    assert_eq!(f.합계(e취소), 40_000);
}

// ─────────────────────────────────── 사유

#[test]
fn 변경사유가_없으면_반영하지_않는다() {
    let f = F::new("reason");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    let e = f.enroll(a, d);

    let path = 입력파일(
        &f.dir,
        "reason",
        &[vec!["1", "가람", "1", "김하나", "1000", "0", "0", "0"]],
    );
    let (_, edits) = f.미리보기(d, &path);
    let err = f
        .db
        .write(|c| dept_fees::apply(c, f.ws, d, &edits, "   ", &f.items))
        .unwrap_err();
    assert!(format!("{err:?}").contains("변경사유"), "{err:?}");
    assert_eq!(f.합계(e), 58_000, "되돌아가야 한다");
}

#[test]
fn 열이_빠진_파일은_읽지_않는다() {
    let f = F::new("cols");
    let a = f.student(1, "가람", 1, "김하나");
    let d = f.dept("로봇과학", "A반", 원래_금액());
    f.enroll(a, d);

    // 재료비 열이 없는 파일
    let path = f.dir.join("missing.xlsx");
    let head = ["학년", "반", "번호", "이름", "강사료", "수용비", "교재비"];
    let body = vec![vec![
        "1".to_string(),
        "가람".to_string(),
        "1".to_string(),
        "김하나".to_string(),
        "1000".to_string(),
        "0".to_string(),
        "0".to_string(),
    ]];
    let widths = vec![write::DEFAULT_WIDTH; head.len()];
    write::write_sheet_sized(&path, "금액 수정", &head, &body, &[4, 5, 6], None, &widths, false)
        .unwrap();

    let err = f
        .db
        .read(|c| dept_fees::preview(c, f.ws, d, &f.items, &path))
        .unwrap_err();
    assert!(format!("{err:?}").contains("재료비"), "{err:?}");
}
