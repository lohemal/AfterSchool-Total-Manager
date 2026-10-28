//! 방과후드림 결제파일 읽기 시험. 시험 파일은 코드로 만든다(`excel::dream_fixture`).

use super::*;
use crate::domain::dream_file::{code, Severity};
use crate::excel::dream_fixture::*;

fn codes(f: &DreamFile) -> Vec<&'static str> {
    f.file_issues.iter().map(|i| i.code).collect()
}

fn row_codes(f: &DreamFile, idx: usize) -> Vec<&'static str> {
    f.rows[idx].issues.iter().map(|i| i.code).collect()
}

fn ok(f: &DreamFile) -> bool {
    f.file_issues.is_empty() && f.rows.iter().all(|r| !r.has_error())
}

// ── 1~3 정상 · 선언 범위 · 서식만 있는 빈 행

#[test]
fn 정상_파일은_18열_네_줄을_읽는다() {
    let f = parse("방과후드림 결제 파일.xlsx", &with(sample_rows()));
    assert!(ok(&f), "{:?} {:?}", f.file_issues, f.rows.iter().map(|r| &r.issues).collect::<Vec<_>>());
    assert_eq!(f.sheet_name.as_deref(), Some("정산내역"));
    assert_eq!(f.rows.len(), 4);
    let r = &f.rows[0];
    assert_eq!(r.source_row, 2);
    assert_eq!(r.display.len(), 18);
    assert_eq!(r.accounting_year.as_deref(), Some("2026"));
    assert_eq!(r.org_code.as_deref(), Some("I100000987"));
    assert_eq!(r.personal_ref.as_ref().unwrap().expose(), "2024000001");
    assert_eq!(r.billing_month.as_deref(), Some("202609"));
    assert_eq!(r.course_code.as_deref(), Some("20260818PGM000003317"));
    assert_eq!(r.item_code, Some("INSTRUCTOR"));
    assert_eq!(r.amount_n, Some(35_000));
    assert_eq!((r.personal_before, r.voucher_before, r.free_voucher_before), (Some(0), Some(35_000), Some(0)));
    assert_eq!(r.transfer_date.as_deref(), Some("20260923"));
    assert_eq!((f.accounting_year.as_deref(), f.org_code.as_deref(), f.billing_month.as_deref()), (Some("2026"), Some("I100000987"), Some("202609")));
}

#[test]
fn 선언_범위는_621행이어도_실제_결제행은_2에서_5행이다() {
    // 2~621행 A~R 에 서식만 있는 빈 셀이 직렬화된 파일(실제 샘플과 같은 모양).
    // 셀 목록에서 그 행들은 값이 없으므로 결제행이 되지 않는다.
    let bytes = with(sample_rows());
    let sheet = cells::read_sheet(&bytes, SHEET).unwrap();
    let valued = sheet.rows.iter().filter(|r| r.iter().any(|c| !c.is_blank())).count();
    assert_eq!(valued, 5, "머리글 1줄 + 결제행 4줄만 값이 있다");
    let f = parse("x.xlsx", &bytes);
    assert_eq!(f.rows.iter().map(|r| r.source_row).collect::<Vec<_>>(), vec![2, 3, 4, 5]);
}

#[test]
fn 끝의_서식만_있는_빈_행은_몇_줄이든_무시한다() {
    let mut rows = sample_rows();
    rows.push(vec![V::Styled; 18]);
    rows.push(vec![V::Styled; 18]);
    let f = parse("x.xlsx", &with(rows));
    assert!(ok(&f));
    assert_eq!(f.rows.len(), 4);
}

// ── 4~5 중간 빈 행 · 부분 행

#[test]
fn 결제행_사이의_빈_행은_파일_오류다() {
    let mut rows = sample_rows();
    rows.insert(2, vec![V::Styled; 18]); // Excel 4행이 비었다
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(codes(&f), vec![code::INTERNAL_EMPTY_ROW]);
    assert_eq!(f.file_issues[0].row, Some(4));
    assert_eq!(f.rows.len(), 4, "빈 행을 건너뛰어 이어 붙이지 않고, 결제행 자체는 그대로 읽는다");
}

