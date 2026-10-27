//! 정산 상세내역 (v0.1.10) — 수익자 · 방과후 이용권 · 자유수강권.
//!
//! 학생은 모두 가명이다. 금액은 실제 정산 엔진(`generate`)이 만든 배분을 쓴다.
//! 같은 칸 여러 배분(사례 H)만은 엔진이 만들지 않는 모양이라 배분 줄을 직접 넣는다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use calamine::{open_workbook_auto, Data, Reader};

use crate::db::Db;
use crate::domain::Program;
use crate::excel;
use crate::model::{
    CostItem, DepartmentInput, EligibilityInput, EnrollmentInput, Fee, GrantInput, ProgramRow,
    SelfPayReport, StudentInput, WorkspaceInput,
};
use crate::repo;
use crate::repo::settle_detail::{DetailKind, DetailRow};

const 강사료: &str = "INSTRUCTOR";
const 수용비: &str = "OPERATION";
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
    fn empty() -> Self {
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
                        name: "10월".into(),
                        start_date: "2026-10-01".into(),
                        end_date: "2026-10-31".into(),
                        note: None,
                    },
                )
            })
            .unwrap();
        let items = db.read(repo::cost_items).unwrap();
        for p in [Program::Voucher, Program::FreeVoucher] {
            db.write(|c| repo::policy::save(c, year, p.code(), 1_000_000, false, "", &[]))
                .unwrap();
        }
        Self {
            db,
            year,
            ws,
            items,
            st: HashMap::new(),
            dp: HashMap::new(),
        }
    }

    fn student(&mut self, key: &'static str, grade: i64, class_no: &str, no: i64, name: &str) {
        let id = self
            .db
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
            .unwrap();
        self.st.insert(key, id);
    }

    fn dept(&mut self, key: &'static str, name: &str, class_name: &str, fees: Vec<Fee>) {
        let ws = self.ws;
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
    }

    fn enroll(&self, student: &str, dept: &str) {
        let (s, d) = (self.st[student], self.dp[dept]);
        self.db
            .write(|c| {
                repo::enrollment::create(
                    c,
                    self.ws,
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

    /// 자격 + 학생별 연간 한도. 지원기간이 없으므로 연간 한도가 곧 쓸 수 있는 돈이다.
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

    fn generate(&self) {
        self.db.write(|c| repo::settle::generate(c, self.ws)).unwrap();
    }

    fn rows(&self, kind: DetailKind) -> Vec<DetailRow> {
        self.db
            .read(|c| repo::settle_detail::rows(c, self.ws, kind, &self.items))
            .unwrap()
    }

    fn of(&self, kind: DetailKind, student: &str) -> Vec<DetailRow> {
        let id = self.st[student];
        self.rows(kind).into_iter().filter(|r| r.student_id == id).collect()
    }

    fn self_pay(&self) -> SelfPayReport {
        self.db
            .read(|c| repo::settle::self_pay_report(c, self.ws, &self.items))
            .unwrap()
    }

    fn program(&self, p: Program) -> Vec<ProgramRow> {
        self.db
            .read(|c| repo::settle::program_rows(c, self.ws, p, &self.items))
            .unwrap()
    }

    fn export(&self, kind: DetailKind, tag: &str) -> PathBuf {
        let dir = tmp_dir(tag);
        let r = self
            .db
            .read(|c| excel::settle_detail::export(c, self.ws, kind, &dir))
            .unwrap();
        PathBuf::from(r.path)
    }

    /// 사례 A~G 가 모두 든 작업공간.
    fn mixed() -> Self {
        let mut f = Self::empty();
        f.dept("부서1", "로봇과학", "A", vec![fee(강사료, 35_000), fee(수용비, 1_500)]);
        f.dept(
            "부서2",
            "만화애니메이션",
            "B",
            vec![fee(강사료, 35_000), fee(수용비, 1_500), fee(재료비, 22_000)],
        );
        f.dept("부서3", "창의미술", "B", vec![fee(강사료, 28_000), fee(수용비, 1_200)]);
        f.dept(
            "부서4",
            "토탈공예미니어처",
            "B",
            vec![fee(강사료, 35_000), fee(수용비, 1_500), fee(재료비, 28_750)],
        );
        f.dept("부서5", "화요키즈쿠킹", "B", vec![fee(강사료, 30_000), fee(재료비, 10_000)]);
        f.dept("부서6", "바둑", "A", vec![fee(강사료, 20_000)]);
        // 사례 G — 이름은 다르게 저장됐지만 화면 이름은 똑같이 '방송댄스B'
        f.dept("부서7", "방송댄스B", "", vec![fee(강사료, 25_000)]);
        f.dept("부서8", "방송댄스", "B", vec![fee(강사료, 26_000)]);

        f.student("A", 3, "1", 1, "가온");
        f.student("B", 3, "1", 2, "나래");
        f.student("C", 3, "1", 3, "다솜");
        f.student("D", 3, "1", 4, "라온");
        f.student("E", 3, "가", 1, "마루");
        f.student("F1", 4, "10", 1, "바다");
        f.student("F2", 4, "2", 1, "바다");
        f.student("G", 5, "1", 1, "사랑");
        f.student("I", 6, "1", 1, "아라");

        f.enroll("A", "부서1"); //  수익자만
        f.enroll("B", "부서2"); //  이용권 지원만
        f.enroll("C", "부서3"); //  이용권 지원 + 초과
        f.enroll("D", "부서4"); //  이용권 + 자유수강권
        for d in ["부서1", "부서5", "부서6"] {
            f.enroll("E", d); //   한 학생 세 부서
        }
        f.enroll("F1", "부서1");
        f.enroll("F2", "부서1");
        f.enroll("G", "부서7");
        f.enroll("G", "부서8");
        f.enroll("I", "부서1"); //  자유수강권만 — 소진 뒤 남은 돈은 수익자로

        f.support("B", Program::Voucher, 58_500);
        f.support("C", Program::Voucher, 3_400);
        f.support("D", Program::Voucher, 41_150);
        f.support("D", Program::FreeVoucher, 24_100);
        f.support("I", Program::FreeVoucher, 10_000);
        f.generate();
        f
    }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("afterschool-detail-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn amt(r: &DetailRow, group: usize, code: &str) -> i64 {
    r.groups[group]
        .fees
        .iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap()
}

// ─────────────────────────────────────────────── 사례

#[test]
fn 사례a_수익자만은_수익자_상세에만_나온다() {
    let f = F::mixed();
    let r = f.of(DetailKind::SelfPay, "A");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].dept_label, "로봇과학A");
    assert_eq!(amt(&r[0], 0, 강사료), 35_000);
    assert_eq!(amt(&r[0], 0, 수용비), 1_500);
    assert_eq!(r[0].total, 36_500);
    assert!(f.of(DetailKind::Voucher, "A").is_empty());
    assert!(f.of(DetailKind::FreeVoucher, "A").is_empty());
}

#[test]
fn 사례b_이용권_지원만이면_지원금_58500_초과금_0() {
    let f = F::mixed();
    let r = f.of(DetailKind::Voucher, "B");
    assert_eq!(r.len(), 1);
    assert_eq!(amt(&r[0], 0, 강사료), 35_000);
    assert_eq!(amt(&r[0], 0, 수용비), 1_500);
    assert_eq!(amt(&r[0], 0, "TEXTBOOK"), 0);
    assert_eq!(amt(&r[0], 0, 재료비), 22_000);
    assert_eq!(r[0].groups[0].total, 58_500);
    assert_eq!(r[0].groups[1].total, 0);
    assert_eq!(r[0].total, 58_500);
    assert!(f.of(DetailKind::SelfPay, "B").is_empty(), "정상 지원액은 수익자가 아니다");
}

#[test]
fn 사례c_지원과_초과가_나뉘고_총합계는_29200() {
    let f = F::mixed();
    let r = f.of(DetailKind::Voucher, "C");
    assert_eq!(r.len(), 1);
    assert_eq!(amt(&r[0], 0, 강사료), 3_400);
    assert_eq!(r[0].groups[0].total, 3_400);
    assert_eq!(amt(&r[0], 1, 강사료), 24_600);
    assert_eq!(amt(&r[0], 1, 수용비), 1_200);
    assert_eq!(r[0].groups[1].total, 25_800);
    assert_eq!(r[0].total, 29_200);

    // 초과금은 수익자 화면에도 들어 있는 같은 돈이다
    let s = f.of(DetailKind::SelfPay, "C");
    assert_eq!(s[0].total, 25_800);
}

#[test]
fn 사례d_자유수강권이_섞여도_이용권_상세에는_이용권만_들어간다() {
    let f = F::mixed();
    let v = f.of(DetailKind::Voucher, "D");
    assert_eq!(v.len(), 1);
    assert_eq!(amt(&v[0], 0, 강사료), 35_000);
    assert_eq!(amt(&v[0], 0, 수용비), 1_500);
    assert_eq!(amt(&v[0], 0, 재료비), 4_650, "자유수강권 24,100원이 섞였다");
    assert_eq!(v[0].groups[0].total, 41_150);
    assert_eq!(v[0].groups[1].total, 0);

    let fr = f.of(DetailKind::FreeVoucher, "D");
    assert_eq!(fr.len(), 1);
    assert_eq!(amt(&fr[0], 0, 강사료), 0);
    assert_eq!(amt(&fr[0], 0, 재료비), 24_100);
    assert_eq!(fr[0].total, 24_100);
    assert!(f.of(DetailKind::SelfPay, "D").is_empty());
}

#[test]
fn 자유수강권만_가진_학생의_남은_돈은_수익자_상세로_간다() {
    let f = F::mixed();
    let fr = f.of(DetailKind::FreeVoucher, "I");
    assert_eq!(fr[0].total, 10_000);
    let s = f.of(DetailKind::SelfPay, "I");
    assert_eq!(s[0].total, 26_500, "자유수강권 초과금이라는 재원은 없다");
    assert!(f.of(DetailKind::Voucher, "I").is_empty());
}

#[test]
fn 사례e_한_학생이_세_부서면_세_줄이다() {
    let f = F::mixed();
    let r = f.of(DetailKind::SelfPay, "E");
    let labels: Vec<&str> = r.iter().map(|x| x.dept_label.as_str()).collect();
    assert_eq!(labels, vec!["로봇과학A", "바둑A", "화요키즈쿠킹B"], "부서명 차례");
    assert_eq!(r.iter().map(|x| x.total).sum::<i64>(), 36_500 + 40_000 + 20_000);
}

#[test]
fn 사례f_동명이인은_합치지_않는다() {
    let f = F::mixed();
    let rows: Vec<DetailRow> = f
        .rows(DetailKind::SelfPay)
        .into_iter()
        .filter(|r| r.name == "바다")
        .collect();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].student_id, rows[1].student_id);
    assert!(rows.iter().all(|r| r.total == 36_500), "두 학생 금액이 한 줄로 합쳐졌다");
    // 반 '2' 가 '10' 보다 앞 — 숫자 반 자연 정렬
    assert_eq!(rows[0].class_no, "2");
    assert_eq!(rows[1].class_no, "10");
}

