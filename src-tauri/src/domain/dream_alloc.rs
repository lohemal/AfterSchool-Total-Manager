//! 방과후드림 지원금 재배분 (P4) — 순수 계산. DB · 화면을 모른다.
//!
//! 지원금 **총액은 새로 계산하지 않는다.** 학생마다 최신 유효 정산이 이미 확정한
//! `T`(정산 총액) · `V`(이용권) · `F`(자유수강권) · `personal`(= T − V − F)을 받아,
//! 그 V · F 를 방과후드림 결제행에 **어느 줄부터 채울지**만 정한다. 기존 정산 배분
//! (`settlement_alloc`)의 부서 · 항목별 금액은 보지 않는다 — 학생 총액만 같으면 된다.
//!
//! ```text
//! 줄 차례  방과후드림 부서 순위 → 공통 항목 순위(모든 부서 같음) → 부서 id → 항목 코드 → 행 번호
//! 줄마다   P = min(N, 남은 V)            V 먼저
//!          Q = min(N − P, 남은 F)        그다음 F
//!          O = N − P − Q                 나머지는 개인부담
//! ```
//!
//! V 와 F 를 비율로 나누지 않는다. N = 0 인 줄은 남은 지원금을 쓰지 않고 O = P = Q = 0.
//! 계산이 끝나면 줄마다 `N = O + P + Q`, 학생 전체 `ΣP = V · ΣQ = F · ΣO = personal ·
//! ΣN = T`, 남은 V · F 가 0 인지 다시 확인한다. 하나라도 어긋나면 결과를 내지 않는다.

use std::collections::HashSet;

/// 학생의 결제행 하나(검증을 마친 것).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub source_row: u32,
    pub department_id: i64,
    /// 비용항목 코드(`INSTRUCTOR` 등)
    pub item_code: String,
    pub amount_n: i64,
}

/// 학생의 확정 기준액(최신 유효 정산).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Target {
    pub total: i64,
    pub voucher: i64,
    pub free_voucher: i64,
    pub personal: i64,
}

/// 배분 차례. `depts` 는 부서 id 를 앞선 차례대로, `items` 는 항목 코드를 차례대로 — 항목
/// 차례는 **모든 부서에 같다**(P6. 실제 업무에서 부서마다 다르게 두지 않는다).
/// 없는 부서 · 항목은 있는 것 뒤에 온다(그 안에서는 부서 id · 항목 코드 · 행 번호 차례).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Priority {
    pub depts: Vec<i64>,
    pub items: Vec<String>,
}

impl Priority {
    fn dept_rank(&self, dept: i64) -> usize {
        self.depts.iter().position(|d| *d == dept).unwrap_or(usize::MAX)
    }

    fn item_rank(&self, item: &str) -> usize {
        self.items.iter().position(|i| i == item).unwrap_or(usize::MAX)
    }

    /// 차례를 글로 — 차례가 같으면 언제나 같은 글이다. 결과를 무효로 할지 가리는 지문의 재료.
    pub fn canonical(&self) -> String {
        format!(
            "D:{};I:{}",
            self.depts.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(","),
            self.items.join(",")
        )
    }
}

/// 한 줄의 배분 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split {
    pub source_row: u32,
    /// 배분 차례(1부터) — 화면에서 '왜 이 줄이 먼저인가'를 보여 줄 때 쓴다
    pub seq: u32,
    pub o: i64,
    pub p: i64,
    pub q: i64,
}

/// 합계.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub n: i64,
    pub o: i64,
    pub p: i64,
    pub q: i64,
}

impl Totals {
    pub fn add(&mut self, n: i64, o: i64, p: i64, q: i64) {
        self.n += n;
        self.o += o;
        self.p += p;
        self.q += q;
    }
}