#[test]
fn 일부만_찬_행은_빈_행이_아니라_줄_오류다() {
    let mut partial = vec![V::None; 18];
    partial[6] = s("홍길동");
    let mut rows = sample_rows();
    rows.push(partial);
    let f = parse("x.xlsx", &with(rows));
    assert!(f.file_issues.is_empty());
    assert_eq!(f.rows.len(), 5);
    let c = row_codes(&f, 4);
    assert!(c.iter().filter(|x| **x == code::REQUIRED_VALUE_MISSING).count() >= 13, "{c:?}");
}

// ── 6~11 머리글 · 열 · 시트 · 빈 파일 · 손상

#[test]
fn 머리글_오타와_순서_바꿈은_오류고_줄을_읽지_않는다() {
    let mut h = header();
    h[2] = s("개인 참조번호");
    let mut all = vec![h];
    all.extend(sample_rows());
    let f = parse("x.xlsx", &book(&[(SHEET, all)], None));
    assert_eq!(codes(&f), vec![code::HEADER_MISMATCH]);
    assert_eq!(f.file_issues[0].column, Some("C"));
    assert!(f.rows.is_empty(), "열을 믿을 수 없으면 줄을 읽지 않는다");

    let mut h = header();
    h.swap(15, 16); // 이용권 ↔ 자유수강권
    let mut all = vec![h];
    all.extend(sample_rows());
    let f = parse("x.xlsx", &book(&[(SHEET, all)], None));
    assert_eq!(codes(&f), vec![code::HEADER_MISMATCH, code::HEADER_MISMATCH]);
    assert!(f.rows.is_empty());

    // 앞뒤 공백은 같은 머리글로 본다
    let mut h = header();
    h[0] = s(" 회계년도 ");
    let mut all = vec![h];
    all.extend(sample_rows());
    assert!(ok(&parse("x.xlsx", &book(&[(SHEET, all)], None))));
}

#[test]
fn r_오른쪽에_값이_있으면_오류다() {
    let mut rows = sample_rows();
    rows[1].push(s("메모"));
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(codes(&f), vec![code::EXTRA_DATA_COLUMN]);
    assert_eq!(f.file_issues[0].row, Some(3));
    assert_eq!(f.file_issues[0].found.as_deref(), Some("S"));
    // 서식만 있는 셀은 값이 아니다
    let mut rows = sample_rows();
    rows[1].push(V::Styled);
    assert!(ok(&parse("x.xlsx", &with(rows))));
}

#[test]
fn 대상_시트가_없거나_다른_시트가_있으면_오류다() {
    let mut all = vec![header()];
    all.extend(sample_rows());
    let f = parse("x.xlsx", &book(&[("Sheet1", all.clone())], None));
    assert_eq!(codes(&f), vec![code::SHEET_MISSING]);
    let f = parse("x.xlsx", &book(&[("정산내역 ", all.clone())], None));
    assert_eq!(codes(&f), vec![code::SHEET_MISSING], "비슷한 이름으로 짐작하지 않는다");
    let f = parse("x.xlsx", &book(&[(SHEET, all.clone()), ("메모", vec![vec![s("x")]])], None));
    assert_eq!(codes(&f), vec![code::EXTRA_SHEET]);
    assert_eq!(f.file_issues[0].found.as_deref(), Some("메모"));
}

#[test]
fn 머리글만_있고_결제행이_없으면_처리할_것이_없다() {
    let f = parse("x.xlsx", &with(Vec::new()));
    assert_eq!(codes(&f), vec![code::EMPTY_DATA]);
    assert!(f.rows.is_empty());
}

#[test]
fn 손상된_파일과_xlsx_가_아닌_파일은_막는다() {
    let good = with(sample_rows());
    let f = parse("x.xlsx", &good[..good.len() / 2]);
    assert_eq!(codes(&f), vec![code::WORKBOOK_CORRUPT]);
    let f = parse("x.xlsx", b"\xD0\xCF\x11\xE0not really an xls");
    assert_eq!(codes(&f), vec![code::WORKBOOK_CORRUPT]);
    for name in ["x.xls", "x.csv", "x.ods", "x.xlsm", "x"] {
        assert_eq!(codes(&parse(name, &good)), vec![code::FILE_NOT_XLSX], "{name}");
    }
    assert!(ok(&parse("대문자.XLSX", &good)));
}

// ── 12~15 숫자로 바뀐 코드