#[test]
fn 사례g_화면_이름이_같은_부서도_id_로_나눈다() {
    let f = F::mixed();
    let r = f.of(DetailKind::SelfPay, "G");
    assert_eq!(r.len(), 2, "이름이 같다고 한 줄로 합쳐졌다");
    assert!(r.iter().all(|x| x.dept_label == "방송댄스B"));
    assert_ne!(r[0].department_id, r[1].department_id);
    let mut totals: Vec<i64> = r.iter().map(|x| x.total).collect();
    totals.sort_unstable();
    assert_eq!(totals, vec![25_000, 26_000]);
}

#[test]
fn 사례h_같은_칸의_여러_배분은_모두_더한다() {
    let f = F::mixed();
    let (a, d1) = (f.st["A"], f.dp["부서1"]);
    // 같은 학생 · 부서 · 항목 · 재원에 배분이 세 줄. 사연(origin)도 둘로 갈린다.
    f.db.write(|c| {
        let (sid, eid): (i64, i64) = c.query_row(
            "SELECT a.settlement_id, a.enrollment_id FROM settlement_alloc a
               JOIN settlement s ON s.id = a.settlement_id AND s.is_latest = 1
              WHERE a.student_id = ?1 AND a.department_id = ?2 LIMIT 1",
            rusqlite::params![a, d1],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        for (origin, amount) in [("PLAIN", 700), ("VOUCHER_EXHAUSTED", 300)] {
            c.execute(
                "INSERT INTO settlement_alloc
                   (settlement_id, student_id, department_id, enrollment_id, item_code, fund, origin, amount)
                 VALUES (?1, ?2, ?3, ?4, 'INSTRUCTOR', 'SELF_PAY', ?5, ?6)",
                rusqlite::params![sid, a, d1, eid, origin, amount],
            )?;
        }
        Ok(())
    })
    .unwrap();

    let r = f.of(DetailKind::SelfPay, "A");
    assert_eq!(r.len(), 1, "같은 부서가 두 줄로 갈렸다");
    assert_eq!(amt(&r[0], 0, 강사료), 36_000, "첫 줄이나 마지막 줄만 가져왔다");
    assert_eq!(r[0].total, 37_500);
    // 화면 목록도 같은 값을 말한다
    let screen = f.self_pay().rows.into_iter().find(|x| x.student_id == a).unwrap();
    assert_eq!(screen.total, 37_500);
}

// ─────────────────────────────────────────────── 검산

/// 학생별 합 — (묶음별 합계)
fn per_student(rows: &[DetailRow]) -> HashMap<i64, Vec<i64>> {
    let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
    for r in rows {
        let e = out.entry(r.student_id).or_insert_with(|| vec![0; r.groups.len()]);
        for (i, g) in r.groups.iter().enumerate() {
            e[i] += g.total;
        }
    }
    out
}

#[test]
fn 수익자_학생별_전체_합계가_수익자_화면과_같다() {
    let f = F::mixed();
    let rows = f.rows(DetailKind::SelfPay);
    let screen = f.self_pay();
    let mine = per_student(&rows);
    assert_eq!(mine.len(), screen.rows.len(), "학생 수가 다르다");
    for s in &screen.rows {
        assert_eq!(mine[&s.student_id][0], s.total, "{} 학생 금액이 다르다", s.name);
    }
    assert_eq!(rows.iter().map(|r| r.total).sum::<i64>(), screen.total);
    // 줄 수도 화면의 부서별 상세와 같다
    assert_eq!(rows.len(), screen.details.len());
}

#[test]
fn 이용권_학생별_지원금_초과금이_이용권_화면과_같다() {
    let f = F::mixed();
    let rows = f.rows(DetailKind::Voucher);
    let mine = per_student(&rows);
    let screen = f.program(Program::Voucher);
    let mut used = 0;
    let mut over = 0;
    for s in &screen {
        let m = mine.get(&s.student_id).cloned().unwrap_or(vec![0, 0]);
        assert_eq!(m[0], s.used_total, "{} 지원금", s.name);
        assert_eq!(m[1], s.over_total, "{} 초과금", s.name);
        used += s.used_total;
        over += s.over_total;
    }
    assert_eq!(rows.iter().map(|r| r.groups[0].total).sum::<i64>(), used);
    assert_eq!(rows.iter().map(|r| r.groups[1].total).sum::<i64>(), over);
    assert_eq!(rows.iter().map(|r| r.total).sum::<i64>(), used + over);
    assert_eq!((used, over), (58_500 + 3_400 + 41_150, 25_800));
}

#[test]
fn 자유수강권_학생별_전체_합계가_자유수강권_화면과_같다() {
    let f = F::mixed();
    let rows = f.rows(DetailKind::FreeVoucher);
    let mine = per_student(&rows);
    let screen = f.program(Program::FreeVoucher);
    let mut total = 0;
    for s in &screen {
        assert_eq!(mine.get(&s.student_id).map(|m| m[0]).unwrap_or(0), s.used_total, "{}", s.name);
        assert_eq!(s.over_total, 0, "자유수강권 화면에는 초과금이 없다");
        total += s.used_total;
    }
    assert_eq!(rows.iter().map(|r| r.total).sum::<i64>(), total);
    assert_eq!(total, 24_100 + 10_000);
}

#[test]
fn 세_상세를_겹치지_않게_더하면_정산_전체와_같다() {
    let f = F::mixed();
    let sum = |k| f.rows(k).iter().map(|r: &DetailRow| r.total).sum::<i64>();
    let self_pay = sum(DetailKind::SelfPay); //       SELF_PAY + VOUCHER_OVER
    let voucher_used: i64 = f.rows(DetailKind::Voucher).iter().map(|r| r.groups[0].total).sum();
    let voucher_over: i64 = f.rows(DetailKind::Voucher).iter().map(|r| r.groups[1].total).sum();
    let free = sum(DetailKind::FreeVoucher);

    let s = f
        .db
        .read(|c| repo::settle::summary(c, f.ws, &f.items))
        .unwrap()
        .unwrap();
    // 이용권 초과금은 수익자 안에 이미 들어 있다 — 따로 더하면 두 번 센다
    assert_eq!(voucher_over, s.total.voucher_over);
    assert_eq!(self_pay, s.total.self_pay + s.total.voucher_over);
    assert_eq!(self_pay + voucher_used + free, s.total.total);
    assert_eq!(s.total.total, s.charge_total, "정산이 원본 금액과 맞지 않는다");
}

#[test]
fn 정산_상세_팝업과_같은_금액이다() {
    let f = F::mixed();
    let kinds = [DetailKind::SelfPay, DetailKind::Voucher, DetailKind::FreeVoucher];
    for (key, id) in &f.st {
        let popup = f
            .db
            .read(|c| repo::settle::student_allocs(c, f.ws, *id))
            .unwrap();
        for kind in kinds {
            let spec = kind.spec();
            for (gi, g) in spec.groups.iter().enumerate() {
                // 팝업에서 (부서, 항목) 마다 이 묶음의 재원을 더한 값
                let mut want: HashMap<(i64, String), i64> = HashMap::new();
                for a in popup.iter().filter(|a| g.funds.contains(&a.fund.as_str())) {
                    *want.entry((a.department_id, a.item_code.clone())).or_insert(0) += a.amount;
                }
                let mut got: HashMap<(i64, String), i64> = HashMap::new();
                for r in f.rows(kind).iter().filter(|r| r.student_id == *id) {
                    for fe in r.groups[gi].fees.iter().filter(|x| x.amount > 0) {
                        got.insert((r.department_id, fe.item_code.clone()), fe.amount);
                    }
                }
                assert_eq!(got, want, "{key} / {kind:?} / {}", g.title);
            }
        }
    }
}

#[test]
fn 학년_반_번호_이름_부서_차례로_놓인다() {
    let f = F::mixed();
    let rows = f.rows(DetailKind::SelfPay);
    let keys: Vec<(i64, String, i64)> = rows
        .iter()
        .map(|r| (r.grade, r.class_no.clone(), r.student_no))
        .collect();
    // 3학년 1반(숫자) → 3학년 가반(문자) → 4학년 2반 → 4학년 10반 → 5학년 → 6학년
    let classes: Vec<String> = keys.iter().map(|k| format!("{}-{}", k.0, k.1)).collect();
    let mut dedup = classes.clone();
    dedup.dedup();
    assert_eq!(dedup, vec!["3-1", "3-가", "4-2", "4-10", "5-1", "6-1"]);
}

// ─────────────────────────────────────────────── 정산 상태 · 읽기 전용

#[test]
fn 낡은_정산이나_정산_전에는_파일을_만들지_않는다() {
    let mut f = F::empty();
    f.dept("부서1", "로봇과학", "A", vec![fee(강사료, 35_000)]);
    f.student("A", 3, "1", 1, "가온");
    f.enroll("A", "부서1");
    let dir = tmp_dir("none");
    for kind in [DetailKind::SelfPay, DetailKind::Voucher, DetailKind::FreeVoucher] {
        let e = f
            .db
            .read(|c| excel::settle_detail::export(c, f.ws, kind, &dir))
            .unwrap_err();
        assert!(e.to_string().contains("정산 전입니다"), "{e}");
    }

    f.generate();
    f.enroll_more();
    for kind in [DetailKind::SelfPay, DetailKind::Voucher, DetailKind::FreeVoucher] {
        let e = f
            .db
            .read(|c| excel::settle_detail::export(c, f.ws, kind, &dir))
            .unwrap_err();
        // 기존 Excel 과 같은 문구 — 같은 함수(require_fresh)로 막는다
        let old = f.db.read(|c| repo::settle::require_fresh(c, f.ws)).unwrap_err();
        assert_eq!(e.to_string(), old.to_string());
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "막혔는데 파일이 생겼다");
    // 정산을 대신 만들지 않는다
    let st = f.db.read(|c| repo::settle::status(c, f.ws)).unwrap();
    assert_eq!(st.state, "STALE_DATA");
}

impl F {
    fn enroll_more(&mut self) {
        self.dept("부서9", "바둑", "A", vec![fee(강사료, 20_000)]);
        self.enroll("A", "부서9");
    }
}

/// 업무 자료 전체의 지문 — 내려받기 전후에 견준다.
fn fingerprint(f: &F) -> Vec<String> {
    f.db.read(|c| {
        let mut out = Vec::new();
        for sql in [
            "SELECT group_concat(id || ':' || data_version) FROM workspace",
            "SELECT group_concat(id || ':' || data_version) FROM academic_year",
            "SELECT COUNT(*) || '/' || group_concat(id || ':' || is_latest) FROM settlement",
            "SELECT COUNT(*) || '/' || SUM(amount) || '/' || SUM(id * amount) FROM settlement_alloc",
            "SELECT COUNT(*) || '/' || SUM(available) FROM settlement_budget",
            "SELECT COUNT(*) || '/' || SUM(amount) || '/' || SUM(id * amount) FROM charge",
            "SELECT COUNT(*) || '/' || COALESCE(MAX(id), 0) FROM change_log",
        ] {
            out.push(c.query_row(sql, [], |r| r.get::<_, String>(0))?);
        }
        out.push(repo::settle::status(c, f.ws)?.state);
        Ok(out)
    })
    .unwrap()
}

#[test]
fn 내려받아도_업무_자료와_정산_상태가_바뀌지_않는다() {
    let f = F::mixed();
    let before = fingerprint(&f);
    assert_eq!(before.last().unwrap(), "FRESH");
    for (i, kind) in [DetailKind::SelfPay, DetailKind::Voucher, DetailKind::FreeVoucher]
        .into_iter()
        .enumerate()
    {
        f.export(kind, &format!("ro{i}"));
    }
    assert_eq!(fingerprint(&f), before);
}

// ─────────────────────────────────────────────── 만든 파일을 다시 읽는다

struct Book {
    name: String,
    cells: Vec<Vec<Data>>,
}

fn read_book(path: &Path) -> Book {
    let mut wb = open_workbook_auto(path).unwrap();
    let names = wb.sheet_names().to_vec();
    assert_eq!(names.len(), 1, "상세내역은 한 장이다");
    let range = wb.worksheet_range(&names[0]).unwrap();
    Book {
        name: names[0].clone(),
        cells: range.rows().map(|r| r.to_vec()).collect(),
    }
}

fn s(d: &Data) -> String {
    excel::read::text(d)
}

fn n(d: &Data) -> i64 {
    match d {
        Data::Float(v) => *v as i64,
        Data::Int(v) => *v,
        other => panic!("금액이 숫자 셀이 아니다: {other:?}"),
    }
}

#[test]
fn 수익자_파일을_다시_읽으면_화면과_같다() {
    let f = F::mixed();
    let path = f.export(DetailKind::SelfPay, "xl-self");
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("수익자_상세내역_2026학년도_10월_"), "{name}");
    let b = read_book(&path);
    assert_eq!(b.name, "수익자 상세내역");

    let head: Vec<String> = b.cells[0].iter().map(s).collect();
    assert_eq!(
        head,
        vec!["학년", "반", "번호", "이름", "부서", "강사료", "수용비", "교재비", "재료비", "합계"]
    );
    let rows = f.rows(DetailKind::SelfPay);
    assert_eq!(b.cells.len(), 1 + rows.len(), "합계 행 없이 자료 줄만");
    for (i, r) in rows.iter().enumerate() {
        let c = &b.cells[1 + i];
        assert_eq!(c.len(), 10);
        assert_eq!(
            [s(&c[0]), s(&c[1]), s(&c[2]), s(&c[3]), s(&c[4])],
            [r.grade.to_string(), r.class_no.clone(), r.student_no.to_string(), r.name.clone(), r.dept_label.clone()]
        );
        let fees: Vec<i64> = c[5..9].iter().map(n).collect();
        assert_eq!(fees, r.groups[0].fees.iter().map(|x| x.amount).collect::<Vec<_>>());
        assert_eq!(n(&c[9]), r.total);
        assert_eq!(fees.iter().sum::<i64>(), n(&c[9]), "합계가 항목 합과 다르다");
    }
    let file_total: i64 = b.cells[1..].iter().map(|c| n(&c[9])).sum();
    assert_eq!(file_total, f.self_pay().total);
}

