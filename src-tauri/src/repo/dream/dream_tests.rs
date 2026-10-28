//! 방과후드림 결제관리 P1 시험. 학생 · 번호 · 생년월일은 모두 가짜다.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::crypto::{self, keys, DataKey};
use crate::db::Db;
use crate::domain::Program;
use crate::model::{
    CostItem, DepartmentInput, EligibilityInput, EnrollmentInput, Fee, GrantInput, StudentInput,
    WorkspaceInput,
};
use crate::repo;
use crate::repo::dream::{amount, course, identity, job, priority, setting};

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

struct F {
    db: Db,
    year: i64,
    ws: i64,
    items: Vec<CostItem>,
    st: HashMap<&'static str, i64>,
    dp: HashMap<&'static str, i64>,
}

impl F {
    fn new() -> Self {
        let db = Db::open_memory().unwrap();
        let year = db
            .write(|c| repo::year::create_year(c, 2026, "2026학년도"))
            .unwrap();
        let items = db.read(repo::cost_items).unwrap();
        for p in [Program::Voucher, Program::FreeVoucher] {
            db.write(|c| repo::policy::save(c, year, p.code(), 1_000_000, false, "", &[]))
                .unwrap();
        }
        let mut f = Self {
            db,
            year,
            ws: 0,
            items,
            st: HashMap::new(),
            dp: HashMap::new(),
        };
        f.ws = f.workspace("10월", "2026-10-01", "2026-10-31");
        f
    }

    fn workspace(&self, name: &str, start: &str, end: &str) -> i64 {
        self.db
            .write(|c| {
                repo::year::create_workspace(
                    c,
                    self.year,
                    &WorkspaceInput {
                        name: name.into(),
                        start_date: start.into(),
                        end_date: end.into(),
                        note: None,
                    },
                )
            })
            .unwrap()
    }

    fn student(&mut self, key: &'static str, grade: i64, class_no: &str, no: i64, name: &str) -> i64 {
        let year = self.year;
        let id = self
            .db
            .write(|c| {
                repo::student::create(
                    c,
                    year,
                    &StudentInput {
                        grade,
                        class_no: class_no.into(),
                        student_no: no,
                        name: name.into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        self.st.insert(key, id);
        id
    }

    fn dept_in(&mut self, ws: i64, key: &'static str, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        let id = self
            .db
            .write(|c| {
                repo::department::create(
                    c,
                    ws,
                    &DepartmentInput {
                        name: name.into(),
                        class_name: Some(class_name.into()),
                        teacher: None,
                        days: None,
                        note: None,
                        fees,
                        capacity: None,
                        allowed_grades: Vec::new(),
                        custom_sessions: 0,
                    },
                )
            })
            .unwrap();
        self.dp.insert(key, id);
        id
    }

    fn dept(&mut self, key: &'static str, name: &str, class_name: &str, fees: Vec<Fee>) -> i64 {
        let ws = self.ws;
        self.dept_in(ws, key, name, class_name, fees)
    }

    fn enroll_in(&self, ws: i64, student: &str, dept: &str) {
        let (s, d) = (self.st[student], self.dp[dept]);
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    ws,
                    &EnrollmentInput {
                        student_id: s,
                        department_id: d,
                        fees: Vec::new(),
                        reason: None,
                    },
                    &self.items,
                )
            })
            .unwrap();
    }

    fn enroll(&self, student: &str, dept: &str) {
        self.enroll_in(self.ws, student, dept)
    }

    fn support(&self, student: &str, program: Program, limit: i64) {
        let s = self.st[student];
        self.db
            .write(|c| {
                repo::eligibility::create(
                    c,
                    self.year,
                    &EligibilityInput {
                        student_id: s,
                        program: program.code().into(),
                        valid_from: None,
                        valid_to: None,
                        note: None,
                    },
                )?;
                repo::policy::grant_save(
                    c,
                    self.year,
                    &GrantInput {
                        student_id: s,
                        program: program.code().into(),
                        period_id: None,
                        amount: limit,
                        reason: Some("시험".into()),
                    },
                )
            })
            .unwrap();
    }

    fn generate_in(&self, ws: i64) {
        self.db.write(|c| repo::settle::generate(c, ws)).unwrap();
    }

    fn generate(&self) {
        self.generate_in(self.ws)
    }

    fn state(&self) -> String {
        self.db.read(|c| repo::settle::status(c, self.ws)).unwrap().state
    }

    fn key(&self) -> DataKey {
        self.db.write(|c| keys::current_or_create(c)).unwrap()
    }

    /// 네 재원이 모두 나오는 작업공간 — 정산까지 만든다.
    fn mixed() -> Self {
        let mut f = Self::new();
        f.dept("로봇", "로봇과학", "B", vec![fee(강사료, 35_000), fee(수용비, 1_500)]);
        f.dept(
            "만화",
            "만화애니메이션",
            "B",
            vec![fee(강사료, 35_000), fee(수용비, 1_500), fee(재료비, 22_000)],
        );
        f.dept("미술", "창의미술", "B", vec![fee(강사료, 28_000), fee(수용비, 1_200)]);
        f.dept(
            "공예",
            "토탈공예미니어처",
            "B",
            vec![fee(강사료, 35_000), fee(수용비, 1_500), fee(재료비, 28_750)],
        );
        f.student("A", 3, "1", 1, "가온"); //  수익자만
        f.student("B", 3, "1", 2, "나래"); //  이용권 지원만
        f.student("C", 3, "1", 3, "다솜"); //  이용권 지원 + 초과
        f.student("D", 3, "1", 4, "라온"); //  이용권 + 자유수강권
        f.student("I", 6, "가", 1, "아라"); // 자유수강권 소진 뒤 수익자
        f.student("J", 6, "가", 2, "자람"); // 이용권 + 자유수강권 + 초과
        f.enroll("A", "로봇");
        f.enroll("B", "만화");
        f.enroll("C", "미술");
        f.enroll("D", "공예");
        f.enroll("I", "로봇");
        f.enroll("J", "로봇");
        f.support("B", Program::Voucher, 58_500);
        f.support("C", Program::Voucher, 3_400);
        f.support("D", Program::Voucher, 41_150);
        f.support("D", Program::FreeVoucher, 24_100);
        f.support("I", Program::FreeVoucher, 10_000);
        f.support("J", Program::Voucher, 1_000);
        f.support("J", Program::FreeVoucher, 2_000);
        f.generate();
        assert_eq!(f.state(), "FRESH");
        f
    }
}

/// 방과후드림 표를 뺀 업무 자료 전체의 지문. 자료판 · 정산 상태도 함께.
fn fingerprint(c: &Connection, ws: i64) -> Vec<String> {
    let tables: Vec<String> = c
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'dream_%' AND name NOT LIKE 'sqlite_%'
              ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut out = Vec::new();
    for t in tables {
        let mut st = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        // 차례를 믿지 않고 줄을 정렬해 견준다 (rowid 가 없는 표도 있다)
        let mut lines = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            let mut line = String::new();
            for i in 0..n {
                line.push_str(&format!("{:?}|", r.get_ref(i).unwrap()));
            }
            lines.push(line);
        }
        lines.sort();
        out.push(format!("{t}:{}", crypto::sha256_hex(lines.join("\n").as_bytes())));
    }
    out.push(repo::settle::status(c, ws).unwrap().state);
    out
}

