//! 방과후드림 결과 파일 patch 시험. 학생 · 번호는 모두 가짜다.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use rust_xlsxwriter::{Format, FormatBorder, Workbook};

use super::*;
use crate::excel::dream_fixture::{book, header, line, s, sample_rows, with, V};

const DATE: &str = "20261030";

/// (행, N, O, P, Q, R)
type Got = (u32, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<String>);

fn rows_of(bytes: &[u8]) -> Vec<u32> {
    let f = dream_file::parse("결제.xlsx", bytes);
    assert!(f.file_issues.is_empty(), "{:?}", f.file_issues);
    f.rows.iter().map(|r| r.source_row).collect()
}

fn plan(rows: &[(u32, i64, i64, i64, i64)]) -> PatchPlan {
    PatchPlan {
        rows: rows
            .iter()
            .map(|(r, n, o, p, q)| PatchRow {
                source_row: *r,
                amount_n: *n,
                o: *o,
                p: *p,
                q: *q,
            })
            .collect(),
        transfer_date: DATE.into(),
    }
}

/// 이름 → (압축을 푼 내용의 SHA-256), 차례대로
fn contents(bytes: &[u8]) -> Vec<(String, String)> {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.name().to_string(), crate::crypto::sha256_hex(&v))
        })
        .collect()
}

fn part_text(bytes: &[u8], part: &str) -> String {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(part).unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

/// 한 항목만 바꿔 다시 묶는다 — '잘못 만든 결과'를 흉내 낸다.
fn rezip(bytes: &[u8], part: &str, f: impl FnOnce(String) -> String) -> Vec<u8> {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    let mut f = Some(f);
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        let name = e.name().to_string();
        let mut v = Vec::new();
        e.read_to_end(&mut v).unwrap();
        if name == part {
            v = (f.take().unwrap())(String::from_utf8(v).unwrap()).into_bytes();
        }
        w.start_file(name, SimpleFileOptions::default()).unwrap();
        w.write_all(&v).unwrap();
    }
    w.finish().unwrap().into_inner()
}

const SHEET1: &str = "xl/worksheets/sheet1.xml";

/// 요청서의 숫자 예 — 2행 30,000 · 3행 10,000 · 4행 40,000
fn three() -> Vec<Vec<V>> {
    vec![
        line("강사료", "01", 30000.0, 30000.0, 0.0, 0.0),
        line("수용비", "02", 10000.0, 10000.0, 0.0, 0.0),
        line("재료비", "04", 40000.0, 40000.0, 0.0, 0.0),
    ]
}

fn three_plan() -> PatchPlan {
    plan(&[(2, 30_000, 0, 30_000, 0), (3, 10_000, 0, 10_000, 0), (4, 40_000, 10_000, 10_000, 20_000)])
}

fn run(src: &[u8], p: &PatchPlan) -> (Vec<u8>, PatchReport, VerifyReport) {
    let rows = rows_of(src);
    let (out, rep) = patch(src, p, &rows).unwrap();
    let v = verify(src, &out, p, &rows).unwrap();
    (out, rep, v)
}

// ─────────────────────────────────────────────── 값