#[test]
fn 이용권_파일은_2단_머리글이고_다시_읽으면_화면과_같다() {
    let f = F::mixed();
    let path = f.export(DetailKind::Voucher, "xl-voucher");
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("방과후이용권_상세내역_2026학년도_10월_"), "{name}");
    let b = read_book(&path);
    assert_eq!(b.name, "이용권 상세내역");

    let h1: Vec<String> = b.cells[0].iter().map(s).collect();
    let h2: Vec<String> = b.cells[1].iter().map(s).collect();
    assert_eq!(h1.len(), 16);
    assert_eq!(&h1[..6], ["학년", "반", "번호", "이름", "부서", "지원금"]);
    assert_eq!(h1[10], "초과금");
    assert_eq!(h1[15], "총합계");
    assert!(h1[6..10].iter().all(|x| x.is_empty()), "지원금 칸이 병합되지 않았다");
    assert_eq!(
        &h2[5..15],
        ["강사료", "수용비", "교재비", "재료비", "합계", "강사료", "수용비", "교재비", "재료비", "합계"]
    );

    let rows = f.rows(DetailKind::Voucher);
    assert_eq!(b.cells.len(), 2 + rows.len());
    let mut used = 0;
    let mut over = 0;
    for (i, r) in rows.iter().enumerate() {
        let c = &b.cells[2 + i];
        assert_eq!(s(&c[3]), r.name);
        let sup: Vec<i64> = c[5..9].iter().map(n).collect();
        let ovr: Vec<i64> = c[10..14].iter().map(n).collect();
        assert_eq!(sup.iter().sum::<i64>(), n(&c[9]));
        assert_eq!(ovr.iter().sum::<i64>(), n(&c[14]));
        assert_eq!(n(&c[9]) + n(&c[14]), n(&c[15]));
        used += n(&c[9]);
        over += n(&c[14]);
    }
    let screen = f.program(Program::Voucher);
    assert_eq!(used, screen.iter().map(|x| x.used_total).sum::<i64>());
    assert_eq!(over, screen.iter().map(|x| x.over_total).sum::<i64>());

    // 사례 D 학생 줄: 재료비 지원금은 4,650 뿐이다
    let d = b.cells[2..].iter().find(|c| s(&c[3]) == "라온").unwrap();
    assert_eq!(n(&d[8]), 4_650);
    assert_eq!(n(&d[9]), 41_150);
}