fn fk_violations(c: &Connection) -> i64 {
    c.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))
        .unwrap()
}

// ─────────────────────────────────────────────── 기준 금액

#[test]
fn 기준액은_재원별_합과_맞고_개인부담은_수익자와_초과금이다() {
    let f = F::mixed();
    let (fresh, all) = f.db.read(|c| amount::workspace_amounts(c, f.ws)).unwrap();
    let by: HashMap<i64, amount::StudentAmount> = all.into_iter().map(|a| (a.student_id, a)).collect();
    let get = |k: &str| by[&f.st[k]].clone();

    for a in by.values() {
        assert_eq!(a.total, a.voucher + a.free_voucher + a.personal);
        assert_eq!(a.personal, a.self_pay + a.voucher_over);
    }
    let a = get("A");
    assert_eq!((a.total, a.voucher, a.free_voucher, a.personal), (36_500, 0, 0, 36_500));
    let b = get("B");
    assert_eq!((b.total, b.voucher, b.personal), (58_500, 58_500, 0));
    let c = get("C");
    assert_eq!((c.total, c.voucher, c.personal, c.voucher_over), (29_200, 3_400, 25_800, 25_800));
    let d = get("D");
    assert_eq!((d.voucher, d.free_voucher, d.personal), (41_150, 24_100, 0));
    let i = get("I");
    assert_eq!((i.free_voucher, i.personal, i.self_pay), (10_000, 26_500, 26_500));
    let j = get("J");
    assert_eq!((j.voucher, j.free_voucher, j.voucher_over, j.personal), (1_000, 2_000, 33_500, 33_500));

    // 정산 전체와 같다
    let s = f
        .db
        .read(|c| repo::settle::summary(c, f.ws, &f.items))
        .unwrap()
        .unwrap();
    let t: i64 = by.values().map(|a| a.total).sum();
    let v: i64 = by.values().map(|a| a.voucher).sum();
    let fv: i64 = by.values().map(|a| a.free_voucher).sum();
    assert_eq!(t, s.total.total);
    assert_eq!(v, s.total.voucher);
    assert_eq!(fv, s.total.free_voucher);
    assert_eq!(fresh.settlement_id, f.db.read(|c| repo::settle::require_fresh(c, f.ws)).unwrap());
}

