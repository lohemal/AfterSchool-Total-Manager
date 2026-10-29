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

// ─────────────────────────────────────────────── P2 — 결제 기준월 · 파일 검사 · 키 상태 · 초기화

mod p2 {
    use super::*;
    use crate::domain::dream_file::code;
    use crate::excel::dream_fixture::{s, sample_rows, with, V};
    use crate::repo::dream::reset::{self, KeyStatus};
    use crate::repo::dream::{file_check, workspace_setting};

    fn ready(f: &F) {
        f.db.write(|c| setting::set_org_code(c, "I100000987")).unwrap();
        f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, "202609")).unwrap();
    }

    fn check(f: &F, bytes: &[u8]) -> file_check::FileCheck {
        f.db.read(|c| file_check::check(c, f.ws, "방과후드림 결제 파일.xlsx", bytes)).unwrap()
    }

    fn file_codes(x: &file_check::FileCheck) -> Vec<&'static str> {
        x.file_issues.iter().map(|i| i.code).collect()
    }

    #[test]
    fn 결제_기준월을_저장_수정해도_정산은_그대로다() {
        let f = F::mixed();
        let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
        assert_eq!(f.db.read(|c| workspace_setting::billing_month(c, f.ws)).unwrap(), None);
        assert_eq!(f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, " 202609 ")).unwrap(), "202609");
        f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, "202610")).unwrap();
        assert_eq!(f.db.read(|c| workspace_setting::billing_month(c, f.ws)).unwrap().as_deref(), Some("202610"));
        for bad in ["202613", "2026-10", "20261", "", "abcdef"] {
            assert!(f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, bad)).is_err(), "{bad}");
        }
        assert!(f.db.write(|c| workspace_setting::set_billing_month(c, 9999, "202609")).is_err());
        // DB 도 형식을 막는다
        let e = f.db.write(|c| {
            c.execute("UPDATE dream_workspace_setting SET billing_month = '202699'", [])?;
            Ok(())
        });
        assert!(e.is_err());
        assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before, "자료판 · 정산이 바뀌었다");
        assert_eq!(f.state(), "FRESH");
    }

    #[test]
    fn 파일_공통값이_작업공간과_맞으면_정상이다() {
        let f = F::mixed();
        ready(&f);
        let x = check(&f, &with(sample_rows()));
        assert_eq!(x.status, "OK", "{:?}", x.file_issues);
        assert_eq!((x.data_rows, x.row_error_count), (4, 0));
        assert_eq!(x.expected_year, 2026);
        assert_eq!(x.sha256.len(), 64);
        assert_eq!(x.rows[0].cells[2], "2******001", "화면으로는 가린 값만 간다");
        assert_eq!(x.rows[0].cells[7], "17****");
    }

    #[test]
    fn 회계년도는_작업공간의_학년도와_견준다() {
        let f = F::mixed();
        ready(&f);
        let mut rows = sample_rows();
        for r in rows.iter_mut() {
            r[0] = s("2027"); // 달력 연도로 짐작하지 않는다 — 작업공간은 2026학년도
        }
        let x = check(&f, &with(rows));
        assert_eq!(file_codes(&x), vec![code::ACCOUNTING_YEAR_MISMATCH]);
        assert_eq!(x.file_issues[0].expected.as_deref(), Some("2026"));
        assert_eq!(x.status, "ERROR");
    }

    #[test]
    fn 기관코드와_결제_기준월은_정해져_있고_같아야_한다() {
        let f = F::mixed();
        // 둘 다 미설정
        let x = check(&f, &with(sample_rows()));
        assert_eq!(file_codes(&x), vec![code::ORG_CODE_NOT_SET, code::BILLING_MONTH_NOT_SET]);
        // 설정했지만 다르다
        f.db.write(|c| setting::set_org_code(c, "0024000001")).unwrap();
        f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, "202610")).unwrap();
        let x = check(&f, &with(sample_rows()));
        assert_eq!(file_codes(&x), vec![code::ORG_CODE_MISMATCH, code::BILLING_MONTH_MISMATCH]);
        assert_eq!(x.file_issues[0].expected.as_deref(), Some("0024000001"), "앞자리 0");
        assert_eq!(x.file_issues[1].found.as_deref(), Some("202609"));
        // 맞추면 정상
        ready(&f);
        assert_eq!(check(&f, &with(sample_rows())).status, "OK");
    }

    #[test]
    fn 줄_오류와_참고는_상태에_다르게_쓰인다() {
        let f = F::mixed();
        ready(&f);
        let mut rows = sample_rows();
        rows[0][14] = V::N(1.0); // 기존 합 ≠ N — 참고만
        let x = check(&f, &with(rows));
        assert_eq!(x.status, "OK");
        assert_eq!((x.row_error_count, x.row_info_count), (0, 1));
        let mut rows = sample_rows();
        rows[1][12] = s("01"); // 수용비 / 01
        let x = check(&f, &with(rows));
        assert_eq!(x.status, "ERROR");
        assert_eq!(x.row_error_count, 1);
        assert_eq!(x.rows[1].status, "ERROR");
    }

    #[test]
    fn 파일을_읽기만_해도_db_는_바뀌지_않는다() {
        let f = F::mixed();
        ready(&f);
        let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
        let dream = |db: &Db| -> Vec<i64> {
            [
                "dream_job",
                "dream_job_row",
                "dream_job_event",
                "dream_key",
                "dream_student_identity",
                "dream_workspace_setting",
            ]
            .iter()
            .map(|t| cnt(db, &format!("SELECT COUNT(*) FROM {t}")))
            .collect()
        };
        let d0 = dream(&f.db);
        for _ in 0..3 {
            check(&f, &with(sample_rows()));
            check(&f, b"not a workbook");
        }
        assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
        assert_eq!(dream(&f.db), d0, "미리보기만으로 처리 이력 · 키가 생겼다");
        assert_eq!(f.state(), "FRESH");
    }

    #[test]
    fn 작업공간을_지우면_결제_기준월도_함께_사라진다() {
        let f = F::mixed();
        let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
        f.db.write(|c| workspace_setting::set_billing_month(c, ws2, "202611")).unwrap();
        f.db.write(|c| repo::year::delete_workspace(c, ws2)).unwrap();
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_workspace_setting"), 0);
        assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);
    }

    // ── 키 상태 · 초기화

    /// 다른 PC 로 옮긴 DB 흉내 — 감싼 키 사본을 이 PC 에서 풀리지 않는 바이트로 바꾼다.
    fn break_key(f: &F) {
        f.db.write(|c| {
            c.execute("UPDATE dream_key SET dpapi_blob = randomblob(178)", [])?;
            Ok(())
        })
        .unwrap();
    }

    /// 개인정보 · 설정을 채운 작업공간.
    fn with_identity() -> F {
        let f = F::mixed();
        let k = f.key();
        let (a, robot) = (f.st["A"], f.dp["로봇"]);
        f.db.write(|c| {
            identity::set_ref(c, &k, a, "I100000987")?;
            identity::set_birth(c, &k, f.st["B"], "170806")?;
            course::set(c, robot, "20260818PGM000003317")?;
            priority::save_dept_order(c, f.ws, &[robot])?;
            priority::save_item_order(c, robot, &[재료비.into()])?;
            setting::set_org_code(c, "2024000001")?;
            workspace_setting::set_billing_month(c, f.ws, "202609")?;
            Ok(())
        })
        .unwrap();
        f
    }

    #[test]
    fn 키_상태는_없음_준비_사용불가로_가른다() {
        let f = F::mixed();
        assert_eq!(f.db.read(reset::key_status).unwrap(), KeyStatus::NotInitialized);
        f.key();
        assert_eq!(f.db.read(reset::key_status).unwrap(), KeyStatus::Ready);
        break_key(&f);
        assert_eq!(f.db.read(reset::key_status).unwrap(), KeyStatus::KeyUnavailable);
        // 키를 쓸 수 없어도 다른 기능은 그대로다
        assert!(f.db.read(|c| amount::workspace_amounts(c, f.ws)).is_ok());
        f.db.write(|c| setting::set_org_code(c, "2024000009")).unwrap();
        assert_eq!(f.state(), "FRESH");
    }

    #[test]
    fn 쓸_수_있는_키는_초기화하지_않는다() {
        let f = with_identity();
        let e = f.db.write(|c| reset::reset_identity(c)).unwrap_err();
        assert_eq!(e.code, "INVALID");
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_student_identity"), 2);
        let f = F::mixed();
        assert!(f.db.write(|c| reset::reset_identity(c)).is_err(), "키가 없을 때도 하지 않는다");
    }

    #[test]
    fn 초기화하면_개인정보만_지우고_새_키로_다시_등록할_수_있다() {
        let f = with_identity();
        let key_of = |db: &Db| -> String {
            db.read(|c| Ok(c.query_row("SELECT key_id FROM dream_key", [], |r| r.get(0))?)).unwrap()
        };
        let old_key = key_of(&f.db);
        break_key(&f);
        let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
        let keep = |db: &Db| -> Vec<i64> {
            ["dream_course_map", "dream_dept_priority", "dream_item_priority", "dream_workspace_setting"]
                .iter()
                .map(|t| cnt(db, &format!("SELECT COUNT(*) FROM {t}")))
                .collect()
        };
        let kept = keep(&f.db);

        let r = f.db.write(|c| reset::reset_identity(c)).unwrap();
        assert_eq!((r.removed_identities, r.removed_keys), (2, 1));
        assert_eq!(f.db.read(reset::key_status).unwrap(), KeyStatus::Ready);
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_student_identity"), 0);
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_key"), 1);
        let new_key = key_of(&f.db);
        assert_ne!(new_key, old_key);
        assert_eq!(keep(&f.db), kept, "강좌 매핑 · 우선순위 · 결제 기준월이 지워졌다");
        assert_eq!(f.db.read(setting::org_code).unwrap().as_deref(), Some("2024000001"));
        assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before, "학생 · 정산 · 자료판이 바뀌었다");
        assert_eq!(f.state(), "FRESH");

        // 새 키로 다시 등록된다
        let k = f.key();
        assert_eq!(k.key_id, new_key);
        f.db.write(|c| identity::set_ref(c, &k, f.st["A"], "I100000987")).unwrap();
        assert_eq!(f.db.read(|c| identity::find_by_ref(c, &k, f.year, "I100000987")).unwrap(), Some(f.st["A"]));
    }

    #[test]
    fn 초기화_도중_실패하면_아무것도_지우지_않는다() {
        let f = with_identity();
        break_key(&f);
        let snapshot = |db: &Db| -> (i64, String) {
            (
                cnt(db, "SELECT COUNT(*) FROM dream_student_identity"),
                db.read(|c| Ok(c.query_row("SELECT key_id || hex(dpapi_blob) FROM dream_key", [], |r| r.get(0))?))
                    .unwrap(),
            )
        };
        let before = snapshot(&f.db);
        // 새 키를 넣는 순간 실패하게 만든다(디스크 · DB 오류 흉내)
        f.db.write(|c| {
            c.execute_batch(
                "CREATE TEMP TRIGGER zz_fail BEFORE INSERT ON dream_key BEGIN SELECT RAISE(ABORT, 'zz'); END;",
            )?;
            Ok(())
        })
        .unwrap();
        assert!(f.db.write(|c| reset::reset_identity(c)).is_err());
        assert_eq!(snapshot(&f.db), before, "반쪽만 지워진 상태가 남았다");
        assert_eq!(f.db.read(reset::key_status).unwrap(), KeyStatus::KeyUnavailable);
    }
}

// ─────────────────────────────────────────────── P3 — 학생 · 강좌 · 항목 · 금액 매칭

mod p3 {
    use super::*;
    use crate::excel::dream_fixture::{book, header, s as t, V};
    use crate::excel::dream_file::SHEET;
    use crate::repo::dream::matching::{code, Matching, State, StudentMatch};
    use crate::repo::dream::session::{self, Session};
    use crate::repo::dream::workspace_setting;

    /// 파일 학생 — (개인참조번호, 학년, 반, 번호, 이름, 생년월일). 모두 가짜.
    fn who(k: &str) -> (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str) {
        match k {
            "A" => ("2024000001", "3학년", "1반", "1", "가온", "170101"),
            "B" => ("2024000002", "3학년", "1반", "2", "나래", "170202"),
            "C" => ("2024000003", "3학년", "1반", "3", "다솜", "170303"),
            "D" => ("2024000004", "3학년", "1반", "4", "라온", "170404"),
            "I" => ("2024000005", "6학년", "가반", "1", "아라", "140505"),
            "J" => ("2024000006", "6학년", "가반", "2", "자람", "140606"),
            _ => unreachable!(),
        }
    }

    /// 부서 — (화면 이름, 강좌명코드, [강사료, 수용비, 교재비, 재료비])
    fn dept(k: &str) -> (&'static str, &'static str, [i64; 4]) {
        match k {
            "로봇" => ("로봇과학B", "PGM0001", [35_000, 1_500, 0, 0]),
            "만화" => ("만화애니메이션B", "PGM0002", [35_000, 1_500, 0, 22_000]),
            "미술" => ("창의미술B", "PGM0003", [28_000, 1_200, 0, 0]),
            "공예" => ("토탈공예미니어처B", "PGM0004", [35_000, 1_500, 0, 28_750]),
            _ => unreachable!(),
        }
    }

    const ITEMS4: [(&str, &str); 4] = [("강사료", "01"), ("수용비", "02"), ("교재비", "03"), ("재료비", "04")];

    /// 결제파일 한 줄
    #[allow(clippy::too_many_arguments)]
    fn line(st: (&str, &str, &str, &str, &str, &str), course: &str, code_: &str, item: (&str, &str), n: i64) -> Vec<V> {
        vec![
            t("2026"),
            t("I100000987"),
            t(st.0),
            t(st.1),
            t(st.2),
            t(st.3),
            t(st.4),
            t(st.5),
            V::N(202610.0),
            t(course),
            t(code_),
            t(item.0),
            t(item.1),
            V::N(n as f64),
            V::N(n as f64),
            V::N(0.0),
            V::N(0.0),
            V::None,
        ]
    }