#[test]
fn 자유수강권_파일을_다시_읽으면_화면과_같다() {
    let f = F::mixed();
    let path = f.export(DetailKind::FreeVoucher, "xl-free");
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("자유수강권_상세내역_2026학년도_10월_"), "{name}");
    let b = read_book(&path);
    assert_eq!(b.name, "자유수강권 상세내역");
    let head: Vec<String> = b.cells[0].iter().map(s).collect();
    assert_eq!(
        head,
        vec!["학년", "반", "번호", "이름", "부서", "강사료", "수용비", "교재비", "재료비", "합계"]
    );
    assert_eq!(b.cells.len(), 1 + 2);
    let names: Vec<String> = b.cells[1..].iter().map(|c| s(&c[3])).collect();
    assert_eq!(names, vec!["라온", "아라"]);
    assert_eq!(n(&b.cells[1][8]), 24_100);
    let total: i64 = b.cells[1..].iter().map(|c| n(&c[9])).sum();
    let screen: i64 = f.program(Program::FreeVoucher).iter().map(|x| x.used_total).sum();
    assert_eq!(total, screen);
}

#[test]
fn 대상이_없으면_머리글만_있는_파일이_된다() {
    let mut f = F::empty();
    f.dept("부서1", "로봇과학", "A", vec![fee(강사료, 35_000)]);
    f.student("A", 3, "1", 1, "가온");
    f.enroll("A", "부서1");
    f.generate();
    let b = read_book(&f.export(DetailKind::Voucher, "xl-empty"));
    assert_eq!(b.cells.len(), 2, "머리글 두 줄만");
}