#[test]
fn 이용권_초과금은_지원금에_다시_더하지_않는다() {
    let f = F::mixed();
    let (_, c) = f.db.read(|x| amount::student_amount(x, f.ws, f.st["C"])).unwrap();
    assert_eq!(c.voucher, 3_400, "초과금이 이용권 지원으로 들어갔다");
    assert_eq!(c.voucher_over, 25_800);
    assert_eq!(c.personal, 25_800);
}

#[test]
fn 한도_기록과_어긋나면_고쳐_맞추지_않고_오류다() {
    let f = F::mixed();
    let b = f.st["B"];
    f.db.write(|c| {
        c.execute(
            "UPDATE settlement_budget SET used_now = used_now - 1 WHERE student_id = ?1 AND program = 'VOUCHER'",
            [b],
        )?;
        Ok(())
    })
    .unwrap();
    let e = f.db.read(|c| amount::student_amount(c, f.ws, b)).unwrap_err();
    assert_eq!(e.code, "DREAM_AMOUNT_CONFLICT");
    let e = f.db.read(|c| amount::workspace_amounts(c, f.ws)).unwrap_err();
    assert_eq!(e.code, "DREAM_AMOUNT_CONFLICT");
}

#[test]
fn 정산_전과_낡은_정산은_모두_거절한다() {
    // NONE
    let mut f = F::new();
    f.dept("로봇", "로봇과학", "B", vec![fee(강사료, 35_000)]);
    f.student("A", 3, "1", 1, "가온");
    f.enroll("A", "로봇");
    let e = f.db.read(|c| amount::fresh_settlement(c, f.ws)).unwrap_err();
    assert!(e.message.contains("정산 전"), "{e}");

    // STALE_DATA — 금액이 바뀌었다
    f.generate();
    assert!(f.db.read(|c| amount::fresh_settlement(c, f.ws)).is_ok());
    f.dept("바둑", "바둑", "A", vec![fee(강사료, 20_000)]);
    assert_eq!(f.state(), "STALE_DATA");
    assert!(f.db.read(|c| amount::workspace_amounts(c, f.ws)).is_err());
    assert!(f.db.read(|c| amount::student_amount(c, f.ws, f.st["A"])).is_err());

    // STALE_YEAR — 지원 설정이 바뀌었다
    f.generate();
    f.support("A", Program::Voucher, 10_000);
    assert_eq!(f.state(), "STALE_YEAR");
    assert!(f.db.read(|c| amount::fresh_settlement(c, f.ws)).is_err());

    // STALE_PRIOR — 앞 작업공간을 다시 정산했다
    f.generate();
    let prior = f.workspace("9월", "2026-09-01", "2026-09-30");
    f.dept_in(prior, "9월로봇", "로봇과학", "B", vec![fee(강사료, 35_000)]);
    f.enroll_in(prior, "A", "9월로봇");
    f.generate_in(prior);
    f.generate();
    assert_eq!(f.state(), "FRESH");
    f.generate_in(prior);
    assert_eq!(f.state(), "STALE_PRIOR");
    assert!(f.db.read(|c| amount::fresh_settlement(c, f.ws)).is_err());
}

// ─────────────────────────────────────────────── 개인참조번호 · 생년월일

#[test]
fn 개인참조번호를_등록하고_찾고_필요할_때만_푼다() {
    let mut f = F::new();
    let a = f.student("A", 3, "가람", 1, "홍길동");
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, " I100000987 ")).unwrap();
    f.db.write(|c| identity::set_birth(c, &k, a, "170806")).unwrap();

    let st = f.db.read(|c| identity::state(c, a)).unwrap();
    assert!(st.has_ref && st.has_birth);
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000987")).unwrap(), Some(a));
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000988")).unwrap(), None);
    assert_eq!(f.db.read(|c| identity::reveal_ref(c, &k, a)).unwrap().unwrap().as_str(), "I100000987");
    assert_eq!(f.db.read(|c| identity::reveal_birth(c, &k, a)).unwrap().unwrap().as_str(), "170806");

    // 바꾸면 예전 번호로는 못 찾는다
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000999")).unwrap();
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000987")).unwrap(), None);
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000999")).unwrap(), Some(a));

    // 같은 키를 다시 받아도 같은 값을 찾는다 (DPAPI 사본을 다시 푼 키)
    let again = f.db.read(keys::current).unwrap().unwrap();
    assert_eq!(again.key_id, k.key_id);
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &again, f.year, "I100000999")).unwrap(), Some(a));
}