#[test]
fn 글자여야_할_코드가_숫자면_고쳐_맞추지_않고_오류다() {
    for (col, letter) in [(1usize, "B"), (2, "C"), (7, "H"), (12, "M"), (10, "K"), (17, "R")] {
        let mut rows = sample_rows();
        rows[0][col] = V::N(1.0);
        let f = parse("x.xlsx", &with(rows));
        let i = f.rows[0].issues.iter().find(|i| i.code == code::INVALID_CELL_TYPE).unwrap_or_else(|| panic!("{letter}"));
        assert_eq!(i.column, Some(letter));
        assert_eq!(i.expected.as_deref(), Some("TEXT"));
        assert!(f.rows[0].has_error());
    }
    // 금액 칸이 글자여도 오류다
    let mut rows = sample_rows();
    rows[0][13] = s("35000");
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(row_codes(&f, 0), vec![code::INVALID_CELL_TYPE]);
    // 참/거짓 셀
    let mut rows = sample_rows();
    rows[0][14] = V::B(true);
    assert!(row_codes(&parse("x.xlsx", &with(rows)), 0).contains(&code::INVALID_CELL_TYPE));
}

// ── 16~18 공통값 섞임

#[test]
fn 회계년도_기관코드_수강월이_섞이면_파일_오류다() {
    let mut rows = sample_rows();
    rows[1][0] = s("2027");
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(codes(&f), vec![code::MIXED_ACCOUNTING_YEAR]);
    assert_eq!(f.file_issues[0].found.as_deref(), Some("2026, 2027"));
    assert_eq!(f.accounting_year, None);

    let mut rows = sample_rows();
    rows[2][1] = s("I100000988");
    assert_eq!(codes(&parse("x.xlsx", &with(rows))), vec![code::MIXED_ORG_CODE]);

    let mut rows = sample_rows();
    rows[3][8] = V::N(202610.0);
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(codes(&f), vec![code::MIXED_BILLING_MONTH]);
    assert_eq!(f.file_issues[0].found.as_deref(), Some("202609, 202610"));
}

// ── 19 항목 · 20~24 금액

#[test]
fn 항목구분과_코드는_둘_다_맞아야_한다() {
    let mut rows = sample_rows();
    rows[0][12] = s("02"); // 강사료 / 02
    rows[1][11] = s("운영비"); // 운영비 / 02
    rows[2][12] = s("3"); // 교재비 / 3
    let f = parse("x.xlsx", &with(rows));
    for i in 0..3 {
        assert_eq!(row_codes(&f, i), vec![code::INVALID_ITEM_PAIR], "{i}");
        assert_eq!(f.rows[i].item_code, None);
    }
    assert_eq!(f.rows[0].issues[0].found.as_deref(), Some("강사료 / 02"));
}

#[test]
fn 정산금액은_0_이상의_정수_원이다() {
    let mut rows = sample_rows();
    rows[0][13] = V::N(-1.0);
    rows[1][13] = V::N(1500.5);
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(row_codes(&f, 0), vec![code::INVALID_AMOUNT]);
    assert_eq!(row_codes(&f, 1), vec![code::INVALID_AMOUNT]);
}

#[test]
fn 영원_행도_결제행이고_기존_배분도_0이어야_한다() {
    let f = parse("x.xlsx", &with(sample_rows()));
    let zero = &f.rows[2];
    assert_eq!((zero.source_row, zero.amount_n), (4, Some(0)));
    assert!(!zero.has_error());
    assert_eq!(f.rows.len(), 4, "0원 행을 빼지 않는다");

    let mut rows = sample_rows();
    rows[2][15] = V::N(100.0); // 0원 행에 이용권 100
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(row_codes(&f, 2), vec![code::INVALID_EXISTING_ALLOCATION]);
}

#[test]
fn 기존_배분이_0이거나_비어도_되고_합이_달라도_참고만_한다() {
    let mut rows = sample_rows();
    rows[0][14] = V::N(0.0);
    rows[0][15] = V::N(0.0);
    rows[0][16] = V::N(0.0); // 합 0 ≠ 35,000
    rows[1][14] = V::None;
    rows[1][15] = V::None;
    rows[1][16] = V::None; // 비어 있음
    let f = parse("x.xlsx", &with(rows));
    assert!(ok(&f), "기존 O/P/Q 는 정답으로 믿지 않으므로 막지 않는다");
    assert_eq!(row_codes(&f, 0), vec![code::EXISTING_SUM_MISMATCH]);
    assert_eq!(f.rows[0].issues[0].severity, Severity::Info);
    assert_eq!(f.rows[1].voucher_before, None);
    assert!(row_codes(&f, 1).is_empty());

    let mut rows = sample_rows();
    rows[0][16] = V::N(-5.0);
    rows[1][14] = V::N(0.5);
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(row_codes(&f, 0), vec![code::INVALID_EXISTING_ALLOCATION]);
    assert_eq!(row_codes(&f, 1), vec![code::INVALID_EXISTING_ALLOCATION]);
}