    /// 학생 × 부서의 네 줄(0원 항목 포함 — 실제 파일처럼)
    fn four(k: &str, d: &str) -> Vec<Vec<V>> {
        let (label, c, fees) = dept(d);
        ITEMS4.iter().zip(fees).map(|(it, n)| line(who(k), label, c, *it, n)).collect()
    }

    fn xlsx(rows: Vec<Vec<V>>) -> Vec<u8> {
        let mut all = vec![header()];
        all.extend(rows);
        book(&[(SHEET, all)], Some(200))
    }

    /// F::mixed 의 모든 수강 그대로의 파일
    fn full_rows() -> Vec<Vec<V>> {
        let mut r = Vec::new();
        for (k, d) in [("A", "로봇"), ("B", "만화"), ("C", "미술"), ("D", "공예"), ("I", "로봇"), ("J", "로봇")] {
            r.extend(four(k, d));
        }
        r
    }

    /// 기관코드 · 결제 기준월 · 적용 학년(전 학년 — P6 이전과 같은 범위)
    fn setup(f: &F) {
        setup_grades(f, &[1, 2, 3, 4, 5, 6]);
    }

    fn setup_grades(f: &F, grades: &[i64]) {
        f.db.write(|c| setting::set_org_code(c, "I100000987")).unwrap();
        f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, "202610")).unwrap();
        f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, grades)).unwrap();
    }

    fn start(f: &F, bytes: &[u8]) -> Session {
        let sha = crypto::sha256_hex(bytes);
        f.db.write(|c| session::start(c, f.ws, "결제.xlsx", bytes, &sha, "0.1.10")).unwrap()
    }

    fn courses_all(f: &F, s: &Session, bytes: &[u8]) -> Session {
        let mut last = None;
        for d in ["로봇", "만화", "미술", "공예"] {
            let code_ = dept(d).1;
            let Some(&id) = f.dp.get(d) else { continue };
            if !s.matching.courses.iter().any(|c| c.course_code == code_) {
                continue;
            }
            last = Some(
                f.db.write(|c| {
                    session::confirm_course(c, s.job_id, bytes, code_, id)?;
                    session::revalidate(c, s.job_id, bytes)
                })
                .unwrap(),
            );
        }
        last.expect("강좌가 하나는 있다")
    }

    fn identities_all(f: &F, s: &Session, bytes: &[u8]) -> Session {
        let picks: Vec<(u32, i64)> = s
            .matching
            .students
            .iter()
            .filter(|x| x.ref_state == "NOT_REGISTERED" && x.candidates.len() == 1)
            .map(|x| (x.key, x.candidates[0].student_id))
            .collect();
        f.db.write(|c| {
            session::confirm_identities(c, s.job_id, bytes, &picks)?;
            session::revalidate(c, s.job_id, bytes)
        })
        .unwrap()
    }

    /// 강좌 연결 · 개인참조번호를 모두 확정한 세션
    fn confirmed(f: &F, bytes: &[u8]) -> Session {
        let s = start(f, bytes);
        let s = courses_all(f, &s, bytes);
        identities_all(f, &s, bytes)
    }

    fn student<'a>(m: &'a Matching, name: &str) -> &'a StudentMatch {
        m.students.iter().find(|s| s.file_name == name).expect("파일 학생")
    }

    fn codes(s: &StudentMatch) -> Vec<&'static str> {
        let mut v: Vec<&str> = s.reasons.iter().map(|r| r.code).collect();
        v.extend(s.lines.iter().flat_map(|l| l.reasons.iter().map(|r| r.code)));
        v.sort_unstable();
        v.dedup();
        v
    }

    // ── 흐름 · 처리 이력

    #[test]
    fn 시작하면_처리이력이_생기고_기준_정산을_적는다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(full_rows());
        let s = start(&f, &bytes);
        let head = f.db.read(|c| job::get(c, s.job_id)).unwrap();
        let fresh = f.db.read(|c| amount::fresh_settlement(c, f.ws)).unwrap();
        assert_eq!(head.settlement_id, Some(fresh.settlement_id));
        assert_eq!(head.source_sha256, crypto::sha256_hex(&bytes));
        assert_eq!(s.job_status, "OPEN", "확인할 것이 있다");
        assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {}", s.job_id)), 24);
        assert!(!s.matching.p4_ready);
        // 지문이 다르면 시작하지 않는다(미리보기 뒤에 파일이 바뀜)
        let e = f.db.write(|c| session::start(c, f.ws, "결제.xlsx", &bytes, &"0".repeat(64), "0.1.10")).unwrap_err();
        assert_eq!(e.code, "FILE_CHANGED");
    }

    #[test]
    fn 정산이_최신이_아니면_시작하지_않는다() {
        let mut f = F::mixed();
        setup(&f);
        f.dept("바둑", "바둑", "A", vec![fee(강사료, 1)]); // 부서가 늘어 STALE_DATA
        let bytes = xlsx(full_rows());
        let sha = crypto::sha256_hex(&bytes);
        assert!(f.db.write(|c| session::start(c, f.ws, "결제.xlsx", &bytes, &sha, "0.1.10")).is_err());
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_job"), 0);
    }

    #[test]
    fn 모두_확정하면_준비_완료이고_기존_자료는_그대로다() {
        let f = F::mixed();
        setup(&f);
        let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
        let bytes = xlsx(full_rows());
        let s0 = start(&f, &bytes);
        assert!(s0.matching.courses.iter().all(|c| c.state == State::NeedsConfirmation));
        // 강좌명이 같은 부서를 제안한다
        let robot = s0.matching.courses.iter().find(|c| c.course_code == "PGM0001").unwrap();
        assert_eq!(robot.suggestions[0].department_id, f.dp["로봇"]);
        assert!(robot.suggestions[0].why.contains(&"SAME_NAME"));
        assert!(robot.suggestions[0].why.contains(&"STUDENT_ENROLLED"));
        let s = confirmed(&f, &bytes);
        let m = &s.matching;
        assert!(m.students.iter().all(|x| x.state == State::Ready), "{:?}", m.students.iter().map(codes).collect::<Vec<_>>());
        assert!(m.p4_ready);
        assert_eq!(s.job_status, "READY");
        // 기준액 — 학생 C: T 29,200 = V 3,400 + 개인부담 25,800
        let c = student(m, "다솜");
        let a = c.amount.as_ref().unwrap();
        assert_eq!((c.sum_n, a.total, a.voucher, a.personal), (29_200, 29_200, 3_400, 25_800));
        // 0원 항목(교재비)도 정상
        assert!(c.lines.iter().any(|l| l.item_code == Some(교재비) && l.charge == Some(0) && l.state == State::Ready));
        // 정산 원본은 그대로
        assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
        assert_eq!(f.state(), "FRESH");
        // 줄 기록에는 내부 id · 금액 · 상태만
        let matched = cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {} AND match_status = 'MATCHED' AND student_id IS NOT NULL AND after_o IS NULL", s.job_id));
        assert_eq!(matched, 24);
    }

    #[test]
    fn 우선순위는_매칭_결과를_바꾸지_않는다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(full_rows());
        let s1 = confirmed(&f, &bytes);
        let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
        f.db.write(|c| {
            priority::save_dept_order(c, f.ws, &[art, robot])?;
            priority::save_item_order(c, robot, &[재료비.into(), 강사료.into()])
        })
        .unwrap();
        let s2 = f.db.write(|c| session::revalidate(c, s1.job_id, &bytes)).unwrap();
        let key = |m: &Matching| serde_json::to_string(&m.students).unwrap();
        assert_eq!(key(&s1.matching), key(&s2.matching));
    }

    // ── 학생

    #[test]
    fn 미등록_학생은_정확한_후보가_하나면_확인_필요다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let a = student(&s.matching, "가온");
        assert_eq!(a.state, State::NeedsConfirmation);
        assert_eq!(a.ref_state, "NOT_REGISTERED");
        assert_eq!(a.candidates.len(), 1);
        assert_eq!(a.candidates[0].student_id, f.st["A"]);
        assert!(codes(a).contains(&code::STUDENT_REF_NOT_REGISTERED));
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_student_identity"), 0, "확인 전에 저장하지 않는다");
    }

    #[test]
    fn 개인참조번호가_맞아도_다른_학생_정보가_다르면_오류다() {
        // (바꿀 칸, 기대 코드)
        let cases: [(usize, &str, &str); 5] = [
            (6, "가온이", code::STUDENT_NAME_MISMATCH),
            (3, "4학년", code::STUDENT_GRADE_MISMATCH),
            (4, "2반", code::STUDENT_CLASS_MISMATCH),
            (5, "9", code::STUDENT_NO_MISMATCH),
            (7, "180101", code::STUDENT_BIRTH_MISMATCH),
        ];
        for (col, v, want) in cases {
            let f = F::mixed();
            setup(&f);
            let good = xlsx(four("A", "로봇"));
            confirmed(&f, &good); // 개인참조번호 · 생년월일 등록
            let mut rows = four("A", "로봇");
            for r in rows.iter_mut() {
                r[col] = t(v);
            }
            let s = start(&f, &xlsx(rows));
            let a = &s.matching.students[0];
            assert_eq!(a.ref_state, "MATCHED");
            assert_eq!(a.state, State::Error, "{want}");
            assert!(codes(a).contains(&want), "{want}: {:?}", codes(a));
            assert!(!s.matching.p4_ready);
        }
    }

    #[test]
    fn 후보가_없으면_오류_여럿이면_확인_필요다() {
        let mut f = F::mixed();
        setup(&f);
        let mut rows = four("A", "로봇");
        for r in rows.iter_mut() {
            r[6] = t("가 온"); // 띄어쓰기만 달라도 같은 학생이 아니다
        }
        let s = start(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::STUDENT_NOT_FOUND));
        assert_eq!(s.matching.students[0].state, State::Error);

        // 반 이름이 '1반' 인 학생을 하나 더 — 원래 값 '1반' 과 후보 '1' 이 서로 다른 학생을 가리킨다
        let twin = f.student("T", 3, "1반", 1, "가온");
        f.enroll("T", "로봇");
        f.generate();
        let s = start(&f, &xlsx(four("A", "로봇")));
        let a = &s.matching.students[0];
        assert_eq!(a.state, State::NeedsConfirmation);
        assert!(codes(a).contains(&code::STUDENT_AMBIGUOUS));
        let mut ids: Vec<i64> = a.candidates.iter().map(|c| c.student_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![f.st["A"], twin]);
        // 사람이 고르면 그 학생으로 등록된다
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let s = courses_all(&f, &s, &bytes);
        let s = f.db.write(|c| {
            session::confirm_identities(c, s.job_id, &bytes, &[(2, twin)])?;
            session::revalidate(c, s.job_id, &bytes)
        })
        .unwrap();
        assert_eq!(s.matching.students[0].student_id, Some(twin));
    }

    #[test]
    fn 반은_원래_값_또는_후보와_정확히_같아야_한다() {
        let f = F::mixed();
        setup(&f);
        for (raw, ok) in [("1", true), ("1반", true), ("01반", false), ("1 반", false), ("일반", false)] {
            let mut rows = four("A", "로봇");
            for r in rows.iter_mut() {
                r[4] = t(raw);
            }
            let s = start(&f, &xlsx(rows));
            let a = &s.matching.students[0];
            assert_eq!(a.candidates.len() == 1, ok, "{raw}");
        }
    }

    #[test]
    fn 다른_학년도_학생은_후보가_아니다() {
        let f = F::mixed();
        setup(&f);
        let y2 = f.db.write(|c| repo::year::create_year(c, 2027, "2027학년도")).unwrap();
        f.db.write(|c| {
            repo::student::create(
                c,
                y2,
                &StudentInput { grade: 3, class_no: "1".into(), student_no: 9, name: "외부".into(), note: None },
            )
        })
        .unwrap();
        let mut rows = four("A", "로봇");
        for r in rows.iter_mut() {
            r[5] = t("9");
            r[6] = t("외부");
        }
        let s = start(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::STUDENT_NOT_FOUND));
    }

    #[test]
    fn 다른_번호가_이미_있는_학생과_남의_번호는_등록하지_않는다() {
        let f = F::mixed();
        setup(&f);
        let k = f.key();
        // A 에게 다른 번호가 이미 있다 → 후보여도 충돌
        f.db.write(|c| identity::set_ref(c, &k, f.st["A"], "9999999999")).unwrap();
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let a = &s.matching.students[0];
        assert_eq!((a.ref_state, a.state), ("CONFLICT", State::Error));
        assert!(f.db.write(|c| session::confirm_identities(c, s.job_id, &bytes, &[(2, f.st["A"])])).is_err());
        // 파일의 번호(B 의 것)를 이미 B 가 쓰고 있으면 A 로 등록할 수 없다
        let f = F::mixed();
        setup(&f);
        let k = f.key();
        f.db.write(|c| identity::set_ref(c, &k, f.st["B"], "2024000001")).unwrap();
        let s = start(&f, &bytes);
        // 번호로 B 를 찾았지만 B 의 정보와 파일(A)이 다르다 → 오류
        assert_eq!(s.matching.students[0].ref_state, "MATCHED");
        assert_eq!(s.matching.students[0].state, State::Error);
        // 후보가 아닌 학생으로는 등록할 수 없다
        let f = F::mixed();
        setup(&f);
        let s = start(&f, &bytes);
        let e = f.db.write(|c| session::confirm_identities(c, s.job_id, &bytes, &[(2, f.st["C"])])).unwrap_err();
        assert_eq!(e.code, "INVALID");
        assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_student_identity"), 0);
    }

    #[test]
    fn 등록하면_학생_전체를_다시_검증해_준비_완료가_된다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let s = courses_all(&f, &s, &bytes);
        assert_eq!(s.matching.students[0].state, State::NeedsConfirmation);
        let s = identities_all(&f, &s, &bytes);
        let a = &s.matching.students[0];
        assert_eq!((a.ref_state, a.birth_state, a.state), ("MATCHED", "MATCHED", State::Ready));
        // 저장은 암호문으로만
        let k = f.key();
        assert_eq!(f.db.read(|c| identity::reveal_birth(c, &k, f.st["A"])).unwrap().unwrap().as_str(), "170101");
    }

    #[test]
    fn 생년월일이_없으면_확인_필요고_등록하면_준비_완료다() {
        let f = F::mixed();
        setup(&f);
        let k = f.key();
        f.db.write(|c| identity::set_ref(c, &k, f.st["A"], "2024000001")).unwrap(); // 생년월일 없이
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let s = courses_all(&f, &s, &bytes);
        let a = &s.matching.students[0];
        assert_eq!((a.birth_state, a.state), ("NOT_REGISTERED", State::NeedsConfirmation));
        assert!(codes(a).contains(&code::STUDENT_BIRTH_NOT_REGISTERED));
        let s = f.db.write(|c| {
            session::confirm_births(c, s.job_id, &bytes, &[2])?;
            session::revalidate(c, s.job_id, &bytes)
        })
        .unwrap();
        assert_eq!(s.matching.students[0].state, State::Ready);
    }

    #[test]
    fn 키를_쓸_수_없으면_시작하지_않는다() {
        let f = F::mixed();
        setup(&f);
        f.key();
        f.db.write(|c| {
            c.execute("UPDATE dream_key SET dpapi_blob = randomblob(178)", [])?;
            Ok(())
        })
        .unwrap();
        let bytes = xlsx(four("A", "로봇"));
        let sha = crypto::sha256_hex(&bytes);
        let e = f.db.write(|c| session::start(c, f.ws, "결제.xlsx", &bytes, &sha, "0.1.10")).unwrap_err();
        assert_eq!(e.code, "KEY_UNAVAILABLE");
    }

    // ── 강좌

    #[test]
    fn 확정된_강좌의_이름이_부서와_달라도_참고일_뿐이다() {
        // P4 정책: 강좌코드 → 부서 연결이 기준이고 강좌명(J)은 열쇠가 아니다
        let f = F::mixed();
        setup(&f);
        f.db.write(|c| course::set(c, f.dp["로봇"], "PGM0001")).unwrap();
        let mut rows = four("A", "로봇");
        for r in rows.iter_mut() {
            r[9] = t("로봇과학 B");
        }
        let bytes = xlsx(rows);
        let s = start(&f, &bytes);
        let c = &s.matching.courses[0];
        assert_eq!(c.state, State::Ready);
        assert_eq!(c.reasons[0].code, code::COURSE_NAME_DIFFERS);
        assert_eq!(c.reasons[0].state, State::Ready, "참고");
        let s = identities_all(&f, &s, &bytes);
        assert_eq!(s.matching.students[0].state, State::Ready, "{:?}", codes(&s.matching.students[0]));
        // 강좌명을 짐작해 맞추지는 않는다 — 연결이 없으면 '이름 같음' 추천이 나오지 않는다
        f.db.write(|c| course::remove(c, f.dp["로봇"])).unwrap();
        let s2 = f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
        let c2 = &s2.matching.courses[0];
        assert_eq!(c2.department_id, None);
        assert_eq!(c2.state, State::NeedsConfirmation);
        assert!(!c2.suggestions.iter().any(|x| x.why.contains(&"SAME_NAME")));
    }

    #[test]
    fn 강좌코드가_맞아도_학생이_수강하지_않으면_오류다() {
        let f = F::mixed();
        setup(&f);
        f.db.write(|c| course::set(c, f.dp["미술"], "PGM0003")).unwrap();
        // A 는 미술을 수강하지 않는다
        let (label, c_, fees) = dept("미술");
        let rows: Vec<Vec<V>> = ITEMS4.iter().zip(fees).map(|(it, n)| line(who("A"), label, c_, *it, n)).collect();
        let s = start(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::ENROLLMENT_NOT_FOUND));
    }

    #[test]
    fn 직전_작업공간의_같은_코드_연결을_제안하고_확정은_사람이_한다() {
        let mut f = F::mixed();
        setup(&f);
        let prev = f.workspace("9월", "2026-09-01", "2026-09-30");
        let pd = f.dept_in(prev, "9월로봇", "로봇과학", "B", vec![fee(강사료, 1)]);
        f.db.write(|c| course::set(c, pd, "PGM0001")).unwrap();
        f.generate(); // 앞 작업공간이 생겨 다시 정산
        let s = start(&f, &xlsx(four("A", "로봇")));
        let c = &s.matching.courses[0];
        assert_eq!(c.state, State::NeedsConfirmation, "제안 ≠ 확정");
        assert_eq!(c.previous.as_ref().map(|p| p.1.as_str()), Some("로봇과학B"));
        let sg = c.suggestions.iter().find(|x| x.department_id == f.dp["로봇"]).unwrap();
        assert!(sg.why.contains(&"PREVIOUS_WORKSPACE"));
        assert_eq!(f.db.read(|c| course::find(c, f.ws, "PGM0001")).unwrap(), None, "자동 저장하지 않는다");
    }

    #[test]
    fn 다른_코드에_이어진_부서와_다른_작업공간_부서는_고를_수_없다() {
        let mut f = F::mixed();
        setup(&f);
        f.db.write(|c| course::set(c, f.dp["로봇"], "PGMXXXX")).unwrap();
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let sg = s.matching.courses[0].suggestions.iter().find(|x| x.department_id == f.dp["로봇"]).unwrap();
        assert!(sg.taken);
        assert!(f.db.write(|c| session::confirm_course(c, s.job_id, &bytes, "PGM0001", f.dp["로봇"])).is_err());
        let ws2 = f.workspace("11월", "2026-11-01", "2026-11-30");
        let other = f.dept_in(ws2, "11월", "로봇과학", "B", vec![fee(강사료, 1)]);
        let e = f.db.write(|c| session::confirm_course(c, s.job_id, &bytes, "PGM0001", other)).unwrap_err();
        assert_eq!(e.code, "INVALID");
        let e = f.db.write(|c| session::confirm_course(c, s.job_id, &bytes, "NOPE01", f.dp["미술"])).unwrap_err();
        assert_eq!(e.code, "INVALID", "파일에 없는 코드");
    }

    #[test]
    fn 강좌_연결을_확정하면_그_강좌의_학생_모두를_다시_본다() {
        let f = F::mixed();
        setup(&f);
        let mut rows = four("A", "로봇");
        rows.extend(four("I", "로봇"));
        rows.extend(four("J", "로봇"));
        let bytes = xlsx(rows);
        let s = start(&f, &bytes);
        let s = identities_all(&f, &s, &bytes);
        assert!(s.matching.students.iter().all(|x| x.state == State::NeedsConfirmation));
        let s = f.db.write(|c| {
            session::confirm_course(c, s.job_id, &bytes, "PGM0001", f.dp["로봇"])?;
            session::revalidate(c, s.job_id, &bytes)
        })
        .unwrap();
        assert!(s.matching.students.iter().all(|x| x.state == State::Ready));
        // 학생은 모두 준비 완료지만 정산에 금액이 남은 B · C · D 가 파일에 없다 — 파일 전체 오류
        let r = s.matching.file_reasons.iter().find(|r| r.code == code::SETTLEMENT_STUDENT_NOT_IN_FILE).unwrap();
        assert_eq!(r.state, State::Error);
        assert_eq!(s.matching.absent.len(), 3);
        assert!(!s.matching.p4_ready);
        assert_eq!(s.job_status, "BLOCKED");
    }

    #[test]
    fn 부서를_지우면_강좌_연결도_사라지고_다시_확인해야_한다() {
        let mut f = F::mixed();
        setup(&f);
        let spare = f.dept("빈", "바둑", "A", vec![fee(강사료, 1)]);
        f.db.write(|c| course::set(c, spare, "PGM9999")).unwrap();
        f.db.write(|c| repo::department::delete_many(c, &[spare])).unwrap();
        assert_eq!(f.db.read(|c| course::find(c, f.ws, "PGM9999")).unwrap(), None);
        assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);
    }

    // ── 금액

    #[test]
    fn 정산금액이_charge_와_다르면_오류고_고쳐_맞추지_않는다() {
        let f = F::mixed();
        setup(&f);
        let mut rows = four("A", "로봇");
        rows[0][13] = V::N(30_000.0); // 강사료 35,000 → 30,000
        let bytes = xlsx(rows);
        let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
        let s = confirmed(&f, &bytes);
        let a = &s.matching.students[0];
        let c = codes(a);
        assert!(c.contains(&code::CHARGE_AMOUNT_MISMATCH) && c.contains(&code::STUDENT_TOTAL_MISMATCH), "{c:?}");
        assert_eq!(a.lines[0].charge, Some(35_000));
        assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before, "Charge 를 고쳤다");
    }

    #[test]
    fn 영원_줄은_charge_0_이나_charge_없음과_맞고_양수와는_다르다() {
        let mut f = F::mixed();
        setup(&f);
        // 강사료만 넣어 수강을 만든다 — 나머지 항목은 Charge 행이 없다
        let x = f.student("X", 2, "1", 5, "하늘");
        let ws = f.ws;
        let items = f.items.clone();
        let robot = f.dp["로봇"];
        f.db.write(|c| {
            repo::enrollment::create(
                c,
                ws,
                &EnrollmentInput { student_id: x, department_id: robot, fees: vec![fee(강사료, 35_000)], reason: None },
                &items,
            )
        })
        .unwrap();
        assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM charge c JOIN enrollment e ON e.id = c.enrollment_id WHERE e.student_id = {x}")), 1);
        f.generate();
        let st = ("2024000099", "2학년", "1반", "5", "하늘", "180505");
        let mk = |n: [i64; 4]| -> Vec<Vec<V>> { ITEMS4.iter().zip(n).map(|(it, v)| line(st, "로봇과학B", "PGM0001", *it, v)).collect() };

        // N=0 + Charge 없음 → 정상
        let bytes = xlsx(mk([35_000, 0, 0, 0]));
        let s = confirmed(&f, &bytes);
        let h = &s.matching.students[0];
        assert_eq!(h.state, State::Ready, "{:?}", codes(h));
        assert_eq!(h.lines[1].charge, Some(0));

        // N>0 + Charge 없음 → 오류
        let s = start(&f, &xlsx(mk([35_000, 1_500, 0, 0])));
        assert!(codes(&s.matching.students[0]).contains(&code::CHARGE_MISSING));
        // N>0 + Charge 0 (A 의 교재비 0원) → 오류
        let mut rows = four("A", "로봇");
        rows[2][13] = V::N(500.0);
        let s = start(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::CHARGE_MISSING));
        // N=0 + Charge > 0 → 오류
        let mut rows = four("A", "로봇");
        rows[1][13] = V::N(0.0);
        rows[1][14] = V::N(0.0);
        let s = start(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::CHARGE_AMOUNT_MISMATCH));
    }

    #[test]
    fn 정산에_있는_양수_charge_가_파일에_없으면_오류다() {
        let f = F::mixed();
        setup(&f);
        let mut rows = four("A", "로봇");
        rows.remove(1); // 수용비 1,500 줄이 빠졌다
        let s = confirmed(&f, &xlsx(rows));
        let a = &s.matching.students[0];
        assert!(codes(a).contains(&code::DB_CHARGE_NOT_IN_FILE), "{:?}", codes(a));
        assert_eq!(a.missing.len(), 1);
        assert_eq!((a.missing[0].item_name.as_str(), a.missing[0].amount), ("수용비", 1_500));
        assert_eq!(a.state, State::Error);
    }

    #[test]
    fn 같은_학생_부서_항목이_두_줄이면_더하지_않고_오류다() {
        let f = F::mixed();
        setup(&f);
        let mut rows = four("A", "로봇");
        let dup = rows[0].clone();
        rows.push(dup);
        let s = confirmed(&f, &xlsx(rows));
        assert!(codes(&s.matching.students[0]).contains(&code::DUPLICATE_PAYMENT_ROW));
        assert_eq!(s.matching.students[0].state, State::Error);
    }

    #[test]
    fn 취소했어도_정산에_들어간_charge_는_맞대어_본다() {
        let f = F::mixed();
        setup(&f);
        let eid: i64 = cnt(&f.db, &format!("SELECT id FROM enrollment WHERE student_id = {}", f.st["A"]));
        let items = f.items.clone();
        // 강사료 20,000 만 남기고 취소
        f.db.write(|c| {
            repo::enrollment::cancel(c, eid, Some(&[fee(강사료, 20_000), fee(수용비, 0)]), "시험", &items)
        })
        .unwrap();
        f.generate();
        let mut rows = four("A", "로봇");
        rows[0][13] = V::N(20_000.0);
        rows[0][14] = V::N(20_000.0);
        rows[1][13] = V::N(0.0);
        rows[1][14] = V::N(0.0);
        let s = confirmed(&f, &xlsx(rows));
        let a = &s.matching.students[0];
        assert_eq!(a.state, State::Ready, "{:?}", codes(a));
        assert_eq!(a.amount.as_ref().unwrap().total, 20_000);
    }

    // ── 학생 단위 · 파일 전체

    #[test]
    fn 한_줄만_틀려도_그_학생_전체가_준비_완료가_아니다() {
        let f = F::mixed();
        setup(&f);
        // A 가 로봇 · 미술 · 공예를 듣는다
        f.enroll("A", "미술");
        f.enroll("A", "공예");
        f.generate();
        let mut rows = four("A", "로봇");
        let mut art = Vec::new();
        let (label, c_, fees) = dept("미술");
        for (it, n) in ITEMS4.iter().zip(fees) {
            art.push(line(who("A"), label, c_, *it, n));
        }
        art[0][13] = V::N(1.0); // 미술 강사료만 틀렸다
        rows.extend(art);
        let (label, c_, fees) = dept("공예");
        for (it, n) in ITEMS4.iter().zip(fees) {
            rows.push(line(who("A"), label, c_, *it, n));
        }
        let s = confirmed(&f, &xlsx(rows));
        let a = &s.matching.students[0];
        assert_eq!(a.state, State::Error);
        // 로봇 · 공예 줄은 그 자체로 맞지만, 기록에는 학생 상태(오류)로 남는다
        assert!(a.lines.iter().filter(|l| l.state == State::Ready).count() >= 8);
        let rows_ok = cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {} AND match_status = 'MATCHED'", s.job_id));
        assert_eq!(rows_ok, 0, "오류 학생의 줄이 준비 완료로 남았다");
        assert!(!s.matching.p4_ready);
        assert_eq!(s.job_status, "BLOCKED");
    }

    #[test]
    fn 파일_전체는_모든_학생이_준비_완료일_때만_다음_단계로_간다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(full_rows());
        let s = start(&f, &bytes);
        let s = courses_all(&f, &s, &bytes);
        // 다솜만 빼고 확인
        let picks: Vec<(u32, i64)> = s
            .matching
            .students
            .iter()
            .filter(|x| x.file_name != "다솜")
            .map(|x| (x.key, x.candidates[0].student_id))
            .collect();
        let s = f.db.write(|c| {
            session::confirm_identities(c, s.job_id, &bytes, &picks)?;
            session::revalidate(c, s.job_id, &bytes)
        })
        .unwrap();
        assert_eq!((s.matching.ready, s.matching.needs_confirmation), (5, 1));
        assert!(!s.matching.p4_ready);
        assert_eq!(s.job_status, "OPEN");
        let s = identities_all(&f, &s, &bytes);
        assert!(s.matching.p4_ready);
        assert_eq!(s.job_status, "READY");
    }

    #[test]
    fn 파일이나_정산이_바뀌면_작업을_계속하지_않는다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(four("A", "로봇"));
        let s = start(&f, &bytes);
        let other = xlsx(four("B", "만화"));
        assert_eq!(f.db.write(|c| session::revalidate(c, s.job_id, &other)).unwrap_err().code, "FILE_CHANGED");
        f.generate(); // 다시 정산 — 정산 id 가 바뀐다
        assert_eq!(f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap_err().code, "SETTLEMENT_CHANGED");
        f.db.write(|c| session::fail(c, s.job_id, "SETTLEMENT_CHANGED")).unwrap();
        assert_eq!(f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap_err().code, "INVALID", "끝난 작업");
    }

    #[test]
    fn 처리_이력과_화면_결과에_개인정보_원문이_없다() {
        let f = F::mixed();
        setup(&f);
        let bytes = xlsx(full_rows());
        let s = confirmed(&f, &bytes);
        let json = serde_json::to_string(&s.matching).unwrap();
        for v in ["2024000001", "170101", "2024000006", "140606"] {
            assert!(!json.contains(v), "화면 결과에 {v}");
        }
        let text: String = f
            .db
            .read(|c| {
                let v: Vec<String> = c
                    .prepare(
                        "SELECT group_concat(quote(r.student_id)||quote(r.reason_code)||quote(r.dream_item_code), '|') FROM dream_job_row r
                         UNION ALL SELECT group_concat(kind||detail, '|') FROM dream_job_event
                         UNION ALL SELECT group_concat(source_name, '|') FROM dream_job",
                    )?
                    .query_map([], |r| r.get::<_, Option<String>>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect();
                Ok(v.join("|"))
            })
            .unwrap();
        for v in ["2024000001", "170101", "가온", "나래"] {
            assert!(!text.contains(v), "처리 이력에 {v}");
        }
    }

    // ─────────────────────────────────────────────── P4 — 우선순위 · 재배분

    mod p4 {
        use super::*;
        use crate::repo::dream::allocation::{self, Allocation, AllocStudent};

        fn allocate(f: &F, job: i64, bytes: &[u8]) -> Allocation {
            f.db.write(|c| allocation::allocate(c, job, bytes)).unwrap()
        }

        fn verify(f: &F, job: i64, bytes: &[u8]) -> Allocation {
            f.db.read(|c| allocation::verify(c, job, bytes)).unwrap()
        }

        fn by_label<'a>(a: &'a Allocation, label_part: &str) -> &'a AllocStudent {
            a.students.iter().find(|s| s.student_label.contains(label_part)).expect("학생")
        }

        /// (부서 화면 이름, 항목 코드) → (O, P, Q)
        fn opq(s: &AllocStudent) -> Vec<(String, String, (i64, i64, i64))> {
            s.lines.iter().map(|l| (l.dept_label.clone(), l.item_code.clone(), l.after)).collect()
        }

        fn after_rows(f: &F, job: i64) -> i64 {
            cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_row WHERE job_id = {job} AND after_o IS NOT NULL"))
        }

        fn versions(f: &F) -> (i64, i64) {
            f.db.read(|c| {
                let ws = repo::year::get_workspace(c, f.ws)?;
                Ok((ws.data_version, repo::year::get_year(c, ws.year_id)?.data_version))
            })
            .unwrap()
        }


        // ── 전체 흐름

        #[test]
        fn 준비_완료_파일을_배분하면_학생과_파일_전체가_정산과_같다() {
            let f = F::mixed();
            setup(&f);
            let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
            let dv = versions(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            assert!(s.matching.p4_ready);
            let a = allocate(&f, s.job_id, &bytes);
            assert_eq!(a.status, "DONE", "{:?}", a.issues);
            assert_eq!(a.job_status, "READY");
            assert_eq!(a.students.len(), 6);
            // 학생마다: ΣN = T · ΣP = V · ΣQ = F · ΣO = 개인부담, 줄마다 N = O + P + Q
            for st in &a.students {
                let x = &st.amount;
                assert_eq!((st.after.n, st.after.p, st.after.q, st.after.o), (x.total, x.voucher, x.free_voucher, x.personal));
                assert!(st.lines.iter().all(|l| l.after.0 + l.after.1 + l.after.2 == l.amount_n));
                assert!(st.lines.iter().all(|l| l.after.0 >= 0 && l.after.1 >= 0 && l.after.2 >= 0));
            }
            // 파일 전체 = 정산 전체
            assert_eq!(a.after, a.settlement);
            assert_eq!(a.after.n, a.after.o + a.after.p + a.after.q);
            assert_eq!(a.before.n, a.after.n, "N 은 바뀌지 않는다");
            // 일반 수익자 · 이용권 전액(D: V + F = T)
            let ga = by_label(&a, "가온");
            assert!(ga.lines.iter().all(|l| l.after == (l.amount_n, 0, 0)));
            let ra = by_label(&a, "라온");
            assert_eq!(ra.after.o, 0);
            // 0원 줄은 0 · 0 · 0
            assert!(a.students.iter().flat_map(|s| &s.lines).filter(|l| l.amount_n == 0).all(|l| l.after == (0, 0, 0)));
            // 저장: 모든 줄에 after, 기록에 지문
            assert_eq!(after_rows(&f, s.job_id), 24);
            let ev = f.db.read(|c| job::last_event(c, s.job_id, "ALLOCATE")).unwrap().unwrap();
            assert!(ev.contains(&format!("fp={}", a.fingerprint.clone().unwrap())));
            // 기존 자료 · 자료판 · 정산은 그대로
            assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
            assert_eq!(versions(&f), dv);
            assert_eq!(f.state(), "FRESH");
            assert_eq!(f.db.read(|c| Ok(fk_violations(c))).unwrap(), 0);
            // 다시 확인하면 그대로 유효
            let v = verify(&f, s.job_id, &bytes);
            assert_eq!(v.status, "DONE");
            assert_eq!(v.fingerprint, a.fingerprint);
        }

        #[test]
        fn 이용권_초과금은_p_에_들어가지_않는다() {
            // C: 이용권 3,400 + 초과(개인부담), J: 이용권 1,000 + 자유수강권 2,000 + 초과
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            let a = allocate(&f, s.job_id, &bytes);
            let c = by_label(&a, "다솜");
            assert!(c.amount.voucher_over > 0, "초과금이 있는 학생이어야 시험이 된다");
            assert_eq!(c.after.p, c.amount.voucher);
            assert_eq!(c.after.p, 3_400);
            assert_eq!(c.after.o, c.amount.self_pay + c.amount.voucher_over);
            let j = by_label(&a, "자람");
            assert!(j.amount.voucher_over > 0);
            assert_eq!((j.after.p, j.after.q), (1_000, 2_000));
            assert_eq!(j.after.o, j.amount.personal);
        }

        #[test]
        fn 준비_완료_작업이어도_배분_전에_p3_를_다시_돌린다() {
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            assert_eq!(s.job_status, "READY");
            // READY 뒤에 강좌 연결이 사라졌다
            f.db.write(|c| course::remove(c, f.dp["만화"])).unwrap();
            let a = allocate(&f, s.job_id, &bytes);
            assert_eq!(a.status, "NOT_READY");
            assert_eq!(a.issues[0].code, allocation::code::P3_NOT_READY);
            assert!(!a.session.as_ref().unwrap().matching.p4_ready, "다시 계산한 P3 결과를 돌려준다");
            assert_eq!(after_rows(&f, s.job_id), 0);
            // 파일이 바뀌면 계산하지 않는다
            let other = xlsx(four("A", "로봇"));
            assert_eq!(f.db.write(|c| allocation::allocate(c, s.job_id, &other)).unwrap_err().code, "FILE_CHANGED");
            // 정산이 바뀌어도
            f.db.write(|c| course::set(c, f.dp["만화"], "PGM0002")).unwrap();
            f.generate();
            assert_eq!(f.db.write(|c| allocation::allocate(c, s.job_id, &bytes)).unwrap_err().code, "SETTLEMENT_CHANGED");
        }

        // ── 우선순위

        #[test]
        fn 부서_순위를_바꾸면_결과가_무효가_되고_다시_계산하면_달라진다() {
            let f = F::mixed();
            setup(&f);
            // 가온(A)이 로봇 · 미술을 듣고 이용권 40,000
            f.enroll("A", "미술");
            f.support("A", Program::Voucher, 40_000);
            f.generate();
            let mut rows = full_rows();
            let (label, c_, fees) = dept("미술");
            for (it, n) in ITEMS4.iter().zip(fees) {
                rows.push(line(who("A"), label, c_, *it, n));
            }
            let bytes = xlsx(rows);
            let s = confirmed(&f, &bytes);
            assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
            let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
            f.db.write(|c| allocation::save_dept_priority(c, f.ws, &[robot, art])).unwrap();
            let a1 = allocate(&f, s.job_id, &bytes);
            assert_eq!(a1.status, "DONE");
            let g1 = by_label(&a1, "가온").clone();
            assert_eq!(g1.order[0].label, "로봇과학B", "적용된 차례를 보여 준다");

            let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
            let dv = versions(&f);
            let n = f.db.write(|c| allocation::save_dept_priority(c, f.ws, &[art, robot])).unwrap();
            assert_eq!(n, 1, "끝나지 않은 작업 하나의 결과를 비웠다");
            assert_eq!(after_rows(&f, s.job_id), 0);
            assert_eq!(verify(&f, s.job_id, &bytes).status, "STALE");
            // 우선순위 저장은 정산 · 자료판을 건드리지 않는다
            assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
            assert_eq!(versions(&f), dv);
            assert_eq!(f.state(), "FRESH");

            let a2 = allocate(&f, s.job_id, &bytes);
            assert_eq!(a2.status, "DONE");
            let g2 = by_label(&a2, "가온");
            assert_eq!(g2.order[0].label, "창의미술B");
            assert_ne!(opq(&g1), opq(g2), "차례가 바뀌면 줄마다 결과가 바뀐다");
            assert_eq!((g1.after.p, g1.after.q, g1.after.o), (g2.after.p, g2.after.q, g2.after.o), "총액은 그대로");
            // 미술(29,200)부터 채운다: 미술 전부 P, 로봇은 남은 10,800
            let art_p: i64 = g2.lines.iter().filter(|l| l.department_id == art).map(|l| l.after.1).sum();
            assert_eq!(art_p, 29_200);
            assert_ne!(a1.fingerprint, a2.fingerprint);
            // 같은 차례로 다시 저장하면 결과를 비우지 않는다
            assert_eq!(f.db.write(|c| allocation::save_dept_priority(c, f.ws, &[art, robot])).unwrap(), 0);
            assert_eq!(verify(&f, s.job_id, &bytes).status, "DONE");
        }

        #[test]
        fn 공통_항목_순위는_모든_부서에_같고_바꾸면_결과가_무효가_된다() {
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            let a1 = allocate(&f, s.job_id, &bytes);
            // 다솜(미술, 이용권 3,400): 기본 차례면 강사료에 들어간다
            let c1 = by_label(&a1, "다솜").clone();
            let art = f.dp["미술"];
            let ins = |s: &AllocStudent, item: &str| s.lines.iter().find(|l| l.item_code == item).unwrap().after;
            assert_eq!(ins(&c1, 강사료).1, 3_400);
            let _ = art;
            let n = f
                .db
                .write(|c| allocation::save_common_item_order(c, f.ws, &[수용비.into(), 강사료.into()]))
                .unwrap();
            assert_eq!(n, 1);
            assert_eq!(verify(&f, s.job_id, &bytes).status, "STALE");
            let a2 = allocate(&f, s.job_id, &bytes);
            let c2 = by_label(&a2, "다솜");
            assert_eq!((ins(c2, 수용비).1, ins(c2, 강사료).1), (1_200, 2_200));
            assert_eq!(c2.order[0].items[..2], ["수용비".to_string(), "강사료".to_string()]);
            // 다른 부서(로봇) 학생에게도 같은 차례 — 자람(이용권 1,000 · 자유수강권 2,000)은 수용비부터
            let j2 = by_label(&a2, "자람");
            assert_eq!(ins(j2, 수용비), (0, 1_000, 500));
            assert_ne!(opq(by_label(&a1, "자람")), opq(j2));
            // 같은 차례를 다시 저장하면 결과를 비우지 않는다
            let n = f
                .db
                .write(|c| allocation::save_common_item_order(c, f.ws, &[수용비.into(), 강사료.into()]))
                .unwrap();
            assert_eq!(n, 0);
            assert_eq!(verify(&f, s.job_id, &bytes).status, "DONE");
            // v0.1.11 부서별 차례 표는 건드리지 않았다
            assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_item_priority"), 0);
        }

        #[test]
        fn 방과후드림_순위가_없으면_기존_정산_순위_그것도_없으면_정해진_차례다() {
            let f = F::mixed();
            let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
            let v = f.db.read(|c| allocation::priority_view(c, f.ws)).unwrap();
            assert_eq!(v.dept_source, "DEFAULT");
            assert_eq!(v.item_source, "DEFAULT");
            // 정해진 차례: 부서명 차례(글자 차례 — 로봇과학 · 만화애니메이션 · 창의미술 · 토탈공예)
            let labels: Vec<&str> = v.departments.iter().map(|d| d.label.as_str()).collect();
            assert_eq!(labels, ["로봇과학B", "만화애니메이션B", "창의미술B", "토탈공예미니어처B"]);
            assert!(v.departments.iter().all(|d| d.in_use));
            // 기존 정산 순위가 있으면 그것을 기본값으로(읽기만)
            f.db.write(|c| repo::priority::dept_save(c, f.ws, &[art, robot])).unwrap();
            f.generate();
            let v = f.db.read(|c| allocation::priority_view(c, f.ws)).unwrap();
            assert_eq!(v.dept_source, "SETTLEMENT");
            assert_eq!(v.departments[0].department_id, art);
            // 방과후드림 순위를 저장하면 그것이 먼저고, 기존 정산 순위는 그대로
            let settle_before = f.db.read(|c| repo::priority::dept_order(c, f.ws)).unwrap();
            f.db.write(|c| allocation::save_dept_priority(c, f.ws, &[robot])).unwrap();
            let v = f.db.read(|c| allocation::priority_view(c, f.ws)).unwrap();
            assert_eq!(v.dept_source, "DREAM");
            assert_eq!(v.departments[0].department_id, robot);
            assert_eq!(v.departments[1].department_id, art, "늘어놓지 않은 부서는 지금 차례 그대로 뒤에");
            assert_eq!(f.db.read(|c| repo::priority::dept_order(c, f.ws)).unwrap(), settle_before);
            assert_eq!(f.state(), "FRESH");
            // 같은 자료면 언제나 같은 차례
            let d1 = f.db.read(|c| allocation::priority_view(c, f.ws)).unwrap().digest;
            let d2 = f.db.read(|c| allocation::priority_view(c, f.ws)).unwrap().digest;
            assert_eq!(d1, d2);
        }

        #[test]
        fn 기존_정산_배분과_달라도_학생_총액이_같으면_정상이다() {
            let mut f = F::new();
            f.dept("로봇", "로봇과학", "B", vec![fee(강사료, 35_000), fee(수용비, 1_500)]);
            f.dept("미술", "창의미술", "B", vec![fee(강사료, 28_000), fee(수용비, 1_200)]);
            f.student("A", 3, "1", 1, "가온");
            f.enroll("A", "로봇");
            f.enroll("A", "미술");
            f.support("A", Program::Voucher, 40_000);
            let (robot, art) = (f.dp["로봇"], f.dp["미술"]);
            // 기존 정산: 로봇 먼저
            f.db.write(|c| repo::priority::dept_save(c, f.ws, &[robot, art])).unwrap();
            f.generate();
            setup(&f);
            let settle_voucher = |d: i64| {
                cnt(
                    &f.db,
                    &format!("SELECT COALESCE(SUM(amount), 0) FROM settlement_alloc WHERE department_id = {d} AND fund = 'VOUCHER'"),
                )
            };
            assert_eq!((settle_voucher(robot), settle_voucher(art)), (36_500, 3_500));
            // 방과후드림: 미술 먼저
            f.db.write(|c| allocation::save_dept_priority(c, f.ws, &[art, robot])).unwrap();
            let mut rows = four("A", "로봇");
            rows.extend(four("A", "미술"));
            let bytes = xlsx(rows);
            let s = confirmed(&f, &bytes);
            let a = allocate(&f, s.job_id, &bytes);
            assert_eq!(a.status, "DONE", "{:?}", a.issues);
            let st = &a.students[0];
            let p_of = |d: i64| st.lines.iter().filter(|l| l.department_id == d).map(|l| l.after.1).sum::<i64>();
            assert_eq!((p_of(robot), p_of(art)), (10_800, 29_200));
            assert_eq!(st.after.p, 40_000);
            assert_eq!(f.state(), "FRESH");
        }

        // ── 파일 전체성 — 취소 학생 포함

        /// A 수강 중(36,500) · B 취소 뒤 20,000 남음 · C 취소 뒤 0원
        fn cancelled() -> F {
            let mut f = F::new();
            f.dept("로봇", "로봇과학", "B", vec![fee(강사료, 35_000), fee(수용비, 1_500)]);
            f.student("A", 3, "1", 1, "가온");
            f.student("B", 3, "1", 2, "나래");
            f.student("C", 3, "1", 3, "다솜");
            for k in ["A", "B", "C"] {
                f.enroll(k, "로봇");
            }
            f.support("B", Program::Voucher, 15_000);
            let items = f.items.clone();
            for (k, left) in [("B", 20_000), ("C", 0)] {
                let eid = cnt(&f.db, &format!("SELECT id FROM enrollment WHERE student_id = {}", f.st[k]));
                f.db.write(|c| repo::enrollment::cancel(c, eid, Some(&[fee(강사료, left), fee(수용비, 0)]), "시험", &items))
                    .unwrap();
            }
            f.generate();
            assert_eq!(f.state(), "FRESH");
            setup(&f);
            f
        }

        fn b_rows() -> Vec<Vec<V>> {
            let mut rows = four("B", "로봇");
            rows[0][13] = V::N(20_000.0);
            rows[0][14] = V::N(20_000.0);
            rows[1][13] = V::N(0.0);
            rows[1][14] = V::N(0.0);
            rows
        }

        #[test]
        fn 취소했어도_정산에_금액이_남은_학생은_파일에_있어야_한다() {
            let f = cancelled();
            // A + B (C 는 0원이라 없어도 된다)
            let mut rows = four("A", "로봇");
            rows.extend(b_rows());
            let bytes = xlsx(rows);
            let s = confirmed(&f, &bytes);
            assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
            assert!(s.matching.absent.is_empty());
            assert_eq!((s.matching.file_total, s.matching.settlement_total), (56_500, 56_500));
            let a = allocate(&f, s.job_id, &bytes);
            assert_eq!(a.status, "DONE");
            let b = by_label(&a, "나래");
            assert_eq!((b.after.n, b.after.p, b.after.o), (20_000, 15_000, 5_000));
        }

        #[test]
        fn 취소_학생이_파일에_없으면_파일_전체_오류다() {
            let f = cancelled();
            let bytes = xlsx(four("A", "로봇"));
            let s = confirmed(&f, &bytes);
            let m = &s.matching;
            assert_eq!(m.students[0].state, State::Ready, "A 자체는 맞다");
            let r = m.file_reasons.iter().find(|r| r.code == code::SETTLEMENT_STUDENT_NOT_IN_FILE).unwrap();
            assert_eq!(r.state, State::Error);
            assert_eq!(r.detail.as_deref(), Some("1명 · 20000원"));
            assert_eq!(m.absent.len(), 1);
            assert_eq!((m.absent[0].student_id, m.absent[0].total, m.absent[0].cancelled), (f.st["B"], 20_000, true));
            assert!(m.file_reasons.iter().any(|r| r.code == code::FILE_TOTAL_MISMATCH));
            assert!(!m.p4_ready);
            assert_eq!(s.job_status, "BLOCKED");
            let a = allocate(&f, s.job_id, &bytes);
            assert_eq!(a.status, "NOT_READY");
            assert_eq!(after_rows(&f, s.job_id), 0);
        }

        #[test]
        fn 취소_학생의_남은_charge_가_파일에_없으면_오류다() {
            let f = cancelled();
            let mut rows = four("A", "로봇");
            let mut b = b_rows();
            b.remove(0); // 남은 강사료 20,000 줄이 빠졌다
            rows.extend(b);
            let s = confirmed(&f, &xlsx(rows));
            let b = student(&s.matching, "나래");
            assert!(codes(b).contains(&code::DB_CHARGE_NOT_IN_FILE), "{:?}", codes(b));
            assert_eq!(b.state, State::Error);
            assert!(!s.matching.p4_ready);
        }

        #[test]
        fn 학생별로_틀렸는데_파일_합계만_맞아도_통과하지_않는다() {
            let f = F::mixed();
            setup(&f);
            let mut rows = full_rows();
            // 가온 강사료 −1,000, 자람 강사료 +1,000 — 파일 ΣN 은 그대로
            let ga = rows.iter().position(|r| matches!(&r[6], V::S(x) if x == "가온")).unwrap();
            let ja = rows.iter().position(|r| matches!(&r[6], V::S(x) if x == "자람")).unwrap();
            rows[ga][13] = V::N(34_000.0);
            rows[ja][13] = V::N(36_000.0);
            let bytes = xlsx(rows);
            let s = confirmed(&f, &bytes);
            assert_eq!(s.matching.file_total, s.matching.settlement_total);
            assert!(!s.matching.file_reasons.iter().any(|r| r.code == code::FILE_TOTAL_MISMATCH));
            assert_eq!(s.matching.errors, 2);
            assert_eq!(allocate(&f, s.job_id, &bytes).status, "NOT_READY");
        }

        #[test]
        fn 정산이_바뀌어_전체가_맞지_않으면_예전_결과를_쓰지_않는다() {
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
            f.generate(); // 다시 정산
            let e = f.db.read(|c| allocation::verify(c, s.job_id, &bytes)).unwrap_err();
            assert_eq!(e.code, "SETTLEMENT_CHANGED");
            assert_eq!(
                f.db.read(|c| allocation::require_valid(c, s.job_id, &bytes)).unwrap_err().code,
                "SETTLEMENT_CHANGED"
            );
        }

        #[test]
        fn 다시_검증하거나_학생_확정이_바뀌면_배분_결과는_남지_않는다() {
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
            // P3 다시 검증 — 줄 기록을 새로 쓰므로 after 가 없다
            f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
            assert_eq!(after_rows(&f, s.job_id), 0);
            assert_eq!(verify(&f, s.job_id, &bytes).status, "STALE");
            assert_eq!(
                f.db.read(|c| allocation::require_valid(c, s.job_id, &bytes)).unwrap_err().code,
                "ALLOCATION_STALE"
            );
            // 다시 계산하면 같은 지문(같은 입력 · 같은 차례)
            let a1 = allocate(&f, s.job_id, &bytes);
            let a2 = allocate(&f, s.job_id, &bytes);
            assert_eq!(a1.fingerprint, a2.fingerprint);
            assert_eq!(
                serde_json::to_string(&a1.students).unwrap(),
                serde_json::to_string(&a2.students).unwrap()
            );
            // 저장된 after 를 누가 바꿔 놓으면 유효하지 않다
            f.db.write(|c| {
                c.execute(
                    &format!("UPDATE dream_job_row SET after_o = after_o + 1 WHERE job_id = {} AND source_row = 2", s.job_id),
                    [],
                )?;
                Ok(())
            })
            .unwrap();
            assert_eq!(verify(&f, s.job_id, &bytes).status, "STALE");
        }

        #[test]
        fn 배분_결과와_기록에_개인정보_원문이_없다() {
            let f = F::mixed();
            setup(&f);
            let bytes = xlsx(full_rows());
            let s = confirmed(&f, &bytes);
            let a = allocate(&f, s.job_id, &bytes);
            let mut v = serde_json::to_value(&a).unwrap();
            v["session"] = serde_json::Value::Null; // P3 결과는 P3 시험이 본다
            let json = v.to_string();
            for x in ["2024000001", "2024000006", "170101", "140606"] {
                assert!(!json.contains(x), "배분 결과에 {x}");
            }
            let events: String = f
                .db
                .read(|c| {
                    let v: Vec<String> = c
                        .prepare("SELECT kind || ':' || detail FROM dream_job_event")?
                        .query_map([], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    Ok(v.join("|"))
                })
                .unwrap();
            for x in ["2024000001", "170101", "가온", "나래", "다솜"] {
                assert!(!events.contains(x), "기록에 {x}");
            }
        }

        // ─────────────────────────────────────────────── P5 — 결과 Excel

        mod p5 {
            use super::*;
            use crate::excel::dream_file;
            use crate::excel::dream_patch;
            use crate::repo::dream::export;
            use crate::domain::dream_alloc::Totals;

            const DATE: &str = "20261030";

            /// 시험마다 따로 쓰는 빈 폴더(끝나면 지운다)
            struct Dir(std::path::PathBuf);
            impl Dir {
                fn new() -> Self {
                    let p = std::env::temp_dir().join(format!("dream-p5-{}", uuid::Uuid::new_v4().simple()));
                    std::fs::create_dir_all(&p).unwrap();
                    Dir(p)
                }
                fn path(&self, name: &str) -> std::path::PathBuf {
                    self.0.join(name)
                }
                fn files(&self) -> Vec<String> {
                    let mut v: Vec<String> = std::fs::read_dir(&self.0)
                        .unwrap()
                        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                        .collect();
                    v.sort();
                    v
                }
            }
            impl Drop for Dir {
                fn drop(&mut self) {
                    let _ = std::fs::remove_dir_all(&self.0);
                }
            }

            /// 원본을 디스크에 두고, P3 확정 · P4 배분까지 마친 작업
            fn ready(f: &F, rows: Vec<Vec<V>>, dir: &Dir) -> (i64, Vec<u8>, std::path::PathBuf) {
                let bytes = xlsx(rows);
                let src = dir.path("결제원본.xlsx");
                std::fs::write(&src, &bytes).unwrap();
                let s = confirmed(f, &bytes);
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                assert_eq!(allocate(f, s.job_id, &bytes).status, "DONE");
                (s.job_id, bytes, src)
            }

            fn export_to(f: &F, job: i64, bytes: &[u8], src: &std::path::Path, out: &std::path::Path, date: &str) -> crate::error::AppResult<export::Written> {
                let ep = f.db.read(|c| export::prepare(c, job, bytes, date))?;
                let w = export::write_result(src, out, bytes, &ep, dream_patch::patch)?;
                f.db.write(|c| export::record(c, job, &w, &ep))?;
                Ok(w)
            }

            fn exported(f: &F, job: i64) -> Vec<String> {
                f.db.read(|c| {
                    let v: Vec<String> = c
                        .prepare("SELECT detail FROM dream_job_event WHERE job_id = ?1 AND kind = 'RESULT_EXPORTED' ORDER BY id")?
                        .query_map([job], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    Ok(v)
                })
                .unwrap()
            }

            #[test]
            fn 검증된_배분을_행_그대로_옮기고_원본과_정산은_그대로다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
                let dv = versions(&f);
                let src_sha = crypto::sha256_hex(&std::fs::read(&src).unwrap());
                let out = dir.path("방과후드림_결제결과_202610_20261030.xlsx");
                let w = export_to(&f, job, &bytes, &src, &out, DATE).unwrap();

                // 결과 = 저장된 after 그대로, R = 고른 날짜, 0원 줄 포함
                let stored = f.db.read(|c| job::stored_after(c, job)).unwrap();
                let res = dream_file::parse("결과.xlsx", &std::fs::read(&out).unwrap());
                assert!(res.file_issues.is_empty());
                assert_eq!(res.rows.len(), 24);
                for r in &res.rows {
                    let (_, a) = stored.iter().find(|(x, _)| *x == r.source_row).unwrap();
                    let (o, p, q) = a.unwrap();
                    assert_eq!((r.personal_before, r.voucher_before, r.free_voucher_before), (Some(o), Some(p), Some(q)));
                    assert_eq!(r.transfer_date.as_deref(), Some(DATE));
                }
                assert!(res.rows.iter().any(|r| r.amount_n == Some(0) && r.transfer_date.as_deref() == Some(DATE)));
                // 파일 전체 = 정산 전체
                let a = verify(&f, job, &bytes);
                let sum = res.rows.iter().fold(Totals::default(), |mut t, r| {
                    t.add(r.amount_n.unwrap(), r.personal_before.unwrap(), r.voucher_before.unwrap(), r.free_voucher_before.unwrap());
                    t
                });
                assert_eq!(sum, a.settlement);
                assert_eq!(w.verify.a_to_n_changed, 0);
                assert_eq!(w.verify.non_target_same, w.verify.entries - 1);
                // 원본 파일 · 업무 자료 · 자료판 · 정산은 그대로
                assert_eq!(crypto::sha256_hex(&std::fs::read(&src).unwrap()), src_sha);
                assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
                assert_eq!(versions(&f), dv);
                assert_eq!(f.state(), "FRESH");
                // 기록: 날짜 · SHA · 파일 이름만(경로 · 개인정보 없음), P4 결과는 그대로 유효
                let ev = exported(&f, job);
                assert_eq!(ev.len(), 1);
                assert!(ev[0].starts_with(&format!("date={DATE} sha={}", w.sha256)), "{}", ev[0]);
                assert!(ev[0].ends_with("file=방과후드림_결제결과_202610_20261030.xlsx"));
                assert!(!ev[0].contains(&dir.0.to_string_lossy().to_string()));
                for x in ["2024000001", "170101", "가온"] {
                    assert!(!ev[0].contains(x));
                }
                assert_eq!(verify(&f, job, &bytes).status, "DONE");
                assert_eq!(after_rows(&f, job), 24);
                // 임시 파일이 남지 않는다
                assert_eq!(dir.files(), ["결제원본.xlsx", "방과후드림_결제결과_202610_20261030.xlsx"]);
            }

            #[test]
            fn 이체예정일이_없거나_실제_날짜가_아니면_만들지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                for d in ["", "2026-10-30", "20261332", "20260230", "2026103"] {
                    let e = export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), d).unwrap_err();
                    assert_eq!(e.code, "INVALID_TRANSFER_DATE", "{d}");
                }
                assert!(!dir.path("결과.xlsx").exists());
                assert!(exported(&f, job).is_empty());
            }

            #[test]
            fn 우선순위가_바뀌어_재계산이_필요하면_만들지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let art = f.dp["미술"];
                let _ = art;
                f.db.write(|c| allocation::save_common_item_order(c, f.ws, &[수용비.into(), 강사료.into()])).unwrap();
                let e = export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), DATE).unwrap_err();
                assert_eq!(e.code, "ALLOCATION_STALE");
                assert!(!dir.path("결과.xlsx").exists());
                // 다시 계산하면 만들 수 있다
                assert_eq!(allocate(&f, job, &bytes).status, "DONE");
                assert!(export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), DATE).is_ok());
            }

            #[test]
            fn 정산이_바뀌거나_낡으면_만들지_않는다() {
                let mut f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                f.dept("바둑", "바둑", "A", vec![fee(강사료, 1)]); // 자료가 바뀌어 STALE_DATA
                assert_ne!(f.state(), "FRESH");
                let e = export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), DATE).unwrap_err();
                assert_eq!(e.code, "SETTLEMENT_NOT_FRESH");
                f.generate(); // 다시 정산 — 정산 id 가 바뀐다
                let e = export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), DATE).unwrap_err();
                assert_eq!(e.code, "SETTLEMENT_CHANGED");
                assert!(!dir.path("결과.xlsx").exists());
                assert!(exported(&f, job).is_empty());
            }

            #[test]
            fn 원본_파일이_바뀌면_만들지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let other = xlsx(four("A", "로봇"));
                assert_eq!(f.db.read(|c| export::prepare(c, job, &other, DATE)).unwrap_err().code, "FILE_CHANGED");
                // 준비 뒤에 디스크의 원본이 바뀌어도 쓰지 않는다
                let ep = f.db.read(|c| export::prepare(c, job, &bytes, DATE)).unwrap();
                std::fs::write(&src, &other).unwrap();
                let e = export::write_result(&src, &dir.path("결과.xlsx"), &bytes, &ep, dream_patch::patch).unwrap_err();
                assert_eq!(e.code, "FILE_CHANGED");
                assert!(!dir.path("결과.xlsx").exists());
                assert_eq!(dir.files(), ["결제원본.xlsx"], "임시 파일이 남지 않는다");
            }

            #[test]
            fn 원본_자리에는_어떤_꼴로도_저장하지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let sha = crypto::sha256_hex(&std::fs::read(&src).unwrap());
                let sneaky = [
                    src.clone(),
                    dir.path("결제원본.XLSX"),
                    dir.0.join(".").join("결제원본.xlsx"),
                    dir.0.join("..").join(dir.0.file_name().unwrap()).join("결제원본.xlsx"),
                ];
                for p in sneaky {
                    let e = export_to(&f, job, &bytes, &src, &p, DATE).unwrap_err();
                    assert_eq!(e.code, "SAME_AS_SOURCE", "{}", p.display());
                }
                assert_eq!(crypto::sha256_hex(&std::fs::read(&src).unwrap()), sha);
                assert!(exported(&f, job).is_empty());
                // .xlsx 가 아니거나 폴더면 거절
                assert!(export_to(&f, job, &bytes, &src, &dir.path("결과.csv"), DATE).is_err());
                assert!(export_to(&f, job, &bytes, &src, &dir.0, DATE).is_err());
            }

            #[test]
            fn 저장할_수_없는_자리면_실패하고_아무것도_남기지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let e = export_to(&f, job, &bytes, &src, &dir.path("없는폴더").join("결과.xlsx"), DATE).unwrap_err();
                assert_eq!(e.code, "INVALID");
                // 결과 자리가 다른 프로그램에서 쓰는 중(여기서는 폴더라 옮길 수 없음)
                std::fs::create_dir(dir.path("잠김.xlsx")).unwrap();
                assert!(export_to(&f, job, &bytes, &src, &dir.path("잠김.xlsx"), DATE).is_err());
                assert_eq!(dir.files(), ["결제원본.xlsx", "잠김.xlsx"], "임시 파일 없음");
                assert!(exported(&f, job).is_empty());
            }

            #[test]
            fn 재검증에_걸리는_결과는_저장하지_않는다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let ep = f.db.read(|c| export::prepare(c, job, &bytes, DATE)).unwrap();
                // 잘못 만드는 writer — O 하나를 1원 틀리게 적는다
                let bad = |s: &[u8], p: &dream_patch::PatchPlan, r: &[u32]| {
                    let mut wrong = p.clone();
                    let i = wrong.rows.iter().position(|x| x.o > 0).unwrap();
                    wrong.rows[i].o -= 1;
                    wrong.rows[i].amount_n -= 1;
                    dream_patch::patch(s, &wrong, r)
                };
                let e = export::write_result(&src, &dir.path("결과.xlsx"), &bytes, &ep, bad).unwrap_err();
                assert!(e.code.starts_with("PATCH_") || e.code.starts_with("VERIFY_"), "{}", e.code);
                // 원본을 그대로 돌려주는 writer(아무것도 안 고침)
                let lazy = |s: &[u8], _: &dream_patch::PatchPlan, _: &[u32]| Ok((s.to_vec(), dream_patch::PatchReport::default()));
                let e = export::write_result(&src, &dir.path("결과.xlsx"), &bytes, &ep, lazy).unwrap_err();
                assert!(e.code.starts_with("VERIFY_"), "{}", e.code);
                assert!(!dir.path("결과.xlsx").exists());
                assert_eq!(dir.files(), ["결제원본.xlsx"]);
                assert!(exported(&f, job).is_empty());
            }

            #[test]
            fn 같은_결과로_다시_만들고_날짜를_바꿔도_opq_는_같다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let a = dir.path("결과1.xlsx");
                let b = dir.path("결과2.xlsx");
                export_to(&f, job, &bytes, &src, &a, DATE).unwrap();
                export_to(&f, job, &bytes, &src, &b, "20261105").unwrap();
                let ra = dream_file::parse("a.xlsx", &std::fs::read(&a).unwrap());
                let rb = dream_file::parse("b.xlsx", &std::fs::read(&b).unwrap());
                for (x, y) in ra.rows.iter().zip(&rb.rows) {
                    assert_eq!((x.personal_before, x.voucher_before, x.free_voucher_before), (y.personal_before, y.voucher_before, y.free_voucher_before));
                    assert_eq!((x.transfer_date.as_deref(), y.transfer_date.as_deref()), (Some(DATE), Some("20261105")));
                }
                let ev = exported(&f, job);
                assert_eq!(ev.len(), 2);
                assert!(ev[0].starts_with("date=20261030") && ev[1].starts_with("date=20261105"));
                // 이체예정일을 바꿔도 배분 결과는 유효하다
                assert_eq!(verify(&f, job, &bytes).status, "DONE");
                // 이미 있는 결과 파일 — 저장 창에서 덮어쓰기를 고른 경우
                export_to(&f, job, &bytes, &src, &a, "20261106").unwrap();
                let ra2 = dream_file::parse("a.xlsx", &std::fs::read(&a).unwrap());
                assert!(ra2.rows.iter().all(|r| r.transfer_date.as_deref() == Some("20261106")));
                assert_eq!(dir.files(), ["결과1.xlsx", "결과2.xlsx", "결제원본.xlsx"]);
            }

            #[test]
            fn 취소_학생이_있어도_정산_전체를_담은_파일만_만든다() {
                let f = cancelled();
                let dir = Dir::new();
                let mut rows = four("A", "로봇");
                rows.extend(b_rows());
                let (job, bytes, src) = ready(&f, rows, &dir);
                let w = export_to(&f, job, &bytes, &src, &dir.path("결과.xlsx"), DATE).unwrap();
                let res = dream_file::parse("결과.xlsx", &std::fs::read(dir.path("결과.xlsx")).unwrap());
                let n: i64 = res.rows.iter().map(|r| r.amount_n.unwrap()).sum();
                assert_eq!(n, 56_500, "취소했지만 금액이 남은 B 포함");
                assert_eq!(w.verify.data_rows, 8);
                // A 만 든 파일은 P4 부터 막혀 결과도 만들 수 없다
                let only_a = xlsx(four("A", "로봇"));
                let s = confirmed(&f, &only_a);
                assert!(!s.matching.p4_ready);
                let e = f.db.read(|c| export::prepare(c, s.job_id, &only_a, DATE)).unwrap_err();
                assert_eq!(e.code, "ALLOCATION_STALE");
            }

            #[test]
            fn 결과_파일을_다시_넣어도_같은_배분이_나온다() {
                let f = F::mixed();
                setup(&f);
                let dir = Dir::new();
                let (job, bytes, src) = ready(&f, full_rows(), &dir);
                let out = dir.path("결과.xlsx");
                export_to(&f, job, &bytes, &src, &out, DATE).unwrap();
                let again = std::fs::read(&out).unwrap();
                let s = confirmed(&f, &again);
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                let a2 = allocate(&f, s.job_id, &again);
                assert_eq!(a2.status, "DONE");
                let first = f.db.read(|c| job::stored_after(c, job)).unwrap();
                let second = f.db.read(|c| job::stored_after(c, s.job_id)).unwrap();
                assert_eq!(first, second);
            }
        }

        // ─────────────────────────────────────────────── P6 — 적용 학년 · 강좌 일괄 연결 · 공통 항목 차례

        mod p6 {
            use super::*;
            use crate::excel::dream_patch;
            use crate::repo::dream::export;

            type Who = (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str);

            /// 학년별 학생 — (개인참조번호, 학년, 반, 번호, 이름, 생년월일). 모두 가짜.
            fn kid(k: &str) -> Who {
                match k {
                    "G1" => ("2024100001", "1학년", "1반", "1", "한별", "190101"),
                    "G2" => ("2024100002", "2학년", "1반", "1", "두리", "180101"),
                    "G3a" => ("2024100003", "3학년", "1반", "1", "세찬", "170101"),
                    "G3b" => ("2024100004", "3학년", "1반", "2", "세린", "170202"),
                    "G3c" => ("2024100005", "3학년", "1반", "3", "세움", "170303"),
                    "G4" => ("2024100006", "4학년", "1반", "1", "네오", "160101"),
                    "G5" => ("2024100007", "5학년", "1반", "1", "다섯", "150101"),
                    "G6" => ("2024100008", "6학년", "1반", "1", "여섯", "140101"),
                    _ => unreachable!(),
                }
            }

            /// 로봇과학B 한 부서에 1~6학년이 모두 다닌다. 3학년 세찬은 이용권 20,000.
            fn grades_fixture() -> F {
                let mut f = F::new();
                f.dept("로봇", "로봇과학", "B", vec![fee(강사료, 35_000), fee(수용비, 1_500)]);
                for (k, g, no, name) in [
                    ("G1", 1, 1, "한별"),
                    ("G2", 2, 1, "두리"),
                    ("G3a", 3, 1, "세찬"),
                    ("G3b", 3, 2, "세린"),
                    ("G4", 4, 1, "네오"),
                    ("G5", 5, 1, "다섯"),
                    ("G6", 6, 1, "여섯"),
                ] {
                    f.student(k, g, "1", no, name);
                    f.enroll(k, "로봇");
                }
                f.support("G3a", Program::Voucher, 20_000);
                f.generate();
                assert_eq!(f.state(), "FRESH");
                f
            }

            fn rows_of(keys: &[&str]) -> Vec<Vec<V>> {
                let (label, c, fees) = dept("로봇");
                keys.iter()
                    .flat_map(|k| ITEMS4.iter().zip(fees).map(move |(it, n)| line(kid(k), label, c, *it, n)))
                    .collect()
            }

            fn file_codes(m: &Matching) -> Vec<&'static str> {
                m.file_reasons.iter().map(|r| r.code).collect()
            }

            fn ws_versions(f: &F) -> (i64, i64) {
                f.db.read(|c| {
                    let ws = repo::year::get_workspace(c, f.ws)?;
                    Ok((ws.data_version, repo::year::get_year(c, ws.year_id)?.data_version))
                })
                .unwrap()
            }

            // ── 적용 학년

            #[test]
            fn 적용_학년이_없으면_시작하지_않고_매칭도_하지_않는다() {
                let f = grades_fixture();
                f.db.write(|c| setting::set_org_code(c, "I100000987")).unwrap();
                f.db.write(|c| workspace_setting::set_billing_month(c, f.ws, "202610")).unwrap();
                let bytes = xlsx(rows_of(&["G3a", "G3b"]));
                let sha = crypto::sha256_hex(&bytes);
                let e = f.db.write(|c| session::start(c, f.ws, "결제.xlsx", &bytes, &sha, "0.1.11")).unwrap_err();
                assert_eq!(e.code, "APPLIED_GRADES_NOT_SET");
                assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_job"), 0);
                let file = crate::excel::dream_file::parse("결제.xlsx", &bytes);
                let m = f.db.read(|c| crate::repo::dream::matching::run(c, f.ws, &file, None)).unwrap();
                assert_eq!(file_codes(&m), [code::APPLIED_GRADES_NOT_SET]);
                assert!(m.students.is_empty() && !m.p4_ready);
                // 빈 선택 · 범위 밖은 저장하지 않는다
                assert_eq!(f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[])).unwrap_err().code, "APPLIED_GRADES_NOT_SET");
                assert!(f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[0])).is_err());
                assert!(f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[10])).is_err());
            }

            #[test]
            fn 삼학년만_적용하면_다른_학년은_파일에_없어도_되고_합계도_삼학년만이다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b"]));
                let s = confirmed(&f, &bytes);
                let m = &s.matching;
                assert!(m.p4_ready, "{:?}", m.file_reasons);
                assert_eq!(m.applied_grades, [3]);
                assert_eq!(m.universe_students, 2);
                // 학교 전체(7명 × 36,500)가 아니라 3학년 2명
                assert_eq!((m.file_total, m.settlement_total), (73_000, 73_000));
                assert!(m.absent.is_empty());
                let a = allocate(&f, s.job_id, &bytes);
                assert_eq!(a.status, "DONE", "{:?}", a.issues);
                assert_eq!(a.applied_grades, [3]);
                assert_eq!((a.settlement.n, a.settlement.p, a.settlement.o), (73_000, 20_000, 53_000));
                assert_eq!(a.after, a.settlement);
            }

            #[test]
            fn 적용_학년_학생이_빠지면_막는다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let s = confirmed(&f, &xlsx(rows_of(&["G3a"])));
                let r = s.matching.file_reasons.iter().find(|r| r.code == code::SETTLEMENT_STUDENT_NOT_IN_FILE).unwrap();
                assert_eq!(r.detail.as_deref(), Some("1명 · 36500원"));
                assert_eq!(s.matching.absent.len(), 1);
                assert_eq!(s.matching.absent[0].student_id, f.st["G3b"]);
                assert!(!s.matching.p4_ready);
            }

            #[test]
            fn 적용_학년이_아닌_학생이_파일에_있으면_빼지_않고_막는다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b", "G4"]));
                let s = confirmed(&f, &bytes);
                let m = &s.matching;
                let r = m.file_reasons.iter().find(|r| r.code == code::FILE_STUDENT_OUTSIDE_APPLIED_GRADES).unwrap();
                assert_eq!((r.state, r.detail.as_deref()), (State::Error, Some("4학년 1명")));
                assert_eq!(m.outside, [(4, 1)]);
                let g4 = student(m, "네오");
                assert_eq!(g4.state, State::Error);
                assert!(codes(g4).contains(&code::FILE_STUDENT_OUTSIDE_APPLIED_GRADES));
                assert!(!m.p4_ready);
                // 4학년 금액을 무시하고 3학년만 계산하지 않는다
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "NOT_READY");
            }

            #[test]
            fn 삼사학년을_적용하면_삼사학년만_있어야_하고_나머지는_없어도_된다() {
                let f = grades_fixture();
                setup_grades(&f, &[4, 3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b", "G4"]));
                let s = confirmed(&f, &bytes);
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                assert_eq!(s.matching.applied_grades, [3, 4]);
                assert_eq!((s.matching.universe_students, s.matching.settlement_total), (3, 109_500));
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
                // 4학년이 빠지면 막는다
                let s2 = confirmed(&f, &xlsx(rows_of(&["G3a", "G3b"])));
                assert!(file_codes(&s2.matching).contains(&code::SETTLEMENT_STUDENT_NOT_IN_FILE));
            }

            #[test]
            fn 전_학년을_적용하면_예전처럼_학교_전체가_대상이다() {
                let f = grades_fixture();
                setup_grades(&f, &[1, 2, 3, 4, 5, 6]);
                let all = ["G1", "G2", "G3a", "G3b", "G4", "G5", "G6"];
                let s = confirmed(&f, &xlsx(rows_of(&all)));
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                assert_eq!(s.matching.settlement_total, 7 * 36_500);
                // 한 명이라도 빠지면 막는다(v0.1.11 과 같다)
                let s2 = confirmed(&f, &xlsx(rows_of(&all[..6])));
                assert!(file_codes(&s2.matching).contains(&code::SETTLEMENT_STUDENT_NOT_IN_FILE));
            }

            #[test]
            fn 적용_학년_안의_취소_학생도_금액이_남으면_대상이다() {
                let mut f = grades_fixture();
                f.student("G3c", 3, "1", 3, "세움");
                f.enroll("G3c", "로봇");
                let items = f.items.clone();
                for (k, left) in [("G3b", 20_000), ("G3c", 0)] {
                    let eid = cnt(&f.db, &format!("SELECT id FROM enrollment WHERE student_id = {}", f.st[k]));
                    f.db.write(|c| repo::enrollment::cancel(c, eid, Some(&[fee(강사료, left), fee(수용비, 0)]), "시험", &items))
                        .unwrap();
                }
                f.generate();
                setup_grades(&f, &[3]);
                let mut b = rows_of(&["G3b"]);
                b[0][13] = V::N(20_000.0);
                b[0][14] = V::N(20_000.0);
                b[1][13] = V::N(0.0);
                b[1][14] = V::N(0.0);
                let mut rows = rows_of(&["G3a"]);
                rows.extend(b.clone());
                // A + B — 정상(C 는 0원, 4학년 D 는 적용 학년이 아님)
                let s = confirmed(&f, &xlsx(rows));
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                assert_eq!((s.matching.universe_students, s.matching.settlement_total), (2, 56_500));
                // B 가 빠지면 막는다
                let s2 = confirmed(&f, &xlsx(rows_of(&["G3a"])));
                let ab = &s2.matching.absent;
                assert_eq!((ab.len(), ab[0].student_id, ab[0].cancelled), (1, f.st["G3b"], true));
            }

            #[test]
            fn 적용_학년을_바꾸면_진행_중_작업을_닫고_되돌려도_예전_결과는_살아나지_않는다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b"]));
                let s = confirmed(&f, &bytes);
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
                let before = f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap();
                let dv = ws_versions(&f);
                // 같은 집합(차례만 다름)은 바뀐 것이 아니다
                let (g, changed) = f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[3, 3])).unwrap();
                assert_eq!((g, changed), (vec![3], false));
                assert_eq!(verify(&f, s.job_id, &bytes).status, "DONE");
                // [3] → [3, 4] — 작업을 닫는다(배분 결과도 비운다)
                let (_, changed) = f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[3, 4])).unwrap();
                assert!(changed);
                let closed = f.db.write(|c| job::close_open(c, f.ws, "GRADES_CHANGED")).unwrap();
                assert_eq!(closed, 1);
                assert_eq!(after_rows(&f, s.job_id), 0);
                assert_eq!(f.db.read(|c| job::get(c, s.job_id)).unwrap().status, "FAILED");
                assert_eq!(f.db.write(|c| allocation::allocate(c, s.job_id, &bytes)).unwrap_err().code, "INVALID");
                // [3] 으로 되돌려도 닫힌 작업은 다시 쓰지 않는다 — 다시 시작해야 한다
                f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[3])).unwrap();
                assert!(f.db.read(|c| allocation::require_valid(c, s.job_id, &bytes)).is_err());
                // 적용 학년 저장은 정산 · 자료판을 건드리지 않는다
                assert_eq!(f.db.read(|c| Ok(fingerprint(c, f.ws))).unwrap(), before);
                assert_eq!(ws_versions(&f), dv);
                assert_eq!(f.state(), "FRESH");
                // 기록은 남아 있다
                assert!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_job_event WHERE job_id = {} AND kind = 'ALLOCATE'", s.job_id)) >= 1);
            }

            #[test]
            fn 지문에_적용_학년이_들어간다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b"]));
                let s = confirmed(&f, &bytes);
                let a = allocate(&f, s.job_id, &bytes);
                // 작업을 닫지 않고 학년만 바꾼 경우(방어) — 지문이 달라 유효하지 않다
                f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[3, 4])).unwrap();
                let v = f.db.read(|c| allocation::verify(c, s.job_id, &bytes)).unwrap();
                assert_ne!(v.status, "DONE");
                f.db.write(|c| workspace_setting::set_applied_grades(c, f.ws, &[3])).unwrap();
                let v = f.db.read(|c| allocation::verify(c, s.job_id, &bytes)).unwrap();
                assert_eq!((v.status, v.fingerprint), ("DONE", a.fingerprint));
            }

            #[test]
            fn 삼학년만의_결과_파일은_그_행만_바꾸고_기록에_학년을_남긴다() {
                let f = grades_fixture();
                setup_grades(&f, &[3]);
                let bytes = xlsx(rows_of(&["G3a", "G3b"]));
                let dir = std::env::temp_dir().join(format!("dream-p6-{}", uuid::Uuid::new_v4().simple()));
                std::fs::create_dir_all(&dir).unwrap();
                let src = dir.join("원본.xlsx");
                std::fs::write(&src, &bytes).unwrap();
                let s = confirmed(&f, &bytes);
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
                let ep = f.db.read(|c| export::prepare(c, s.job_id, &bytes, "20261030")).unwrap();
                assert_eq!(ep.applied_grades, [3]);
                let out = dir.join("결과.xlsx");
                let w = export::write_result(&src, &out, &bytes, &ep, dream_patch::patch).unwrap();
                f.db.write(|c| export::record(c, s.job_id, &w, &ep)).unwrap();
                assert_eq!((w.verify.data_rows, w.verify.a_to_n_changed), (8, 0));
                let res = crate::excel::dream_file::parse("결과.xlsx", &std::fs::read(&out).unwrap());
                assert_eq!(res.rows.len(), 8, "3학년 두 명의 줄만 — 새 줄을 만들지 않는다");
                assert_eq!(res.rows.iter().map(|r| r.voucher_before.unwrap()).sum::<i64>(), 20_000);
                let ev: String = f
                    .db
                    .read(|c| Ok(c.query_row("SELECT detail FROM dream_job_event WHERE kind = 'RESULT_EXPORTED'", [], |r| r.get(0))?))
                    .unwrap();
                assert!(ev.contains(" grades=3 "), "{ev}");
                let _ = std::fs::remove_dir_all(&dir);
            }

            // ── 강좌코드 일괄 연결

            /// F::mixed + 코딩교실B(가온). 파일의 미술 강좌명은 표기가 달라 '이름 같음' 추천이 없다.
            fn five_courses() -> (F, Vec<u8>) {
                let mut f = F::mixed();
                f.dept("코딩", "코딩교실", "B", vec![fee(강사료, 10_000)]);
                f.enroll("A", "코딩");
                f.generate();
                setup(&f);
                let mut rows = full_rows();
                for r in rows.iter_mut().filter(|r| matches!(&r[10], V::S(c) if c == "PGM0003")) {
                    r[9] = t("창의 미술B");
                }
                for (it, n) in ITEMS4.iter().zip([10_000, 0, 0, 0]) {
                    rows.push(line(who("A"), "코딩 교실", "PGM0005", *it, n));
                }
                (f, xlsx(rows))
            }

            fn maps(f: &F) -> Vec<(String, i64)> {
                f.db.read(|c| course::list(c, f.ws)).unwrap().into_iter().map(|m| (m.course_code, m.department_id)).collect()
            }

            #[test]
            fn 추천은_저장하지_않고_고른_것만_한꺼번에_잇는다() {
                let (f, bytes) = five_courses();
                let s = start(&f, &bytes);
                assert_eq!(s.matching.courses.len(), 5);
                assert!(maps(&f).is_empty(), "추천만으로는 아무것도 저장하지 않는다");
                let same: Vec<&str> = s
                    .matching
                    .courses
                    .iter()
                    .filter(|c| c.suggestions.iter().filter(|x| x.why.contains(&"SAME_NAME")).count() == 1)
                    .map(|c| c.course_code.as_str())
                    .collect();
                assert_eq!(same, ["PGM0001", "PGM0002", "PGM0004"], "이름이 정확히 같은 셋만");
                // 셋은 추천 그대로, 미술은 직접, 코딩은 아직
                let picks: Vec<(String, i64)> = vec![
                    ("PGM0001".into(), f.dp["로봇"]),
                    ("PGM0002".into(), f.dp["만화"]),
                    ("PGM0004".into(), f.dp["공예"]),
                    ("PGM0003".into(), f.dp["미술"]),
                ];
                let n = f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &picks)).unwrap();
                assert_eq!(n, 4);
                assert_eq!(maps(&f).len(), 4);
                let s = f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
                let left: Vec<&str> = s.matching.courses.iter().filter(|c| c.department_id.is_none()).map(|c| c.course_code.as_str()).collect();
                assert_eq!(left, ["PGM0005"]);
                let s = identities_all(&f, &s, &bytes);
                assert!(!s.matching.p4_ready, "미연결 강좌가 남으면 준비 완료가 아니다");
                // 마지막 하나를 이으면 진행 가능
                f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &[("PGM0005".into(), f.dp["코딩"])])).unwrap();
                let s = f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                // 같은 연결을 다시 보내면 바뀐 것이 없다
                assert_eq!(f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &picks)).unwrap(), 0);
            }

            #[test]
            fn 하나라도_안_되면_아무것도_저장하지_않는다() {
                let (f, bytes) = five_courses();
                let s = start(&f, &bytes);
                let ok = ("PGM0001".to_string(), f.dp["로봇"]);
                // 두 코드를 한 부서에
                let e = f
                    .db
                    .write(|c| session::confirm_courses(c, s.job_id, &bytes, &[ok.clone(), ("PGM0002".into(), f.dp["로봇"])]))
                    .unwrap_err();
                assert_eq!(e.code, "CONFLICT");
                assert!(maps(&f).is_empty(), "반쯤 저장되지 않는다");
                // 같은 코드를 두 번
                assert!(f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &[ok.clone(), ok.clone()])).is_err());
                // 파일에 없는 코드 · 다른 작업공간 부서
                assert!(f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &[ok.clone(), ("PGM9999".into(), f.dp["만화"])])).is_err());
                let mut g = f;
                let other_ws = g.workspace("11월", "2026-11-01", "2026-11-30");
                let other = g.dept_in(other_ws, "남의", "남의부서", "A", vec![fee(강사료, 1)]);
                let e = g.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &[ok.clone(), ("PGM0002".into(), other)]));
                assert!(e.is_err());
                assert!(maps(&g).is_empty());
                // 이미 다른 코드에 이어진 부서로 — 묶음 전체를 거절
                g.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, std::slice::from_ref(&ok))).unwrap();
                let e = g
                    .db
                    .write(|c| session::confirm_courses(c, s.job_id, &bytes, &[("PGM0004".into(), g.dp["공예"]), ("PGM0002".into(), g.dp["로봇"])]))
                    .unwrap_err();
                assert_eq!(e.code, "CONFLICT");
                assert_eq!(maps(&g), [("PGM0001".to_string(), g.dp["로봇"])]);
            }

            #[test]
            fn 연결을_바꾸면_배분_결과가_무효가_되고_서로_맞바꾸기도_한_번에_된다() {
                let (f, bytes) = five_courses();
                let s = start(&f, &bytes);
                let all: Vec<(String, i64)> = [("PGM0001", "로봇"), ("PGM0002", "만화"), ("PGM0003", "미술"), ("PGM0004", "공예"), ("PGM0005", "코딩")]
                    .iter()
                    .map(|(c, d)| (c.to_string(), f.dp[*d]))
                    .collect();
                f.db.write(|c| session::confirm_courses(c, s.job_id, &bytes, &all)).unwrap();
                let s = f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
                let s = identities_all(&f, &s, &bytes);
                assert!(s.matching.p4_ready, "{:?}", s.matching.file_reasons);
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
                let dv = ws_versions(&f);
                // 로봇 ↔ 만화 맞바꾸기(한 번에) — 최종 연결이 1:1 이면 받는다
                let swap = vec![("PGM0001".to_string(), f.dp["만화"]), ("PGM0002".to_string(), f.dp["로봇"])];
                let n = f.db.write(|c| {
                    let n = session::confirm_courses(c, s.job_id, &bytes, &swap)?;
                    session::revalidate(c, s.job_id, &bytes)?;
                    Ok(n)
                })
                .unwrap();
                assert_eq!(n, 2);
                assert_eq!(after_rows(&f, s.job_id), 0, "배분 결과가 남지 않는다");
                assert_eq!(verify(&f, s.job_id, &bytes).status, "NOT_READY", "학생이 그 부서를 듣지 않으니 매칭부터 막힌다");
                assert_eq!(ws_versions(&f), dv);
                assert_eq!(f.state(), "FRESH");
            }

            // ── 공통 항목 차례 — v0.1.11 부서별 차례와의 호환

            #[test]
            fn 예전_부서별_차례가_없으면_정산_순위나_기본_차례를_쓴다() {
                let f = F::mixed();
                let o = f.db.read(|c| priority::common_item_order(c, f.ws)).unwrap();
                assert_eq!((o.source, o.order[0].as_str()), ("DEFAULT", 강사료));
            }

            #[test]
            fn 예전_부서별_차례가_모두_같으면_그_차례를_그대로_쓴다() {
                let f = F::mixed();
                setup(&f);
                let order: Vec<String> = vec![재료비.into(), 수용비.into(), 강사료.into(), 교재비.into()];
                for d in ["로봇", "만화", "미술", "공예"] {
                    f.db.write(|c| priority::save_item_order(c, f.dp[d], &order)).unwrap();
                }
                let o = f.db.read(|c| priority::common_item_order(c, f.ws)).unwrap();
                assert_eq!((o.source, o.order.clone()), ("LEGACY", order.clone()));
                let bytes = xlsx(full_rows());
                let s = confirmed(&f, &bytes);
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE", "v0.1.11 과 같은 차례로 바로 배분할 수 있다");
            }

            #[test]
            fn 예전_부서별_차례가_다르면_고르지_않고_공통_차례를_저장해야_배분한다() {
                let f = F::mixed();
                setup(&f);
                let a: Vec<String> = vec![강사료.into(), 수용비.into(), 교재비.into(), 재료비.into()];
                let b: Vec<String> = vec![재료비.into(), 교재비.into(), 수용비.into(), 강사료.into()];
                f.db.write(|c| priority::save_item_order(c, f.dp["로봇"], &a)).unwrap();
                f.db.write(|c| priority::save_item_order(c, f.dp["만화"], &b)).unwrap();
                let o = f.db.read(|c| priority::common_item_order(c, f.ws)).unwrap();
                assert_eq!(o.source, "CONFLICT");
                let bytes = xlsx(full_rows());
                let s = confirmed(&f, &bytes);
                let r = allocate(&f, s.job_id, &bytes);
                assert_eq!((r.status, r.issues[0].code), ("FAILED", allocation::code::ITEM_ORDER_NOT_CONFIRMED));
                // 공통 차례를 저장하면 그것만 쓴다 — 예전 부서별 차례(로봇 강사료 먼저)는 보지 않는다
                f.db.write(|c| allocation::save_common_item_order(c, f.ws, &b)).unwrap();
                let o = f.db.read(|c| priority::common_item_order(c, f.ws)).unwrap();
                assert_eq!((o.source, o.order.clone()), ("DREAM", b.clone()));
                let s = f.db.write(|c| session::revalidate(c, s.job_id, &bytes)).unwrap();
                let r = allocate(&f, s.job_id, &bytes);
                assert_eq!(r.status, "DONE");
                let ga = by_label(&r, "자람"); // 로봇 — 재료비가 없으니 교재비(0) → 수용비 → 강사료
                let first_paid = ga.lines.iter().filter(|l| l.after.1 > 0).min_by_key(|l| l.seq).unwrap();
                assert_eq!(first_paid.item_code, 수용비);
                assert_eq!(cnt(&f.db, "SELECT COUNT(*) FROM dream_item_priority"), 8, "예전 표는 지우지 않는다");
                assert_eq!(f.state(), "FRESH");
            }

            #[test]
            fn 항목_차례는_작업공간마다_따로이고_한쪽을_바꿔도_다른_쪽은_그대로다() {
                let f = F::mixed();
                setup(&f);
                let a_ws = f.ws;
                let b_ws = f.workspace("11월", "2026-11-01", "2026-11-30");
                let bytes = xlsx(full_rows());
                let s = confirmed(&f, &bytes);
                assert_eq!(allocate(&f, s.job_id, &bytes).status, "DONE");
                let order = |ws: i64| f.db.read(|c| priority::common_item_order(c, ws)).unwrap();
                let dv = ws_versions(&f);
                let a_order: Vec<String> = vec![강사료.into(), 수용비.into(), 교재비.into(), 재료비.into()];
                let b_order: Vec<String> = vec![재료비.into(), 교재비.into(), 수용비.into(), 강사료.into()];

                // B 를 저장해도 A 의 배분은 그대로 유효하다
                assert_eq!(f.db.write(|c| allocation::save_common_item_order(c, b_ws, &b_order)).unwrap(), 0);
                assert_eq!(verify(&f, s.job_id, &bytes).status, "DONE");
                assert_eq!((order(b_ws).source, order(b_ws).order), ("DREAM", b_order.clone()));
                assert_eq!(order(a_ws).source, "DEFAULT");
                // A 에 지금 쓰던 차례(기본값)를 그대로 저장하는 것은 바뀐 것이 아니다
                assert_eq!(f.db.write(|c| allocation::save_common_item_order(c, a_ws, &a_order)).unwrap(), 0);
                assert_eq!(verify(&f, s.job_id, &bytes).status, "DONE");
                // A → B → A 로 읽어도 각자 제 차례
                assert_eq!(order(a_ws).order, a_order);
                assert_eq!(order(b_ws).order, b_order);
                assert_eq!(order(a_ws).order, a_order);
                // A 를 바꾸면 A 만 다시 계산 — B 의 차례는 그대로
                let a2: Vec<String> = vec![재료비.into(), 강사료.into(), 수용비.into(), 교재비.into()];
                assert_eq!(f.db.write(|c| allocation::save_common_item_order(c, a_ws, &a2)).unwrap(), 1);
                assert_eq!(verify(&f, s.job_id, &bytes).status, "STALE");
                assert_eq!(order(a_ws).order, a2);
                assert_eq!(order(b_ws).order, b_order);
                // 같은 작업공간의 모든 부서에 같은 차례 — 배분 근거 표시도 부서마다 같다
                let r = allocate(&f, s.job_id, &bytes);
                assert_eq!(r.status, "DONE");
                let names: Vec<String> = a2.iter().map(|c| crate::domain::dream::ITEMS.iter().find(|x| x.2 == c).unwrap().1.to_string()).collect();
                assert!(r.students.iter().flat_map(|st| &st.order).all(|d| d.items == names));
                // 정산 · 자료판은 그대로
                assert_eq!(ws_versions(&f), dv);
                assert_eq!(f.state(), "FRESH");
                // B 를 지우면 B 의 차례도 사라지고, 같은 id 로 새 작업공간이 생겨도 물려받지 않는다
                f.db.write(|c| {
                    c.execute("DELETE FROM workspace WHERE id = ?1", [b_ws])?;
                    Ok(())
                })
                .unwrap();
                assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_item_order WHERE workspace_id = {b_ws}")), 0);
                assert_eq!(cnt(&f.db, &format!("SELECT COUNT(*) FROM dream_item_order WHERE workspace_id = {a_ws}")), 4);
                let c_ws = f.workspace("12월", "2026-12-01", "2026-12-31");
                assert_ne!(order(c_ws).source, "DREAM", "새 작업공간({c_ws})이 지운 작업공간({b_ws})의 차례를 물려받았다");
                assert!(f.db.read(|c| workspace_setting::applied_grades(c, c_ws)).unwrap().is_empty());
            }
        }
    }
}