#[test]
fn 같은_학년도의_다른_학생에게_같은_번호를_줄_수_없다() {
    let mut f = F::new();
    let a = f.student("A", 3, "1", 1, "가온");
    let b = f.student("B", 3, "1", 2, "나래");
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();
    let e = f.db.write(|c| identity::set_ref(c, &k, b, "I100000987")).unwrap_err();
    assert_eq!(e.code, "CONFLICT");
    let all = format!("{} {:?}", e.message, e.detail);
    assert!(!all.contains("I100000987"), "오류에 번호가 나왔다: {all}");
    // 같은 학생에게 같은 번호를 다시 주는 것은 괜찮다
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();

    // 다른 학년도에는 같은 번호가 있어도 된다
    let y2 = f.db.write(|c| repo::year::create_year(c, 2027, "2027학년도")).unwrap();
    let other = f
        .db
        .write(|c| {
            repo::student::create(
                c,
                y2,
                &StudentInput {
                    grade: 4,
                    class_no: "1".into(),
                    student_no: 1,
                    name: "가온".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    f.db.write(|c| identity::set_ref(c, &k, other, "I100000987")).unwrap();
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, y2, "I100000987")).unwrap(), Some(other));
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000987")).unwrap(), Some(a));
}

#[test]
fn 형식이_틀린_값은_저장하지_않는다() {
    let mut f = F::new();
    let a = f.student("A", 3, "1", 1, "가온");
    let k = f.key();
    for bad in ["", "I-100", "가나다"] {
        assert!(f.db.write(|c| identity::set_ref(c, &k, a, bad)).is_err(), "{bad}");
    }
    for bad in ["20170806", "171306", "abcdef"] {
        assert!(f.db.write(|c| identity::set_birth(c, &k, a, bad)).is_err(), "{bad}");
    }
    let n: i64 = f
        .db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM dream_student_identity", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn 한_칸을_지우면_다른_칸은_남고_둘_다_지우면_줄이_없어진다() {
    let mut f = F::new();
    let a = f.student("A", 3, "1", 1, "가온");
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();
    f.db.write(|c| identity::set_birth(c, &k, a, "170806")).unwrap();
    f.db.write(|c| identity::clear_ref(c, a)).unwrap();
    let st = f.db.read(|c| identity::state(c, a)).unwrap();
    assert!(!st.has_ref && st.has_birth);
    assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000987")).unwrap(), None);
    f.db.write(|c| identity::clear_birth(c, a)).unwrap();
    let n: i64 = f
        .db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM dream_student_identity", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(n, 0);

    f.db.write(|c| identity::set_birth(c, &k, a, "170806")).unwrap();
    f.db.write(|c| identity::delete(c, a)).unwrap();
    assert_eq!(f.db.read(|c| identity::reveal_birth(c, &k, a)).unwrap(), None);
}

#[test]
fn 암호문을_다른_학생이나_다른_칸으로_옮기면_열리지_않는다() {
    let mut f = F::new();
    let a = f.student("A", 3, "1", 1, "가온");
    let b = f.student("B", 3, "1", 2, "나래");
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();
    f.db.write(|c| identity::set_birth(c, &k, a, "170806")).unwrap();
    f.db.write(|c| identity::set_birth(c, &k, b, "180101")).unwrap();
    f.db.write(|c| {
        // b 의 생년월일 자리에 a 의 것을 옮겨 붙인다
        c.execute(
            "UPDATE dream_student_identity
                SET birth_nonce = (SELECT birth_nonce FROM dream_student_identity WHERE student_id = ?1),
                    birth_cipher = (SELECT birth_cipher FROM dream_student_identity WHERE student_id = ?1)
              WHERE student_id = ?2",
            [a, b],
        )?;
        // a 의 생년월일 자리에 a 의 개인참조번호 암호문을 옮겨 붙인다
        c.execute(
            "UPDATE dream_student_identity SET birth_nonce = ref_nonce, birth_cipher = ref_cipher
              WHERE student_id = ?1",
            [a],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(f.db.read(|c| identity::reveal_birth(c, &k, b)).unwrap_err().code, "CRYPTO_TAMPERED");
    assert_eq!(f.db.read(|c| identity::reveal_birth(c, &k, a)).unwrap_err().code, "CRYPTO_TAMPERED");
}

#[test]
fn 다른_키로_저장된_줄은_다른_키로_풀거나_고칠_수_없다() {
    let mut f = F::new();
    let a = f.student("A", 3, "1", 1, "가온");
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();
    let stranger = DataKey::new(k.key_id.clone(), [9u8; crypto::KEY_LEN]);
    assert!(f.db.read(|c| identity::reveal_ref(c, &stranger, a)).is_err());
    assert_eq!(
        f.db.read(|c| identity::find_by_ref(c, &stranger, f.year, "I100000987")).unwrap(),
        None,
        "다른 키의 찾기 값이 맞았다"
    );
    let other_id = DataKey::new("other".into(), [9u8; crypto::KEY_LEN]);
    assert_eq!(
        f.db.write(|c| identity::set_birth(c, &other_id, a, "170806")).unwrap_err().code,
        "KEY_MISMATCH"
    );
}

#[test]
fn db_파일에_개인정보_평문과_키가_남지_않는다() {
    let dir = std::env::temp_dir().join("afterschool-dream-plain");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.db");
    let db = Db::open(&path).unwrap();
    let year = db.write(|c| repo::year::create_year(c, 2026, "2026학년도")).unwrap();
    let a = db
        .write(|c| {
            repo::student::create(
                c,
                year,
                &StudentInput {
                    grade: 3,
                    class_no: "1".into(),
                    student_no: 1,
                    name: "가온".into(),
                    note: None,
                },
            )
        })
        .unwrap();
    // 평문이 바이트 그대로 남는지 보기 쉽게 고유한 가짜 값을 쓴다
    let (r, b) = ("Q7W3E9R1T5Y2", "991231");
    let k = db.write(|c| keys::current_or_create(c)).unwrap();
    db.write(|c| identity::set_ref(c, &k, a, r)).unwrap();
    db.write(|c| identity::set_birth(c, &k, a, b)).unwrap();

    // 모든 칸을 글자로 훑는다
    let texts: Vec<String> = db
        .read(|c| {
            let mut st = c.prepare(
                "SELECT quote(student_id)||quote(year_id)||quote(key_id)||quote(ref_nonce)||quote(ref_cipher)
                        ||quote(ref_lookup)||quote(birth_nonce)||quote(birth_cipher) FROM dream_student_identity
                 UNION ALL SELECT quote(key_id)||quote(dpapi_blob) FROM dream_key",
            )?;
            let v = st.query_map([], |x| x.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(v)
        })
        .unwrap();
    let hex_of = |s: &str| crypto::hex(s.as_bytes()).to_uppercase();
    for t in &texts {
        assert!(!t.contains(r) && !t.contains(b), "평문이 칸에 있다");
        assert!(!t.contains(&hex_of(r)) && !t.contains(&hex_of(b)), "평문이 BLOB 에 그대로 있다");
    }
    drop(db);

    // 파일 바이트(본 파일 · -wal)에도 없다. 키 바이트도 없다.
    let key_bytes = *k.expose();
    for name in ["t.db", "t.db-wal"] {
        let p = dir.join(name);
        if let Ok(bytes) = std::fs::read(&p) {
            for needle in [r.as_bytes(), b.as_bytes(), &key_bytes[..]] {
                assert!(!bytes.windows(needle.len()).any(|w| w == needle), "{name} 에 평문 · 키가 있다");
            }
        }
    }
}

// ─────────────────────────────────────────────── 기관코드 · 강좌코드

#[test]
fn 기관코드를_저장하고_바꿔도_정산은_그대로다() {
    let f = F::mixed();
    let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
    assert_eq!(f.db.read(setting::org_code).unwrap(), None);
    assert_eq!(f.db.write(|c| setting::set_org_code(c, " 2024000001 ")).unwrap(), "2024000001");
    assert_eq!(f.db.read(setting::org_code).unwrap().as_deref(), Some("2024000001"));
    f.db.write(|c| setting::set_org_code(c, "0024000002")).unwrap();
    assert_eq!(f.db.read(setting::org_code).unwrap().as_deref(), Some("0024000002"), "앞자리 0");
    assert!(f.db.write(|c| setting::set_org_code(c, "24-01")).is_err());
    let after = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
    // app_setting 만 다르다
    let strip = |v: &[String]| v.iter().filter(|x| !x.starts_with("app_setting:")).cloned().collect::<Vec<_>>();
    assert_eq!(strip(&before), strip(&after));
    assert_eq!(f.state(), "FRESH");
}

#[test]
fn 강좌코드는_작업공간에서_한_부서에만_잇는다() {
    let mut f = F::mixed();
    let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
    let code = "20260818PGM000003317";
    f.db.write(|c| course::set(c, robot, code)).unwrap();
    assert_eq!(f.db.read(|c| course::find(c, f.ws, code)).unwrap(), Some(robot));
    let e = f.db.write(|c| course::set(c, art, code)).unwrap_err();
    assert_eq!(e.code, "CONFLICT");
    // 같은 값 다시 — 괜찮다
    f.db.write(|c| course::set(c, robot, code)).unwrap();
    // 부서의 코드를 바꾸면 예전 코드로는 못 찾는다
    f.db.write(|c| course::set(c, robot, "20260818PGM000009999")).unwrap();
    assert_eq!(f.db.read(|c| course::find(c, f.ws, code)).unwrap(), None);
    assert_eq!(f.db.read(|c| course::list(c, f.ws)).unwrap().len(), 1);
    // 다른 작업공간에서는 같은 코드를 쓸 수 있다
    let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
    let d2 = f.dept_in(ws2, "11월로봇", "로봇과학", "B", vec![fee(강사료, 35_000)]);
    f.db.write(|c| course::set(c, d2, code)).unwrap();
    assert_eq!(f.db.read(|c| course::find(c, ws2, code)).unwrap(), Some(d2));
    // 이름으로 짐작하지 않는다 — 형식이 틀린 코드는 거절
    assert!(f.db.write(|c| course::set(c, art, "로봇과학B")).is_err());
    f.db.write(|c| course::remove(c, robot)).unwrap();
    assert!(f.db.read(|c| course::list(c, f.ws)).unwrap().is_empty());
}

#[test]
fn 다른_작업공간_부서로_잇는_줄은_db_가_막는다() {
    let f = F::mixed();
    let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
    let robot = f.dp["로봇"];
    let e = f
        .db
        .write(|c| {
            c.execute(
                "INSERT INTO dream_course_map (workspace_id, department_id, course_code) VALUES (?1, ?2, 'X1')",
                [ws2, robot],
            )?;
            Ok(())
        })
        .unwrap_err();
    assert!(format!("{e:?}").contains("작업공간과 다릅니다"), "{e:?}");
    let a = f.st["A"];
    let k = f.key();
    let e = f
        .db
        .write(|c| {
            let y2 = repo::year::create_year(c, 2027, "2027학년도")?;
            let s = crypto::seal(&k, b"x", b"y")?;
            c.execute(
                "INSERT INTO dream_student_identity (student_id, year_id, key_id, birth_nonce, birth_cipher)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![a, y2, k.key_id, s.nonce, s.ciphertext],
            )?;
            Ok(())
        })
        .unwrap_err();
    assert!(format!("{e:?}").contains("학년도와 다릅니다"), "{e:?}");
}

// ─────────────────────────────────────────────── 우선순위

#[test]
fn 방과후드림_우선순위는_따로_저장되고_기존_정산_순위는_그대로다() {
    let mut f = F::mixed();
    let (robot, comic, art, craft) = (f.dp["로봇"], f.dp["만화"], f.dp["미술"], f.dp["공예"]);
    // 기존 정산 순위 하나를 정해 둔다(기본값으로 쓰이는지 보려고)
    f.db.write(|c| repo::priority::dept_save(c, f.ws, &[craft])).unwrap();
    f.generate();
    // 아래 '다른 작업공간 부서' 거절 시험에 쓸 부서 — 지문을 뜨기 전에 만든다
    let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
    let other = f.dept_in(ws2, "11월", "바둑", "A", vec![fee(강사료, 1)]);
    let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();

    // 미설정: 기존 정산 순위(공예) → 부서명 차례
    let order = f.db.read(|c| priority::dept_order(c, f.ws)).unwrap();
    assert_eq!(order, vec![craft, robot, comic, art], "로봇과학 < 만화애니메이션 < 창의미술");
    assert!(!f.db.read(|c| priority::has_dept_order(c, f.ws)).unwrap());

    // 설정: 로봇 → 미술. 나머지는 기본 규칙으로 뒤에
    f.db.write(|c| priority::save_dept_order(c, f.ws, &[robot, art])).unwrap();
    let order = f.db.read(|c| priority::dept_order(c, f.ws)).unwrap();
    assert_eq!(order, vec![robot, art, craft, comic]);
    assert_eq!(order, f.db.read(|c| priority::dept_order(c, f.ws)).unwrap(), "같은 자료면 같은 차례");

    // 부서마다 항목 차례가 다를 수 있다
    f.db.write(|c| priority::save_item_order(c, art, &[재료비.into(), 강사료.into()])).unwrap();
    assert_eq!(
        f.db.read(|c| priority::item_order(c, art)).unwrap(),
        vec![재료비, 강사료, 수용비, 교재비]
    );
    assert_eq!(
        f.db.read(|c| priority::item_order(c, robot)).unwrap(),
        vec![강사료, 수용비, 교재비, 재료비],
        "미설정 부서는 기존 차례"
    );

    // 잘못된 입력은 저장하지 않는다
    assert!(f.db.write(|c| priority::save_dept_order(c, f.ws, &[robot, robot])).is_err());
    assert!(f.db.write(|c| priority::save_dept_order(c, f.ws, &[other])).is_err());
    assert!(f.db.write(|c| priority::save_item_order(c, art, &["NOPE".into()])).is_err());
    assert!(f.db.write(|c| priority::save_item_order(c, art, &[강사료.into(), 강사료.into()])).is_err());

    // 기존 정산 순위 · 정산 · 자료판은 그대로
    let after = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
    assert_eq!(after, before, "기존 표 · 자료판 · 정산 상태가 바뀌었다");
    assert_eq!(f.db.read(|c| repo::priority::dept_order(c, f.ws)).unwrap(), vec![craft]);
    assert_eq!(f.state(), "FRESH");
}

// ─────────────────────────────────────────────── 처리 이력

fn new_job(c: &Connection, ws: i64, fresh: Option<&amount::FreshSettlement>) -> crate::error::AppResult<i64> {
    job::create(
        c,
        &job::NewJob {
            workspace_id: ws,
            source_name: "방과후드림 결제 파일.xlsx",
            source_sha256: &"ab".repeat(32),
            source_size: 37_463,
            settlement: fresh,
            app_version: "0.1.10",
        },
    )
}

#[test]
fn 처리_이력은_30일이_지나면_정리되고_설정은_남는다() {
    let f = F::mixed();
    let a = f.st["A"];
    let robot = f.dp["로봇"];
    let k = f.key();
    f.db.write(|c| identity::set_ref(c, &k, a, "I100000987")).unwrap();
    f.db.write(|c| course::set(c, robot, "20260818PGM000003317")).unwrap();
    f.db.write(|c| priority::save_dept_order(c, f.ws, &[robot])).unwrap();
    f.db.write(|c| priority::save_item_order(c, robot, &[수용비.into()])).unwrap();
    f.db.write(|c| setting::set_org_code(c, "2024000001")).unwrap();

    let fresh = f.db.read(|c| amount::fresh_settlement(c, f.ws)).unwrap();
    let ids: Vec<i64> = (0..4)
        .map(|_| f.db.write(|c| new_job(c, f.ws, Some(&fresh))).unwrap())
        .collect();
    for id in &ids {
        f.db.write(|c| {
            job::add_row(
                c,
                *id,
                &job::RowRecord {
                    source_row: 2,
                    student_id: Some(a),
                    department_id: Some(robot),
                    dream_item_code: "01".into(),
                    item_code: Some(강사료.into()),
                    amount_n: Some(35_000),
                    before: Some((0, 35_000, 0)),
                    match_status: "MATCHED".into(),
                    ..Default::default()
                },
            )?;
            job::add_event(c, *id, "IMPORT", "rows=4")
        })
        .unwrap();
    }
    // 0: 끝난 지 31일, 1: 끝난 지 29일, 2: 버려진 지 31일, 3: 오늘 시작
    f.db.write(|c| {
        job::set_status(c, ids[0], "EXPORTED", Some(&"cd".repeat(32)))?;
        job::set_status(c, ids[1], "FAILED", None)?;
        c.execute_batch(&format!(
            "UPDATE dream_job SET created_at = '2026-09-01 10:00:00', finished_at = '2026-09-27 09:00:00' WHERE id = {};
             UPDATE dream_job SET created_at = '2026-09-20 10:00:00', finished_at = '2026-09-29 11:00:00' WHERE id = {};
             UPDATE dream_job SET created_at = '2026-09-27 08:00:00' WHERE id = {};
             UPDATE dream_job SET created_at = '2026-10-28 08:00:00' WHERE id = {};",
            ids[0], ids[1], ids[2], ids[3]
        ))?;
        Ok(())
    })
    .unwrap();
    let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();

    let n = f.db.write(|c| job::cleanup_expired(c, "2026-10-28 12:00:00")).unwrap();
    assert_eq!(n, 2);
    let left: Vec<i64> = f
        .db
        .read(|c| {
            let v = c
                .prepare("SELECT id FROM dream_job ORDER BY id")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
            Ok(v)
        })
        .unwrap();
    assert_eq!(left, vec![ids[1], ids[3]]);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_job_row"), 2, "줄이 함께 지워지지 않았다");
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_job_event"), 2);
    // 설정은 그대로
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_student_identity"), 1);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_course_map"), 1);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_dept_priority"), 1);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_item_priority"), 1);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_key"), 1);
    assert_eq!(f.db.read(setting::org_code).unwrap().as_deref(), Some("2024000001"));
    assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);

    // 기록한 것: 기준 정산과 상태
    let (sid, state, status) = f
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT settlement_id, settle_state, status FROM dream_job WHERE id = ?1",
                [ids[1]],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
            )?)
        })
        .unwrap();
    assert_eq!((sid, state.as_str(), status.as_str()), (fresh.settlement_id, "FRESH", "FAILED"));
}