#[test]
fn 기존_세_가지_excel_은_그대로다() {
    let f = F::mixed();
    let dir = tmp_dir("old");
    let ws = repo::year::get_workspace;
    f.db.read(|c| {
        let w = ws(c, f.ws)?;
        let y = repo::year::get_year(c, w.year_id)?;
        let scope = [y.name.as_str(), w.name.as_str()];
        let report = repo::settle::self_pay_report(c, f.ws, &f.items)?;
        excel::admin::write_self_pay(&report, &f.items, &scope, &dir)?;
        let rows = repo::settle::program_rows(c, f.ws, Program::Voucher, &f.items)?;
        excel::admin::write_program(&rows, &f.items, "방과후이용권", true, &scope, &dir)?;
        Ok(())
    })
    .unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert!(names[0].starts_with("방과후이용권_2026학년도_10월_"));
    assert!(names[1].starts_with("수익자_2026학년도_10월_"));

    let mut wb = open_workbook_auto(dir.join(&names[1])).unwrap();
    assert_eq!(wb.sheet_names().to_vec(), vec!["학생별 합계", "부서별 상세"]);
    let r = wb.worksheet_range("부서별 상세").unwrap();
    let head: Vec<String> = r.rows().next().unwrap().iter().map(s).collect();
    assert_eq!(head.last().unwrap(), "발생원인");
}
