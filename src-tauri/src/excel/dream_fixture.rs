//! 시험용 방과후드림 결제파일 만들기 — 실제 샘플과 같은 모양(시트 `정산내역`, 18열,
//! 2~621행 A~R 에 서식만 있는 빈 셀)을 코드로 만든다. 원본 xlsx 는 저장소에 넣지 않는다.
//! 기관코드 · 개인참조번호 · 학생은 모두 가짜다.

use rust_xlsxwriter::{Format, Workbook};

use crate::domain::dream::HEADERS;
use crate::excel::dream_file::SHEET;

#[derive(Clone)]
pub(crate) enum V {
    S(String),
    N(f64),
    B(bool),
    /// 서식만 있는 빈 셀
    Styled,
    /// 아무것도 없음
    None,
}

pub(crate) fn s(v: &str) -> V {
    V::S(v.to_string())
}

pub(crate) fn header() -> Vec<V> {
    HEADERS.iter().map(|h| s(h)).collect()
}

/// 실제 샘플과 같은 모양의 한 줄.
pub(crate) fn line(item: &str, item_code: &str, n: f64, o: f64, p: f64, q: f64) -> Vec<V> {
    vec![
        s("2026"),
        s("I100000987"), // B 기관코드 — 영문+숫자(실제 샘플 그대로)
        s("2024000001"), // C 개인참조번호
        s("3학년"),
        s("가람반"),
        s("1"),
        s("홍길동"),
        s("170806"),
        V::N(202609.0),
        s("항공드론B"),
        s("20260818PGM000003317"),
        s(item),
        s(item_code),
        V::N(n),
        V::N(o),
        V::N(p),
        V::N(q),
        s("20260923"),
    ]
}

/// 샘플의 네 줄(강사료 · 수용비 · 교재비 0원 · 재료비).
pub(crate) fn sample_rows() -> Vec<Vec<V>> {
    vec![
        line("강사료", "01", 35000.0, 0.0, 35000.0, 0.0),
        line("수용비", "02", 1500.0, 0.0, 1500.0, 0.0),
        line("교재비", "03", 0.0, 0.0, 0.0, 0.0),
        line("재료비", "04", 25000.0, 0.0, 25000.0, 0.0),
    ]
}

/// 시트들을 만들어 xlsx 바이트로. `styled_to` 가 있으면 그 행까지 A~R 에 서식만 있는 빈 셀.
pub(crate) fn book(sheets: &[(&str, Vec<Vec<V>>)], styled_to: Option<u32>) -> Vec<u8> {
    let mut wb = Workbook::new();
    let border = Format::new().set_border(rust_xlsxwriter::FormatBorder::Thin);
    for (name, rows) in sheets {
        let ws = wb.add_worksheet();
        ws.set_name(*name).unwrap();
        if let Some(last) = styled_to {
            for r in 1..last {
                for c in 0..18u16 {
                    ws.write_blank(r, c, &border).unwrap();
                }
            }
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let (r, c) = (r as u32, c as u16);
                match v {
                    V::S(t) => {
                        ws.write_string_with_format(r, c, t, &border).unwrap();
                    }
                    V::N(n) => {
                        ws.write_number_with_format(r, c, *n, &border).unwrap();
                    }
                    V::B(b) => {
                        ws.write_boolean(r, c, *b).unwrap();
                    }
                    V::Styled => {
                        ws.write_blank(r, c, &border).unwrap();
                    }
                    V::None => {}
                }
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

pub(crate) fn with(rows: Vec<Vec<V>>) -> Vec<u8> {
    let mut all = vec![header()];
    all.extend(rows);
    book(&[(SHEET, all)], Some(621))
}