#[test]
fn 처리_이력에는_경로와_개인정보가_들어가지_않는다() {
    let f = F::mixed();
    for bad in ["C:\\Users\\x\\결제.xlsx", "a/b.xlsx", "C:결제.xlsx"] {
        let e = f
            .db
            .write(|c| {
                job::create(
                    c,
                    &job::NewJob {
                        workspace_id: f.ws,
                        source_name: bad,
                        source_sha256: &"ab".repeat(32),
                        source_size: 1,
                        settlement: None,
                        app_version: "0.1.10",
                    },
                )
            })
            .unwrap_err();
        assert_eq!(e.code, "INVALID", "{bad}");
    }
    assert!(f.db.write(|c| job::set_status(c, 999, "DONE", None)).is_err());
    // 이력 표에는 이름 · 개인참조번호 · 생년월일 칸이 없다
    let cols: Vec<String> = f
        .db
        .read(|c| {
            let v = c
                .prepare(
                    "SELECT name FROM pragma_table_info('dream_job_row')
                     UNION ALL SELECT name FROM pragma_table_info('dream_job')
                     UNION ALL SELECT name FROM pragma_table_info('dream_job_event')",
                )?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(v)
        })
        .unwrap();
    for c in &cols {
        for bad in ["name", "ref", "birth", "path"] {
            assert!(!(c.contains(bad) && c != "source_name"), "이력 칸 {c}");
        }
    }
}