#[test]
fn 계획의_값을_그_행에_그대로_적고_r_은_글자다() {
    let src = with(three());
    let (out, rep, v) = run(&src, &three_plan());
    let f = dream_file::parse("결과.xlsx", &out);
    let got: Vec<Got> = f
        .rows
        .iter()
        .map(|r| (r.source_row, r.amount_n, r.personal_before, r.voucher_before, r.free_voucher_before, r.transfer_date.clone()))
        .collect();
    let d = Some(DATE.to_string());
    assert_eq!(
        got,
        vec![
            (2, Some(30_000), Some(0), Some(30_000), Some(0), d.clone()),
            (3, Some(10_000), Some(0), Some(10_000), Some(0), d.clone()),
            (4, Some(40_000), Some(10_000), Some(10_000), Some(20_000), d),
        ]
    );
    // XML — O · P · Q 는 숫자 셀(t 없음), R 은 inlineStr, 서식 번호는 원래 그대로
    let x = part_text(&out, SHEET1);
    let orig = part_text(&src, SHEET1);
    let style = |xml: &str, r: &str| {
        let i = xml.find(&format!("<c r=\"{r}\"")).unwrap();
        let e = xml[i..].find('>').unwrap();
        attr_map(&xml[i..i + e + 1]).get("s").cloned()
    };
    for r in ["O2", "P3", "Q4", "R2", "R4"] {
        assert_eq!(style(&x, r), style(&orig, r), "{r} 서식");
    }
    assert!(x.contains(&format!("<c r=\"P2\" s=\"{}\"><v>30000</v></c>", style(&orig, "P2").unwrap())));
    assert!(x.contains(&format!("t=\"inlineStr\"><is><t>{DATE}</t></is></c>")));
    assert!(!x.contains("<c r=\"O4\" s=\"1\" t=\"n\""));
    assert_eq!(rep.data_rows, 3);
    // O2 는 30000 → 0, P2 0 → 30000 … R 은 셋 다(공유 문자열 → 글자)
    assert_eq!(v.target_cells_changed, rep.changed_cells);
    assert_eq!(v.a_to_n_changed, 0);
    // 결과를 다시 결제파일로 읽어도 구조가 정상이다
    assert!(f.file_issues.is_empty());
    assert!(f.rows.iter().all(|r| !r.has_error()));
}

#[test]
fn 영원_줄도_0_으로_적고_날짜를_넣는다() {
    let src = with(sample_rows()); // 4행이 교재비 0원
    let p = plan(&[(2, 35_000, 0, 35_000, 0), (3, 1_500, 0, 1_500, 0), (4, 0, 0, 0, 0), (5, 25_000, 5_000, 20_000, 0)]);
    let (out, ..) = run(&src, &p);
    let f = dream_file::parse("결과.xlsx", &out);
    let r4 = f.rows.iter().find(|r| r.source_row == 4).unwrap();
    assert_eq!(
        (r4.amount_n, r4.personal_before, r4.voucher_before, r4.free_voucher_before, r4.transfer_date.as_deref()),
        (Some(0), Some(0), Some(0), Some(0), Some(DATE))
    );
    assert_eq!(f.rows.len(), 4, "0원 줄도 지우지 않는다");
}

#[test]
fn 원본_배분과_달라도_결과는_계획대로고_원본_파일은_그대로다() {
    let src = with(vec![line("강사료", "01", 50000.0, 50000.0, 0.0, 0.0)]);
    let before = crate::crypto::sha256_hex(&src);
    let p = plan(&[(2, 50_000, 10_000, 30_000, 10_000)]);
    let (out, ..) = run(&src, &p);
    let r = &dream_file::parse("결과.xlsx", &out).rows[0];
    assert_eq!((r.personal_before, r.voucher_before, r.free_voucher_before), (Some(10_000), Some(30_000), Some(10_000)));
    assert_eq!(crate::crypto::sha256_hex(&src), before);
    let o = &dream_file::parse("결제.xlsx", &src).rows[0];
    assert_eq!((o.personal_before, o.voucher_before, o.free_voucher_before), (Some(50_000), Some(0), Some(0)));
}

#[test]
fn 이미_같은_값이면_그_셀은_건드리지_않는다() {
    let src = with(three());
    let p = plan(&[(2, 30_000, 30_000, 0, 0), (3, 10_000, 10_000, 0, 0), (4, 40_000, 40_000, 0, 0)]);
    let (_, rep, _) = run(&src, &p);
    assert_eq!(rep.changed_cells, 3, "R 셋만 바뀐다");
}

// ─────────────────────────────────────────────── 패키지 · 시트 보존