/// 실패 까닭.
pub mod code {
    /// N · T · V · F 가운데 음수가 있다
    pub const NEGATIVE_AMOUNT: &str = "ALLOC_NEGATIVE_AMOUNT";
    /// T ≠ V + F + personal
    pub const TARGET_INCONSISTENT: &str = "ALLOC_TARGET_INCONSISTENT";
    /// ΣN ≠ T
    pub const TOTAL_MISMATCH: &str = "ALLOC_TOTAL_MISMATCH";
    /// 같은 행 번호가 두 번
    pub const DUPLICATE_ROW: &str = "ALLOC_DUPLICATE_ROW";
    /// 줄 capacity 안에 V 를 다 담지 못했다
    pub const VOUCHER_LEFT: &str = "ALLOC_VOUCHER_LEFT";
    /// 줄 capacity 안에 F 를 다 담지 못했다
    pub const FREE_LEFT: &str = "ALLOC_FREE_LEFT";
    /// 결과 줄이 입력 줄과 다르다
    pub const ROW_SET_MISMATCH: &str = "ALLOC_ROW_SET_MISMATCH";
    /// 어떤 줄에서 N ≠ O + P + Q 이거나 음수
    pub const ROW_SUM_MISMATCH: &str = "ALLOC_ROW_SUM_MISMATCH";
    /// ΣP ≠ V
    pub const VOUCHER_SUM_MISMATCH: &str = "ALLOC_VOUCHER_SUM_MISMATCH";
    /// ΣQ ≠ F
    pub const FREE_SUM_MISMATCH: &str = "ALLOC_FREE_SUM_MISMATCH";
    /// ΣO ≠ personal
    pub const PERSONAL_SUM_MISMATCH: &str = "ALLOC_PERSONAL_SUM_MISMATCH";
    /// 파일 전체 합계가 정산 전체와 다르다
    pub const FILE_SUM_MISMATCH: &str = "ALLOC_FILE_SUM_MISMATCH";
}

/// 줄을 배분 차례로 — 입력 차례와 무관하게 같은 결과(`lines` 의 위치를 돌려준다).
pub fn order(lines: &[Line], pr: &Priority) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..lines.len()).collect();
    idx.sort_by(|a, b| {
        let key = |l: &Line| {
            (
                pr.dept_rank(l.department_id),
                pr.item_rank(&l.item_code),
                l.department_id,
                l.item_code.clone(),
                l.source_row,
            )
        };
        key(&lines[*a]).cmp(&key(&lines[*b]))
    });
    idx
}

/// 학생 한 명의 배분. 결과는 행 번호 차례다. 검산까지 통과해야 `Ok`.
pub fn allocate(lines: &[Line], target: &Target, pr: &Priority) -> Result<Vec<Split>, &'static str> {
    if lines.iter().any(|l| l.amount_n < 0)
        || target.total < 0
        || target.voucher < 0
        || target.free_voucher < 0
        || target.personal < 0
    {
        return Err(code::NEGATIVE_AMOUNT);
    }
    if target.total != target.voucher + target.free_voucher + target.personal {
        return Err(code::TARGET_INCONSISTENT);
    }
    let mut rows = HashSet::new();
    if !lines.iter().all(|l| rows.insert(l.source_row)) {
        return Err(code::DUPLICATE_ROW);
    }
    if lines.iter().map(|l| l.amount_n).sum::<i64>() != target.total {
        return Err(code::TOTAL_MISMATCH);
    }

    let mut voucher_left = target.voucher;
    let mut free_left = target.free_voucher;
    let mut out = Vec::with_capacity(lines.len());
    for (seq, i) in order(lines, pr).into_iter().enumerate() {
        let l = &lines[i];
        let p = l.amount_n.min(voucher_left);
        voucher_left -= p;
        let q = (l.amount_n - p).min(free_left);
        free_left -= q;
        out.push(Split {
            source_row: l.source_row,
            seq: seq as u32 + 1,
            o: l.amount_n - p - q,
            p,
            q,
        });
    }
    if voucher_left != 0 {
        return Err(code::VOUCHER_LEFT);
    }
    if free_left != 0 {
        return Err(code::FREE_LEFT);
    }
    out.sort_by_key(|s| s.source_row);
    verify(lines, target, &out)?;
    Ok(out)
}