// ─────────────────────────────────────────────── 기존 자료 불변 · 지우기

#[test]
fn 방과후드림_자료를_저장_수정_삭제_정리해도_기존_자료와_정산은_그대로다() {
    let f = F::mixed();
    let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
    let k = f.key();
    let (a, b) = (f.st["A"], f.st["B"]);
    let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
    f.db.write(|c| {
        identity::set_ref(c, &k, a, "I100000987")?;
        identity::set_birth(c, &k, a, "170806")?;
        identity::set_ref(c, &k, b, "I100000988")?;
        identity::set_ref(c, &k, a, "I100000989")?;
        identity::clear_birth(c, a)?;
        identity::delete(c, b)?;
        course::set(c, robot, "C1")?;
        course::set(c, art, "C2")?;
        course::remove(c, art)?;
        priority::save_dept_order(c, f.ws, &[art, robot])?;
        priority::save_item_order(c, robot, &[재료비.into()])?;
        priority::save_item_order(c, robot, &[])?;
        let fresh = amount::fresh_settlement(c, f.ws)?;
        let id = new_job(c, f.ws, Some(&fresh))?;
        job::add_event(c, id, "IMPORT", "rows=0")?;
        job::set_status(c, id, "CANCELLED", None)?;
        job::cleanup_expired(c, "2099-01-01 00:00:00")?;
        Ok(())
    })
    .unwrap();
    // 기준 금액 읽기도 아무것도 고치지 않는다
    f.db.read(|c| amount::workspace_amounts(c, f.ws)).unwrap();
    let after = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
    assert_eq!(after, before, "기존 표 · 자료판 · 정산 상태가 바뀌었다");
    assert_eq!(f.state(), "FRESH");
}