// ── 25~27 이체예정일

#[test]
fn 이체예정일은_비어도_되고_있으면_실제_날짜여야_한다() {
    let mut rows = sample_rows();
    rows[0][17] = V::None;
    rows[1][17] = V::Styled;
    rows[2][17] = s("20260931");
    rows[3][17] = s("2026-09-23");
    let f = parse("x.xlsx", &with(rows));
    assert!(row_codes(&f, 0).is_empty() && f.rows[0].transfer_date.is_none());
    assert!(row_codes(&f, 1).is_empty());
    assert_eq!(row_codes(&f, 2), vec![code::INVALID_TRANSFER_DATE]);
    assert_eq!(row_codes(&f, 3), vec![code::INVALID_TRANSFER_DATE]);
}

// ── 28~29 생년월일

#[test]
fn 생년월일은_yymmdd_실제_날짜만_받고_세기를_붙이지_않는다() {
    let f = parse("x.xlsx", &with(sample_rows()));
    assert_eq!(f.rows[0].birth.as_ref().unwrap().expose(), "170806", "YYYYMMDD 로 늘리지 않는다");
    for bad in ["171306", "170230", "20170806", "17086", "17O806"] {
        let mut rows = sample_rows();
        rows[0][7] = s(bad);
        let f = parse("x.xlsx", &with(rows));
        assert_eq!(row_codes(&f, 0), vec![code::INVALID_BIRTHDATE], "{bad}");
        assert!(f.rows[0].issues[0].found.is_none(), "생년월일 값을 오류에 싣지 않는다");
    }
}

// ── 30~32 학년 · 반

#[test]
fn 학년과_반은_원래_값과_비교용_후보를_함께_지닌다() {
    let f = parse("x.xlsx", &with(sample_rows()));
    let r = &f.rows[0];
    assert_eq!((r.grade_raw.as_str(), r.grade_candidate), ("3학년", Some(3)));
    assert_eq!((r.class_raw.as_str(), r.class_candidate.as_deref()), ("가람반", Some("가람")));

    let mut rows = sample_rows();
    rows[0][4] = s("반달반");
    rows[1][4] = s("반달");
    rows[2][3] = s("삼학년");
    let f = parse("x.xlsx", &with(rows));
    assert_eq!(f.rows[0].class_candidate.as_deref(), Some("반달"), "안쪽 반은 지우지 않는다");
    assert_eq!(f.rows[0].class_raw, "반달반");
    assert_eq!(f.rows[1].class_candidate, None);
    assert_eq!(f.rows[1].class_raw, "반달");
    assert_eq!(row_codes(&f, 2), vec![code::INVALID_GRADE]);
}

// ── 개인정보

#[test]
fn 화면용_칸과_디버그_출력에는_개인참조번호와_생년월일이_없다() {
    let f = parse("x.xlsx", &with(sample_rows()));
    let r = &f.rows[0];
    assert_eq!(r.display[2], "2******001");
    assert_eq!(r.display[7], "17****");
    let dbg = format!("{f:?}");
    assert!(!dbg.contains("2024000001") && !dbg.contains("170806"), "디버그 출력에 원래 값이 있다");
    // 오류의 found 에도 싣지 않는다
    let mut rows = sample_rows();
    rows[0][2] = s("2024-000001");
    let f = parse("x.xlsx", &with(rows));
    let all = format!("{:?}", f.rows[0].issues);
    assert!(!all.contains("2024-000001"), "{all}");
}

#[test]
fn 수강월은_숫자_여섯_자리_yyyymm_이어야_한다() {
    for bad in [V::N(20269.0), V::N(202613.0), V::N(202609.5), s("202609")] {
        let mut rows = sample_rows();
        rows[0][8] = bad;
        let f = parse("x.xlsx", &with(rows));
        let c = row_codes(&f, 0);
        assert!(c == vec![code::INVALID_BILLING_MONTH] || c == vec![code::INVALID_CELL_TYPE], "{c:?}");
    }
}