/// 실제 샘플 모양 — 열 너비 · 머리글 서식 · 결제행 서식 · 621행까지 서식만 있는 빈 셀
fn official(rows: Vec<Vec<V>>) -> Vec<u8> {
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_border(FormatBorder::Thin);
    let cell = Format::new().set_border(FormatBorder::Thin);
    let ws = wb.add_worksheet();
    ws.set_name(SHEET).unwrap();
    for c in 0..18u16 {
        ws.set_column_width(c, 12).unwrap();
    }
    for r in 1..621u32 {
        for c in 0..18u16 {
            ws.write_blank(r, c, &cell).unwrap();
        }
    }
    for (c, h) in header().iter().enumerate() {
        if let V::S(t) = h {
            ws.write_string_with_format(0, c as u16, t, &head).unwrap();
        }
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let (r, c) = (r as u32 + 1, c as u16);
            match v {
                V::S(t) => {
                    ws.write_string_with_format(r, c, t, &cell).unwrap();
                }
                V::N(n) => {
                    ws.write_number_with_format(r, c, *n, &cell).unwrap();
                }
                _ => {}
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

#[test]
fn 시트_밖_항목은_내용이_같고_차례도_같다() {
    let src = official(three());
    let (out, rep, v) = run(&src, &three_plan());
    let (a, b) = (contents(&src), contents(&out));
    assert_eq!(a.iter().map(|x| &x.0).collect::<Vec<_>>(), b.iter().map(|x| &x.0).collect::<Vec<_>>(), "항목 · 차례");
    for (x, y) in a.iter().zip(&b) {
        if x.0 == rep.sheet_part {
            assert_ne!(x.1, y.1);
        } else {
            assert_eq!(x.1, y.1, "{} 가 바뀌었다", x.0);
        }
    }
    for must in ["xl/styles.xml", "xl/sharedStrings.xml", "docProps/core.xml", "docProps/app.xml", "[Content_Types].xml", "xl/workbook.xml"] {
        assert!(a.iter().any(|x| x.0 == must), "{must} 가 fixture 에 있어야 시험이 된다");
    }
    assert_eq!(v.non_target_same, a.len() - 1);
}

#[test]
fn 차원_열너비_뒤쪽_서식행이_그대로다() {
    let src = official(three());
    let (out, ..) = run(&src, &three_plan());
    let (x0, x1) = (part_text(&src, SHEET1), part_text(&out, SHEET1));
    let head = |x: &str| x[..x.find("<sheetData").unwrap()].to_string();
    assert_eq!(head(&x0), head(&x1), "dimension · cols · sheetFormatPr");
    assert!(head(&x1).contains("<dimension ref=\"A1:R621\"/>"), "{}", head(&x1));
    assert!(head(&x1).contains("<col "), "열 너비");
    let tail = |x: &str| x[x.find("<row r=\"5\"").unwrap()..].to_string();
    assert_eq!(tail(&x0), tail(&x1), "5행부터 끝(621행)까지 서식만 있는 행이 바이트 그대로");
    assert_eq!(x0.matches("<row ").count(), x1.matches("<row ").count());
}

#[test]
fn 빈_셀_요소에도_서식을_지키며_적는다() {
    let mut rows = three();
    for r in rows.iter_mut() {
        for v in &mut r[14..18] {
            *v = V::Styled; // O~R 이 서식만 있는 빈 셀
        }
    }
    let src = with(rows);
    let (out, rep, _) = run(&src, &three_plan());
    assert_eq!(rep.inserted_cells, 0);
    assert_eq!(rep.changed_cells, 12);
    assert_eq!(dream_file::parse("결과.xlsx", &out).rows[2].free_voucher_before, Some(20_000));
}

#[test]
fn 셀_요소가_없으면_같은_열의_서식으로_넣는다() {
    // 서식행 없이 — 3행만 O~R 셀이 아예 없다
    let mut rows = three();
    for v in &mut rows[1][14..18] {
        *v = V::None;
    }
    let mut all = vec![header()];
    all.extend(rows);
    let src = book(&[(SHEET, all)], None);
    assert!(!part_text(&src, SHEET1).contains("r=\"O3\""));
    let (out, rep, v) = run(&src, &three_plan());
    assert_eq!(rep.inserted_cells, 4);
    let x = part_text(&out, SHEET1);
    let at = |r: &str| x.find(&format!("r=\"{r}\"")).unwrap();
    assert!(at("N3") < at("O3") && at("O3") < at("P3") && at("Q3") < at("R3") && at("R3") < at("A4"), "열 차례");
    let style = |r: &str| attr_map(&x[x[..at(r)].rfind("<c").unwrap()..]).get("s").cloned();
    assert_eq!(style("O3"), style("O2"));
    assert_eq!(style("R3"), style("R2"));
    assert_eq!(v.target_cells_changed, rep.changed_cells);
}

#[test]
fn 서식을_정할_수_없으면_셀을_넣지_않는다() {
    // 모든 결제행에 O~R 셀이 없다 — 어느 서식을 쓸지 알 수 없다
    let mut rows = three();
    for r in rows.iter_mut() {
        for v in &mut r[14..18] {
            *v = V::None;
        }
    }
    let mut all = vec![header()];
    all.extend(rows);
    let src = book(&[(SHEET, all)], None);
    assert_eq!(patch(&src, &three_plan(), &[2, 3, 4]).unwrap_err(), code::MISSING_CELL_STYLE_UNKNOWN);
}

// ─────────────────────────────────────────────── 막는 것

#[test]
fn 대상_셀에_수식이_있으면_만들지_않는다() {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(SHEET).unwrap();
    for (c, h) in header().iter().enumerate() {
        if let V::S(t) = h {
            ws.write_string(0, c as u16, t).unwrap();
        }
    }
    for (c, v) in three()[0].iter().enumerate() {
        match v {
            V::S(t) => {
                ws.write_string(1, c as u16, t).unwrap();
            }
            V::N(n) => {
                ws.write_number(1, c as u16, *n).unwrap();
            }
            _ => {}
        }
    }
    ws.write_formula(1, 14, "=N2-P2-Q2").unwrap();
    let src = wb.save_to_buffer().unwrap();
    let p = plan(&[(2, 30_000, 0, 30_000, 0)]);
    assert_eq!(patch(&src, &p, &[2]).unwrap_err(), code::TARGET_CELL_HAS_FORMULA);
}

#[test]
fn 대상_영역이_병합이면_만들지_않는다() {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(SHEET).unwrap();
    for (r, row) in [header()].into_iter().chain(three()).enumerate() {
        for (c, v) in row.iter().enumerate() {
            match v {
                V::S(t) => {
                    ws.write_string(r as u32, c as u16, t).unwrap();
                }
                V::N(n) => {
                    ws.write_number(r as u32, c as u16, *n).unwrap();
                }
                _ => {}
            }
        }
    }
    ws.merge_range(2, 15, 2, 16, "", &Format::new()).unwrap(); // P3:Q3
    let src = wb.save_to_buffer().unwrap();
    assert_eq!(patch(&src, &three_plan(), &[2, 3, 4]).unwrap_err(), code::TARGET_CELL_MERGED);
}

#[test]
fn 계획이_결제행과_정확히_맞지_않으면_만들지_않는다() {
    let src = with(three());
    let rows = [2, 3, 4];
    let mut p = three_plan();
    p.rows.push(p.rows[0]);
    assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::DUPLICATE_ROW, "같은 행 두 번");
    let mut p = three_plan();
    p.rows.pop();
    assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::MISSING_ROW, "빠진 행");
    let mut p = three_plan();
    p.rows.push(PatchRow {
        source_row: 9,
        amount_n: 0,
        o: 0,
        p: 0,
        q: 0,
    });
    assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::EXTRA_ROW, "결제행이 아닌 행");
    let mut p = three_plan();
    p.rows[0].o += 1;
    assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::ROW_SUM_MISMATCH);
    let mut p = three_plan();
    p.rows[2].o = -10_000;
    p.rows[2].p = 30_000;
    assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::NEGATIVE_AMOUNT);
    for d in ["20261332", "2026-10-30", "", "20260230"] {
        let mut p = three_plan();
        p.transfer_date = d.into();
        assert_eq!(patch(&src, &p, &rows).unwrap_err(), code::INVALID_TRANSFER_DATE, "{d}");
    }
    // 시트에 없는 행을 결제행이라고 넘겨도
    let p = plan(&[(2, 30_000, 0, 30_000, 0), (3, 10_000, 0, 10_000, 0), (4, 40_000, 0, 40_000, 0), (700, 0, 0, 0, 0)]);
    assert_eq!(patch(&src, &p, &[2, 3, 4, 700]).unwrap_err(), code::ROW_NOT_FOUND);
}