#[test]
fn 학생_부서_작업공간_학년도_지우기가_막히지_않는다() {
    let mut f = F::mixed();
    let k = f.key();
    // 수강 없는 학생 — 지울 수 있는 학생
    let lone = f.student("L", 2, "1", 9, "외톨이");
    let spare = f.dept("빈부서", "바둑", "A", vec![fee(강사료, 20_000)]);
    f.generate(); // 부서가 늘어 낡았다 — 다시 정산
    let fresh = f.db.read(|c| amount::fresh_settlement(c, f.ws)).unwrap();
    let jid = f
        .db
        .write(|c| {
            identity::set_ref(c, &k, lone, "I100000987")?;
            identity::set_birth(c, &k, f.st["A"], "170806")?;
            course::set(c, spare, "SPARE1")?;
            priority::save_dept_order(c, f.ws, &[spare])?;
            priority::save_item_order(c, spare, &[강사료.into()])?;
            let id = new_job(c, f.ws, Some(&fresh))?;
            job::add_row(
                c,
                id,
                &job::RowRecord {
                    source_row: 2,
                    student_id: Some(lone),
                    department_id: Some(spare),
                    match_status: "ERROR".into(),
                    reason_code: "NO_ENROLLMENT".into(),
                    ..Default::default()
                },
            )?;
            Ok(id)
        })
        .unwrap();

    // 학생 — 개인정보는 함께 사라지고, 이력 줄은 남되 연결만 끊긴다
    f.db.write(|c| repo::student::delete_many(c, &[lone])).unwrap();
    assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_student_identity WHERE student_id = {lone}")), 0);
    assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {jid} AND student_id IS NULL")), 1);
    assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);

    // 부서
    f.db.write(|c| repo::department::delete_many(c, &[spare])).unwrap();
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_course_map"), 0);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_dept_priority"), 0);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_item_priority"), 0);
    assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {jid} AND department_id IS NULL")), 1);
    assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);

    // 작업공간 — 이력도 함께
    let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
    let d2 = f.dept_in(ws2, "11월", "바둑", "A", vec![fee(강사료, 1)]);
    f.db.write(|c| {
        course::set(c, d2, "WS2")?;
        new_job(c, ws2, None)?;
        Ok(())
    })
    .unwrap();
    f.db.write(|c| repo::year::delete_workspace(c, ws2)).unwrap();
    assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job WHERE workspace_id = {ws2}")), 0);
    assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_course_map"), 0);
    assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);

    // 학년도 — 다른 학년도를 만들어 통째로 지운다
    let y2 = f.db.write(|c| repo::year::create_year(c, 2027, "2027학년도")).unwrap();
    f.db.write(|c| {
        let s = repo::student::create(
            c,
            y2,
            &StudentInput {
                grade: 1,
                class_no: "1".into(),
                student_no: 1,
                name: "새싹".into(),
                note: None,
            },
        )?;
        identity::set_ref(c, &k, s, "I200000001")
    })
    .unwrap();
    f.db.write(|c| repo::year::delete_year(c, y2)).unwrap();
    assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_student_identity WHERE year_id = {y2}")), 0);
    assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);
    let ok: String = f
        .db
        .read(|c| Ok(c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(ok, "ok");
}

fn cnt(db: &Db, sql: &str) -> i64 {
    db.read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
}