/// 배분 결과의 검산 — 줄마다 · 학생 전체.
pub fn verify(lines: &[Line], target: &Target, splits: &[Split]) -> Result<(), &'static str> {
    if lines.len() != splits.len() {
        return Err(code::ROW_SET_MISMATCH);
    }
    let mut t = Totals::default();
    for l in lines {
        let Some(s) = splits.iter().find(|s| s.source_row == l.source_row) else {
            return Err(code::ROW_SET_MISMATCH);
        };
        if s.o < 0 || s.p < 0 || s.q < 0 || s.o + s.p + s.q != l.amount_n {
            return Err(code::ROW_SUM_MISMATCH);
        }
        t.add(l.amount_n, s.o, s.p, s.q);
    }
    if t.n != target.total {
        return Err(code::TOTAL_MISMATCH);
    }
    if t.p != target.voucher {
        return Err(code::VOUCHER_SUM_MISMATCH);
    }
    if t.q != target.free_voucher {
        return Err(code::FREE_SUM_MISMATCH);
    }
    if t.o != target.personal {
        return Err(code::PERSONAL_SUM_MISMATCH);
    }
    Ok(())
}

/// 파일 전체 검산 — 배분 뒤 합계(`after`)가 정산 전체(`settlement`)와 같은가.
pub fn verify_file(after: &Totals, settlement: &Target) -> Result<(), &'static str> {
    let ok = after.n == settlement.total
        && after.p == settlement.voucher
        && after.q == settlement.free_voucher
        && after.o == settlement.personal
        && after.n == after.o + after.p + after.q;
    if ok {
        Ok(())
    } else {
        Err(code::FILE_SUM_MISMATCH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const 강사료: &str = "INSTRUCTOR";
    const 수용비: &str = "OPERATION";
    const 교재비: &str = "TEXTBOOK";
    const 재료비: &str = "MATERIAL";
    const 로봇: i64 = 10;
    const 미술: i64 = 20;
    const 공예: i64 = 30;

    fn l(row: u32, dept: i64, item: &str, n: i64) -> Line {
        Line {
            source_row: row,
            department_id: dept,
            item_code: item.into(),
            amount_n: n,
        }
    }

    fn tg(total: i64, voucher: i64, free: i64) -> Target {
        Target {
            total,
            voucher,
            free_voucher: free,
            personal: total - voucher - free,
        }
    }

    fn pr(depts: &[i64]) -> Priority {
        Priority {
            depts: depts.to_vec(),
            items: vec![강사료.to_string(), 수용비.into(), 교재비.into(), 재료비.into()],
        }
    }

    /// (행, O, P, Q)
    fn opq(s: &[Split]) -> Vec<(u32, i64, i64, i64)> {
        s.iter().map(|x| (x.source_row, x.o, x.p, x.q)).collect()
    }

    #[test]
    fn 일반_수익자는_모두_개인부담이다() {
        let lines = [l(2, 로봇, 강사료, 35_000), l(3, 로봇, 수용비, 1_500)];
        let s = allocate(&lines, &tg(36_500, 0, 0), &pr(&[로봇])).unwrap();
        assert_eq!(opq(&s), vec![(2, 35_000, 0, 0), (3, 1_500, 0, 0)]);
    }

    #[test]
    fn 이용권만_있으면_p_부터_채우고_나머지는_o() {
        let lines = [l(2, 로봇, 강사료, 35_000), l(3, 로봇, 수용비, 1_500), l(4, 로봇, 재료비, 22_000)];
        let s = allocate(&lines, &tg(58_500, 40_000, 0), &pr(&[로봇])).unwrap();
        // 강사료 → 수용비 → 재료비 차례
        assert_eq!(opq(&s), vec![(2, 0, 35_000, 0), (3, 0, 1_500, 0), (4, 18_500, 3_500, 0)]);
    }

    #[test]
    fn 자유수강권만_있으면_q_부터_채운다() {
        let lines = [l(2, 로봇, 강사료, 35_000), l(3, 로봇, 수용비, 1_500)];
        let s = allocate(&lines, &tg(36_500, 0, 10_000), &pr(&[로봇])).unwrap();
        assert_eq!(opq(&s), vec![(2, 25_000, 0, 10_000), (3, 1_500, 0, 0)]);
    }

    #[test]
    fn 이용권과_자유수강권은_같은_차례에서_v_먼저_f_다음이다() {
        // 요청서 예시: 로봇 강사료 30,000 · 로봇 수용비 10,000 · 미술 강사료 40,000, V 50,000 · F 20,000
        let lines = [l(2, 로봇, 강사료, 30_000), l(3, 로봇, 수용비, 10_000), l(4, 미술, 강사료, 40_000)];
        let s = allocate(&lines, &tg(80_000, 50_000, 20_000), &pr(&[로봇, 미술])).unwrap();
        assert_eq!(opq(&s), vec![(2, 0, 30_000, 0), (3, 0, 10_000, 0), (4, 10_000, 10_000, 20_000)]);
    }

    #[test]
    fn 한_줄에서도_비율로_나누지_않고_v_를_먼저_채운다() {
        let lines = [l(2, 로봇, 강사료, 50_000)];
        let s = allocate(&lines, &tg(50_000, 30_000, 20_000), &pr(&[로봇])).unwrap();
        assert_eq!(opq(&s), vec![(2, 0, 30_000, 20_000)]);
    }

    #[test]
    fn 지원금이_전액이면_개인부담은_0() {
        let lines = [l(2, 로봇, 강사료, 35_000), l(3, 미술, 강사료, 28_000), l(4, 미술, 수용비, 1_200)];
        let s = allocate(&lines, &tg(64_200, 41_150, 23_050), &pr(&[미술, 로봇])).unwrap();
        assert!(s.iter().all(|x| x.o == 0), "{:?}", opq(&s));
        assert_eq!(s.iter().map(|x| x.p).sum::<i64>(), 41_150);
        assert_eq!(s.iter().map(|x| x.q).sum::<i64>(), 23_050);
    }

    #[test]
    fn 영원_줄은_지원금을_쓰지_않고_그대로_남는다() {
        let lines = [l(2, 로봇, 강사료, 0), l(3, 로봇, 수용비, 1_500), l(4, 로봇, 교재비, 0)];
        let s = allocate(&lines, &tg(1_500, 1_000, 500), &pr(&[로봇])).unwrap();
        assert_eq!(opq(&s), vec![(2, 0, 0, 0), (3, 0, 1_000, 500), (4, 0, 0, 0)]);
        assert_eq!(s.len(), 3, "0원 줄도 지우지 않는다");
    }

    #[test]
    fn 여러_부서_여러_항목은_부서_차례_안에서_항목_차례다() {
        let lines = [
            l(2, 로봇, 강사료, 35_000),
            l(3, 로봇, 수용비, 1_500),
            l(4, 공예, 강사료, 35_000),
            l(5, 공예, 재료비, 28_750),
        ];
        let mut p = pr(&[공예, 로봇]);
        p.items = vec![재료비.into(), 강사료.into(), 수용비.into(), 교재비.into()];
        let s = allocate(&lines, &tg(100_250, 40_000, 0), &p).unwrap();
        // 공예 재료비 28,750 → 공예 강사료 11,250 → 로봇은 개인부담
        assert_eq!(
            opq(&s),
            vec![(2, 35_000, 0, 0), (3, 1_500, 0, 0), (4, 23_750, 11_250, 0), (5, 0, 28_750, 0)]
        );
        let seq: Vec<u32> = s.iter().map(|x| x.seq).collect();
        assert_eq!(seq, vec![3, 4, 2, 1]);
    }

    #[test]
    fn 부서_차례를_바꾸면_결과가_바뀌고_총액은_그대로다() {
        let lines = [l(2, 로봇, 강사료, 35_000), l(3, 미술, 강사료, 30_000)];
        let t = tg(65_000, 50_000, 0);
        let a = allocate(&lines, &t, &pr(&[로봇, 미술])).unwrap();
        let b = allocate(&lines, &t, &pr(&[미술, 로봇])).unwrap();
        assert_eq!(opq(&a), vec![(2, 0, 35_000, 0), (3, 15_000, 15_000, 0)]);
        assert_eq!(opq(&b), vec![(2, 15_000, 20_000, 0), (3, 0, 30_000, 0)]);
        for s in [&a, &b] {
            assert_eq!(s.iter().map(|x| x.p).sum::<i64>(), 50_000);
        }
    }

    #[test]
    fn 공통_항목_차례는_모든_부서에_같게_적용된다() {
        // 부서 A(로봇) · B(미술), 공통 재료비 → 강사료 → 수용비 → 교재비
        let mut lines = Vec::new();
        let mut row = 2;
        for d in [미술, 로봇] {
            for it in [강사료, 수용비, 교재비, 재료비] {
                lines.push(l(row, d, it, 10_000));
                row += 1;
            }
        }
        let mut p = pr(&[로봇, 미술]);
        p.items = vec![재료비.into(), 강사료.into(), 수용비.into(), 교재비.into()];
        let s = allocate(&lines, &tg(80_000, 80_000, 0), &p).unwrap();
        let by_seq = |s: &[Split]| {
            let mut v: Vec<(u32, i64, String)> = s
                .iter()
                .map(|x| {
                    let ln = lines.iter().find(|y| y.source_row == x.source_row).unwrap();
                    (x.seq, ln.department_id, ln.item_code.clone())
                })
                .collect();
            v.sort();
            v.into_iter().map(|(_, d, i)| (d, i)).collect::<Vec<_>>()
        };
        let want = |order: [&str; 4]| {
            [로봇, 미술]
                .iter()
                .flat_map(|d| order.iter().map(move |i| (*d, i.to_string())))
                .collect::<Vec<_>>()
        };
        assert_eq!(by_seq(&s), want([재료비, 강사료, 수용비, 교재비]));
        // 공통 차례를 바꾸면 두 부서 모두 바뀐다
        p.items = vec![교재비.into(), 수용비.into(), 강사료.into(), 재료비.into()];
        let s2 = allocate(&lines, &tg(80_000, 80_000, 0), &p).unwrap();
        assert_eq!(by_seq(&s2), want([교재비, 수용비, 강사료, 재료비]));
        // 이용권이 모자라면 앞선 줄부터 — 로봇 재료비 · 강사료까지만
        p.items = vec![재료비.into(), 강사료.into(), 수용비.into(), 교재비.into()];
        let s3 = allocate(&lines, &tg(80_000, 20_000, 0), &p).unwrap();
        let paid: Vec<(i64, String)> = s3
            .iter()
            .filter(|x| x.p > 0)
            .map(|x| {
                let ln = lines.iter().find(|y| y.source_row == x.source_row).unwrap();
                (ln.department_id, ln.item_code.clone())
            })
            .collect();
        let mut paid = paid;
        paid.sort();
        let mut exp = vec![(로봇, 재료비.to_string()), (로봇, 강사료.to_string())];
        exp.sort();
        assert_eq!(paid, exp);
    }

    #[test]
    fn 같은_입력이면_언제나_같은_결과이고_줄_차례와_무관하다() {
        let lines = vec![
            l(5, 미술, 재료비, 7_000),
            l(2, 로봇, 강사료, 35_000),
            l(4, 미술, 강사료, 28_000),
            l(3, 로봇, 수용비, 1_500),
        ];
        let t = tg(71_500, 40_000, 10_000);
        let p = pr(&[미술, 로봇]);
        let first = allocate(&lines, &t, &p).unwrap();
        for _ in 0..20 {
            assert_eq!(allocate(&lines, &t, &p).unwrap(), first);
        }
        let mut rev = lines.clone();
        rev.reverse();
        assert_eq!(allocate(&rev, &t, &p).unwrap(), first);
        rev.swap(0, 2);
        assert_eq!(allocate(&rev, &t, &p).unwrap(), first);
    }

    #[test]
    fn 차례가_없는_부서와_항목도_결정적으로_뒤에_온다() {
        let lines = [l(2, 공예, 강사료, 10_000), l(3, 미술, 강사료, 10_000), l(4, 로봇, 강사료, 10_000)];
        let p = Priority {
            depts: vec![미술],
            items: Vec::new(),
        };
        let s = allocate(&lines, &tg(30_000, 15_000, 0), &p).unwrap();
        // 미술(순위 있음) → 로봇(id 10) → 공예(id 30)
        assert_eq!(opq(&s), vec![(2, 10_000, 0, 0), (3, 0, 10_000, 0), (4, 5_000, 5_000, 0)]);
    }

    #[test]
    fn 기준액과_줄이_맞지_않으면_배분하지_않는다() {
        let lines = [l(2, 로봇, 강사료, 10_000)];
        let p = pr(&[로봇]);
        assert_eq!(allocate(&lines, &tg(9_000, 0, 0), &p), Err(code::TOTAL_MISMATCH));
        let bad = Target {
            total: 10_000,
            voucher: 5_000,
            free_voucher: 0,
            personal: 4_000,
        };
        assert_eq!(allocate(&lines, &bad, &p), Err(code::TARGET_INCONSISTENT));
        assert_eq!(allocate(&[l(2, 로봇, 강사료, -1)], &tg(-1, 0, 0), &p), Err(code::NEGATIVE_AMOUNT));
        assert_eq!(
            allocate(&[l(2, 로봇, 강사료, 1), l(2, 로봇, 수용비, 1)], &tg(2, 0, 0), &p),
            Err(code::DUPLICATE_ROW)
        );
    }

    #[test]
    fn 검산은_줄_합과_학생_합을_모두_본다() {
        let lines = [l(2, 로봇, 강사료, 10_000), l(3, 로봇, 수용비, 5_000)];
        let t = tg(15_000, 8_000, 2_000);
        let good = allocate(&lines, &t, &pr(&[로봇])).unwrap();
        assert_eq!(verify(&lines, &t, &good), Ok(()));
        // 줄 합이 N 과 다르면
        let mut bad = good.clone();
        bad[0].o += 1;
        assert_eq!(verify(&lines, &t, &bad), Err(code::ROW_SUM_MISMATCH));
        // 줄 합은 맞지만 P · Q 가 뒤바뀌면(ΣP ≠ V)
        let mut swap = good.clone();
        swap[0].p -= 1_000;
        swap[0].q += 1_000;
        assert_eq!(verify(&lines, &t, &swap), Err(code::VOUCHER_SUM_MISMATCH));
        // 음수
        let mut neg = good.clone();
        neg[1].o = -1;
        neg[1].p += 1;
        assert_eq!(verify(&lines, &t, &neg), Err(code::ROW_SUM_MISMATCH));
        // 줄이 빠지면
        assert_eq!(verify(&lines, &t, &good[..1]), Err(code::ROW_SET_MISMATCH));
    }

    #[test]
    fn 파일_전체_검산() {
        let after = Totals {
            n: 100,
            o: 50,
            p: 30,
            q: 20,
        };
        let st = tg(100, 30, 20);
        assert_eq!(verify_file(&after, &st), Ok(()));
        assert_eq!(verify_file(&after, &tg(100, 20, 30)), Err(code::FILE_SUM_MISMATCH));
        assert_eq!(
            verify_file(
                &Totals {
                    n: 101,
                    ..after
                },
                &st
            ),
            Err(code::FILE_SUM_MISMATCH)
        );
    }

    #[test]
    fn 차례_글은_차례가_같으면_같다() {
        let a = pr(&[로봇, 미술]);
        let b = pr(&[로봇, 미술]);
        assert_eq!(a.canonical(), b.canonical());
        assert_ne!(a.canonical(), pr(&[미술, 로봇]).canonical());
        let mut c = a.clone();
        c.items = vec![수용비.into(), 강사료.into(), 교재비.into(), 재료비.into()];
        assert_ne!(a.canonical(), c.canonical());
    }
}