#[test]
fn 시트는_이름과_관계로_찾고_짐작하지_않는다() {
    // 정산내역이 두 번째 시트 — sheet2.xml 을 가리킨다
    let src = book(&[("안내", vec![vec![s("참고")]]), (SHEET, vec![header()])], None);
    assert_eq!(sheet_part(&read_entries(&src).unwrap()).unwrap(), "xl/worksheets/sheet2.xml");
    let none = book(&[("결제", vec![header()])], None);
    assert_eq!(sheet_part(&read_entries(&none).unwrap()).unwrap_err(), code::TARGET_SHEET_NOT_FOUND);
    assert_eq!(read_entries(b"not a zip").err(), Some(code::PACKAGE_CORRUPT));
    assert_eq!(resolve("xl", "worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml");
    assert_eq!(resolve("xl", "/xl/worksheets/s.xml"), "xl/worksheets/s.xml");
    assert_eq!(resolve("xl/sub", "../worksheets/s.xml"), "xl/worksheets/s.xml");
}

// ─────────────────────────────────────────────── 재검증

#[test]
fn 잘못_만든_결과는_재검증에서_걸린다() {
    let src = official(three());
    let p = three_plan();
    let rows = rows_of(&src);
    let (good, _) = patch(&src, &p, &rows).unwrap();
    assert!(verify(&src, &good, &p, &rows).is_ok());
    let bad = |part: &str, f: &dyn Fn(String) -> String| verify(&src, &rezip(&good, part, f), &p, &rows).unwrap_err();

    // O 값이 계획과 다르다
    assert_eq!(bad(SHEET1, &|x| x.replacen("<v>10000</v></c><c r=\"P4\"", "<v>9999</v></c><c r=\"P4\"", 1)), code::VERIFY_TARGET_VALUE);
    // N(A~N) 셀이 바뀌었다
    assert_eq!(bad(SHEET1, &|x| x.replacen("<v>40000</v>", "<v>40001</v>", 1)), code::VERIFY_CELL_CHANGED);
    // dimension 이 줄었다
    assert_eq!(
        bad(SHEET1, &|x| x.replace("<dimension ref=\"A1:R621\"/>", "<dimension ref=\"A1:R4\"/>")),
        code::VERIFY_SHEET_STRUCTURE_CHANGED
    );
    // 뒤쪽 서식행이 사라졌다
    assert_eq!(
        bad(SHEET1, &|x| {
            let s = x.find("<row r=\"621\"").unwrap();
            let e = x[s..].find("</row>").unwrap() + s + 6;
            format!("{}{}", &x[..s], &x[e..])
        }),
        code::VERIFY_ROWS_CHANGED
    );
    // 시트 밖 항목(서식)이 바뀌었다
    assert_eq!(bad("xl/styles.xml", &|x| x.replacen("<fonts", "<fonts ", 1)), code::VERIFY_NON_TARGET_CHANGED);
    // R 을 공유 문자열 그대로 두었다(날짜가 안 바뀜)
    assert_eq!(
        bad(SHEET1, &|x| x.replacen(&format!(" t=\"inlineStr\"><is><t>{DATE}</t></is></c>"), " t=\"inlineStr\"><is><t>20260923</t></is></c>", 1)),
        code::VERIFY_TARGET_VALUE
    );
    // 계획과 다른 결과를 원본처럼 넘기면(아무것도 안 고침)
    assert!(verify(&src, &src, &p, &rows).is_err());
}

#[test]
fn 결과를_다시_patch_해도_같은_값이다() {
    // 결과 파일을 다시 넣어도 결제파일로 정상이고, 같은 계획이면 R 외에는 바뀌지 않는다
    let src = official(three());
    let (out, ..) = run(&src, &three_plan());
    let (again, rep2, _) = run(&out, &three_plan());
    assert_eq!(rep2.changed_cells, 0, "이미 같은 값 — 아무 셀도 바뀌지 않는다");
    let by: BTreeMap<String, String> = contents(&again).into_iter().collect();
    assert_eq!(by[SHEET1], contents(&out).into_iter().find(|x| x.0 == SHEET1).unwrap().1);
}
