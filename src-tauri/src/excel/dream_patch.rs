//! 방과후드림 결과 파일 — 원본 XLSX 패키지를 그대로 옮기며 `정산내역` 시트의 **실제 결제행
//! O · P · Q · R 셀만** 바꾼다 (P5). DB 도 회계 규칙도 모른다 — 받은 값을 받은 행에 적을 뿐이다.
//!
//! ```text
//! 패키지   _rels/.rels → 통합문서 → 통합문서 rels → `정산내역` 이 가리키는 시트 부분
//!          (sheet1.xml 로 짐작하지 않는다 — 못 찾으면 만들지 않는다)
//!          시트 밖의 항목은 압축된 바이트 그대로 복사(raw copy), 항목 차례도 그대로
//! 시트     XML 을 다시 쓰지 않는다. 바꿀 셀 요소의 바이트 구간만 갈아 끼운다
//!          O · P · Q  숫자 셀 <v>정수</v>   (s 서식 번호 등 다른 속성은 그대로, t 만 뺀다)
//!          R          글자 셀 t="inlineStr" <is><t>YYYYMMDD</t></is>  (sharedStrings 를 건드리지 않는다)
//!          셀 요소가 없으면 같은 열의 다른 결제행 서식이 하나로 정해질 때만 넣는다
//! 막는 것  대상 셀의 수식 · 병합, 같은 행 두 번, 빠진 행, 결제행이 아닌 행, N ≠ O + P + Q,
//!          접두사 붙은 XML, r 없는 행 · 셀, 차례가 어긋난 셀
//! ```
//!
//! 만든 결과는 `verify` 가 원본과 맞대어 다시 본다 — 패키지 항목(시트 밖은 내용 SHA-256 이
//! 같아야), 시트 XML(바뀐 셀 밖은 바이트가 같아야), 결제파일 읽기(A~N 셀 값 · 타입이 같고
//! O · P · Q · R 이 계획과 같아야). 하나라도 어긋나면 결과를 쓰지 않는다.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::domain::dream_file::Severity;
use crate::excel::cells::{self, Cell};
use crate::excel::dream_file::{self, SHEET};

/// 바꿀 열 — O(15) · P(16) · Q(17) · R(18), 1부터 센 열 번호.
const COL_O: u32 = 15;
const COL_R: u32 = 18;
/// 패키지 전체를 풀었을 때의 상한 — 압축 폭탄을 막는다(결제파일은 수백 KB).
const MAX_UNPACKED: u64 = 200 * 1024 * 1024;

/// 한 결제행에 적을 값. 이름 · 번호 같은 개인정보는 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchRow {
    pub source_row: u32,
    /// 검산용 — 쓰지 않는다
    pub amount_n: i64,
    pub o: i64,
    pub p: i64,
    pub q: i64,
}

/// 결과 파일에 적을 것 전체.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchPlan {
    pub rows: Vec<PatchRow>,
    /// `YYYYMMDD`
    pub transfer_date: String,
}

/// 실패 까닭.
pub mod code {
    pub const INVALID_TRANSFER_DATE: &str = "PATCH_INVALID_TRANSFER_DATE";
    pub const DUPLICATE_ROW: &str = "PATCH_DUPLICATE_ROW";
    pub const MISSING_ROW: &str = "PATCH_MISSING_ROW";
    pub const EXTRA_ROW: &str = "PATCH_EXTRA_ROW";
    pub const ROW_SUM_MISMATCH: &str = "PATCH_ROW_SUM_MISMATCH";
    pub const NEGATIVE_AMOUNT: &str = "PATCH_NEGATIVE_AMOUNT";
    pub const PACKAGE_CORRUPT: &str = "PATCH_PACKAGE_CORRUPT";
    pub const PACKAGE_TOO_LARGE: &str = "PATCH_PACKAGE_TOO_LARGE";
    pub const TARGET_SHEET_NOT_FOUND: &str = "PATCH_TARGET_SHEET_NOT_FOUND";
    pub const UNSUPPORTED_SHEET_XML: &str = "PATCH_UNSUPPORTED_SHEET_XML";
    pub const ROW_NOT_FOUND: &str = "PATCH_ROW_NOT_FOUND";
    pub const TARGET_CELL_HAS_FORMULA: &str = "TARGET_CELL_HAS_FORMULA";
    pub const TARGET_CELL_MERGED: &str = "TARGET_CELL_MERGED";
    pub const TARGET_CELL_UNSUPPORTED: &str = "PATCH_TARGET_CELL_UNSUPPORTED";
    pub const MISSING_CELL_STYLE_UNKNOWN: &str = "PATCH_MISSING_CELL_STYLE_UNKNOWN";
    // 재검증
    pub const VERIFY_ENTRIES_CHANGED: &str = "VERIFY_ENTRIES_CHANGED";
    pub const VERIFY_NON_TARGET_CHANGED: &str = "VERIFY_NON_TARGET_CHANGED";
    pub const VERIFY_SHEET_STRUCTURE_CHANGED: &str = "VERIFY_SHEET_STRUCTURE_CHANGED";
    pub const VERIFY_ROWS_CHANGED: &str = "VERIFY_ROWS_CHANGED";
    pub const VERIFY_CELL_CHANGED: &str = "VERIFY_CELL_CHANGED";
    pub const VERIFY_TARGET_VALUE: &str = "VERIFY_TARGET_VALUE";
    pub const VERIFY_PARSE_FAILED: &str = "VERIFY_PARSE_FAILED";
    pub const VERIFY_A_TO_N_CHANGED: &str = "VERIFY_A_TO_N_CHANGED";
}

/// 만든 결과 요약 — 개인정보 없음.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchReport {
    /// 시트 부분 이름(`xl/worksheets/sheet1.xml` 등)
    pub sheet_part: String,
    pub entries: usize,
    pub data_rows: usize,
    /// 바이트가 바뀐 O · P · Q · R 셀(새로 넣은 셀 포함)
    pub changed_cells: usize,
    /// 셀 요소가 없어 새로 넣은 셀
    pub inserted_cells: usize,
}

/// 재검증 결과 — 개인정보 없음.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub entries: usize,
    /// 내용이 같은지 SHA-256 으로 본 시트 밖 항목 수
    pub non_target_same: usize,
    pub data_rows: usize,
    pub sheet_rows: usize,
    /// 값 · 타입이 바뀐 O · P · Q · R 셀
    pub target_cells_changed: usize,
    /// 바뀐 A~N 셀 — 언제나 0 이어야 통과한다
    pub a_to_n_changed: usize,
}

// ─────────────────────────────────────────────── 패키지

struct Entry {
    name: String,
    method: CompressionMethod,
    modified: Option<zip::DateTime>,
    data: Vec<u8>,
}

fn read_entries(bytes: &[u8]) -> Result<Vec<Entry>, &'static str> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| code::PACKAGE_CORRUPT)?;
    let mut total = 0u64;
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|_| code::PACKAGE_CORRUPT)?;
        total += f.size();
        if total > MAX_UNPACKED {
            return Err(code::PACKAGE_TOO_LARGE);
        }
        let name = f.name().to_string();
        if !seen.insert(name.clone()) {
            return Err(code::PACKAGE_CORRUPT);
        }
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data).map_err(|_| code::PACKAGE_CORRUPT)?;
        out.push(Entry {
            name,
            method: f.compression(),
            modified: f.last_modified(),
            data,
        });
    }
    Ok(out)
}

fn entry<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|e| e.name == name)
}

fn utf8(e: &Entry) -> Result<&str, &'static str> {
    std::str::from_utf8(&e.data).map_err(|_| code::UNSUPPORTED_SHEET_XML)
}

/// `dir` 기준 상대 경로를 패키지 안 이름으로.
fn resolve(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn dir_of(part: &str) -> &str {
    part.rfind('/').map(|i| &part[..i]).unwrap_or("")
}

fn rels_of(part: &str) -> String {
    let (d, f) = match part.rfind('/') {
        Some(i) => (&part[..i], &part[i + 1..]),
        None => ("", part),
    };
    if d.is_empty() {
        format!("_rels/{f}.rels")
    } else {
        format!("{d}/_rels/{f}.rels")
    }
}

/// rels 안에서 조건에 맞는 관계의 대상.
fn rel_target(xml: &str, pick: impl Fn(&HashMap<String, String>) -> bool) -> Vec<String> {
    tags(xml, "Relationship")
        .into_iter()
        .filter_map(|(s, e)| {
            let a = attr_map(&xml[s..e]);
            if a.get("TargetMode").is_some_and(|m| m == "External") || !pick(&a) {
                return None;
            }
            a.get("Target").map(|t| unescape(t))
        })
        .collect()
}

/// `정산내역` 시트 부분 이름. 하나로 정해지지 않으면 오류.
fn sheet_part(entries: &[Entry]) -> Result<String, &'static str> {
    let nf = code::TARGET_SHEET_NOT_FOUND;
    let root = utf8(entry(entries, "_rels/.rels").ok_or(nf)?)?;
    let books = rel_target(root, |a| a.get("Type").is_some_and(|t| t.ends_with("/officeDocument")));
    let [book] = books.as_slice() else { return Err(nf) };
    let book = resolve("", book);
    let wb = utf8(entry(entries, &book).ok_or(nf)?)?;
    let sheets: Vec<HashMap<String, String>> = tags(wb, "sheet").into_iter().map(|(s, e)| attr_map(&wb[s..e])).collect();
    let mine: Vec<&HashMap<String, String>> = sheets
        .iter()
        .filter(|a| a.get("name").map(|n| unescape(n)).as_deref() == Some(SHEET))
        .collect();
    let [one] = mine.as_slice() else { return Err(nf) };
    let rid = one.get("r:id").ok_or(nf)?;
    let rels = utf8(entry(entries, &rels_of(&book)).ok_or(nf)?)?;
    let targets = rel_target(rels, |a| {
        a.get("Id") == Some(rid) && a.get("Type").is_some_and(|t| t.ends_with("/worksheet"))
    });
    let [t] = targets.as_slice() else { return Err(nf) };
    let part = resolve(dir_of(&book), t);
    entry(entries, &part).ok_or(nf)?;
    Ok(part)
}

// ─────────────────────────────────────────────── XML 조각 읽기 (바이트 위치 그대로)

/// `start` 의 `<` 부터 짝이 맞는 `>` 다음 위치. 따옴표 안의 `>` 는 건너뛴다.
fn tag_end(xml: &str, start: usize) -> Option<usize> {
    let b = xml.as_bytes();
    let mut q: Option<u8> = None;
    let mut i = start;
    while i < b.len() {
        let c = b[i];
        match q {
            Some(x) if c == x => q = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => q = Some(c),
            None if c == b'>' => return Some(i + 1),
            None => {}
        }
        i += 1;
    }
    None
}

/// 이름이 정확히 `name` 인 여는 태그들의 (시작, 끝) — `<name` 다음이 공백 · `>` · `/` 인 것만.
fn tags(xml: &str, name: &str) -> Vec<(usize, usize)> {
    tags_in(xml, name, 0, xml.len())
}

fn tags_in(xml: &str, name: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let pat = format!("<{name}");
    let mut out = Vec::new();
    let mut i = from;
    while let Some(off) = xml[i..to].find(&pat) {
        let s = i + off;
        let next = xml.as_bytes().get(s + pat.len()).copied();
        if matches!(next, Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/')) {
            match tag_end(xml, s) {
                Some(e) if e <= to => {
                    out.push((s, e));
                    i = e;
                    continue;
                }
                _ => break,
            }
        }
        i = s + pat.len();
    }
    out
}

/// 여는 태그의 속성 — (이름, 원래 값 글자), 원래 차례대로. 값의 엔티티는 풀지 않는다.
fn attrs(tag: &str) -> Vec<(String, String)> {
    let b = tag.as_bytes();
    let mut i = 1;
    while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
        i += 1;
    }
    let mut out = Vec::new();
    loop {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || matches!(b[i], b'>' | b'/') {
            break;
        }
        let ns = i;
        while i < b.len() && !matches!(b[i], b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
            i += 1;
        }
        let name = tag[ns..i].to_string();
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            break;
        }
        i += 1;
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        let Some(&qc) = b.get(i).filter(|c| matches!(c, b'"' | b'\'')) else { break };
        let vs = i + 1;
        let Some(len) = tag[vs..].find(qc as char) else { break };
        out.push((name, tag[vs..vs + len].to_string()));
        i = vs + len + 1;
    }
    out
}

fn attr_map(tag: &str) -> HashMap<String, String> {
    attrs(tag).into_iter().collect()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `O17` → (15, 17). 글자 · 숫자 꼴이 아니면 None.
fn cell_ref(r: &str) -> Option<(u32, u32)> {
    let split = r.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = r.split_at(split);
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_uppercase()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let col = letters.bytes().fold(0u32, |a, b| a * 26 + (b - b'A' + 1) as u32);
    Some((col, digits.parse().ok()?))
}

fn col_letters(mut c: u32) -> String {
    let mut s = String::new();
    while c > 0 {
        let m = (c - 1) % 26;
        s.insert(0, (b'A' + m as u8) as char);
        c = (c - 1) / 26;
    }
    s
}

#[derive(Debug, Clone)]
struct CellX {
    col: u32,
    start: usize,
    end: usize,
    /// 여는 태그 끝(self-closing 이면 end 와 같다)
    open_end: usize,
    attrs: Vec<(String, String)>,
}

impl CellX {
    fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
    fn inner<'a>(&self, xml: &'a str) -> &'a str {
        if self.open_end == self.end {
            ""
        } else {
            &xml[self.open_end..self.end - "</c>".len()]
        }
    }
}

#[derive(Debug, Clone)]
struct RowX {
    r: u32,
    start: usize,
    open_end: usize,
    /// `</row>` 시작(self-closing 이면 None)
    close: Option<usize>,
    attrs: Vec<(String, String)>,
    cells: Vec<CellX>,
}

/// 시트 XML 을 행 · 셀 바이트 위치로. (sheetData 시작, 끝, 행들)
struct SheetX {
    data_start: usize,
    data_end: usize,
    rows: Vec<RowX>,
}

fn scan_sheet(xml: &str) -> Result<SheetX, &'static str> {
    let bad = code::UNSUPPORTED_SHEET_XML;
    // 접두사 붙은 꼴(<x:sheetData> 등)은 다루지 않는다
    if xml.contains(":sheetData") {
        return Err(bad);
    }
    let sd = tags(xml, "sheetData");
    let [(ds, de)] = sd.as_slice() else { return Err(bad) };
    if xml[*ds..*de].ends_with("/>") {
        return Ok(SheetX {
            data_start: *ds,
            data_end: *de,
            rows: Vec::new(),
        });
    }
    let close = xml[*de..].find("</sheetData>").map(|i| de + i).ok_or(bad)?;
    let mut rows = Vec::new();
    let mut i = *de;
    for (rs, re) in tags_in(xml, "row", *de, close) {
        if rs < i {
            return Err(bad);
        }
        let a = attrs(&xml[rs..re]);
        let r: u32 = a.iter().find(|(n, _)| n == "r").and_then(|(_, v)| v.parse().ok()).ok_or(bad)?;
        let (rclose, rend) = if xml[rs..re].ends_with("/>") {
            (None, re)
        } else {
            let c = xml[re..close].find("</row>").map(|x| re + x).ok_or(bad)?;
            (Some(c), c + "</row>".len())
        };
        let mut cells = Vec::new();
        if let Some(c) = rclose {
            let mut last_col = 0;
            for (cs, ce) in tags_in(xml, "c", re, c) {
                let ca = attrs(&xml[cs..ce]);
                let (col, row) = ca.iter().find(|(n, _)| n == "r").and_then(|(_, v)| cell_ref(v)).ok_or(bad)?;
                if row != r || col <= last_col {
                    return Err(bad);
                }
                last_col = col;
                let end = if xml[cs..ce].ends_with("/>") {
                    ce
                } else {
                    xml[ce..c].find("</c>").map(|x| ce + x + "</c>".len()).ok_or(bad)?
                };
                cells.push(CellX {
                    col,
                    start: cs,
                    end,
                    open_end: ce,
                    attrs: ca,
                });
            }
        }
        if rows.last().is_some_and(|p: &RowX| p.r >= r) {
            return Err(bad);
        }
        rows.push(RowX {
            r,
            start: rs,
            open_end: re,
            close: rclose,
            attrs: a,
            cells,
        });
        i = rend;
    }
    Ok(SheetX {
        data_start: *ds,
        data_end: close + "</sheetData>".len(),
        rows,
    })
}

/// 병합 범위 가운데 결제행 O~R 을 덮는 것이 있는가.
fn merged_target(xml: &str, rows: &BTreeSet<u32>) -> bool {
    tags(xml, "mergeCell").into_iter().any(|(s, e)| {
        let Some(r) = attr_map(&xml[s..e]).get("ref").cloned() else { return true };
        let (a, b) = r.split_once(':').unwrap_or((&r, &r));
        let (Some((c1, r1)), Some((c2, r2))) = (cell_ref(a), cell_ref(b)) else { return true };
        let (c1, c2) = (c1.min(c2), c1.max(c2));
        let (r1, r2) = (r1.min(r2), r1.max(r2));
        c2 >= COL_O && c1 <= COL_R && rows.range(r1..=r2).next().is_some()
    })
}

fn has_child(inner: &str, name: &str) -> bool {
    !tags(inner, name).is_empty()
}

fn render_attrs(attrs: &[(String, String)], drop_t: bool) -> String {
    attrs
        .iter()
        .filter(|(n, _)| !(drop_t && n == "t"))
        .map(|(n, v)| format!(" {n}=\"{v}\""))
        .collect()
}

fn number_cell(attrs: &[(String, String)], n: i64) -> String {
    format!("<c{}><v>{n}</v></c>", render_attrs(attrs, true))
}

fn date_cell(attrs: &[(String, String)], d: &str) -> String {
    format!("<c{} t=\"inlineStr\"><is><t>{d}</t></is></c>", render_attrs(attrs, true))
}

// ─────────────────────────────────────────────── 만들기

fn check_plan(plan: &PatchPlan, data_rows: &[u32]) -> Result<BTreeMap<u32, PatchRow>, &'static str> {
    if crate::domain::dream::check_transfer_date(&plan.transfer_date).as_deref() != Ok(plan.transfer_date.as_str()) {
        return Err(code::INVALID_TRANSFER_DATE);
    }
    let mut by_row = BTreeMap::new();
    for r in &plan.rows {
        if r.o < 0 || r.p < 0 || r.q < 0 || r.amount_n < 0 {
            return Err(code::NEGATIVE_AMOUNT);
        }
        if r.o + r.p + r.q != r.amount_n {
            return Err(code::ROW_SUM_MISMATCH);
        }
        if by_row.insert(r.source_row, *r).is_some() {
            return Err(code::DUPLICATE_ROW);
        }
    }
    let want: BTreeSet<u32> = data_rows.iter().copied().collect();
    if want.len() != data_rows.len() {
        return Err(code::DUPLICATE_ROW);
    }
    if by_row.keys().any(|r| !want.contains(r)) {
        return Err(code::EXTRA_ROW);
    }
    if want.iter().any(|r| !by_row.contains_key(r)) {
        return Err(code::MISSING_ROW);
    }
    Ok(by_row)
}

/// 셀 요소가 없는 열에 쓸 서식 — 같은 열의 다른 결제행 셀 서식이 하나뿐일 때만.
fn style_for(sheet: &SheetX, data: &BTreeSet<u32>, col: u32) -> Result<Option<String>, &'static str> {
    let styles: BTreeSet<Option<String>> = sheet
        .rows
        .iter()
        .filter(|r| data.contains(&r.r))
        .filter_map(|r| r.cells.iter().find(|c| c.col == col))
        .map(|c| c.attr("s").map(str::to_string))
        .collect();
    match styles.len() {
        1 => Ok(styles.into_iter().next().expect("하나")),
        _ => Err(code::MISSING_CELL_STYLE_UNKNOWN),
    }
}

/// 원본 바이트에서 결과 바이트를 만든다. 원본은 읽기만 한다.
///
/// `data_rows` 는 결제파일 읽기(P2)가 정한 실제 결제행 번호들이다. 계획은 그 행 모두를
/// 정확히 한 번씩 담아야 한다.
pub fn patch(src: &[u8], plan: &PatchPlan, data_rows: &[u32]) -> Result<(Vec<u8>, PatchReport), &'static str> {
    let by_row = check_plan(plan, data_rows)?;
    let entries = read_entries(src)?;
    let part = sheet_part(&entries)?;
    let target = entry(&entries, &part).expect("sheet_part 가 확인했다");
    let xml = utf8(target)?;
    let sheet = scan_sheet(xml)?;
    let data: BTreeSet<u32> = by_row.keys().copied().collect();
    if merged_target(xml, &data) {
        return Err(code::TARGET_CELL_MERGED);
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut report = PatchReport {
        sheet_part: part.clone(),
        entries: entries.len(),
        data_rows: data.len(),
        ..Default::default()
    };
    let mut styles: HashMap<u32, Option<String>> = HashMap::new();
    for row in sheet.rows.iter().filter(|r| data.contains(&r.r)) {
        let pr = by_row[&row.r];
        let Some(close) = row.close else { return Err(code::ROW_NOT_FOUND) };
        let mut inserted_max = 0;
        for col in COL_O..=COL_R {
            let want_val = |a: &[(String, String)]| match col {
                15 => number_cell(a, pr.o),
                16 => number_cell(a, pr.p),
                17 => number_cell(a, pr.q),
                _ => date_cell(a, &plan.transfer_date),
            };
            match row.cells.iter().find(|c| c.col == col) {
                Some(c) => {
                    let inner = c.inner(xml);
                    if has_child(inner, "f") || c.attr("t") == Some("array") {
                        return Err(code::TARGET_CELL_HAS_FORMULA);
                    }
                    if c.attr("vm").is_some() || c.attr("cm").is_some() {
                        return Err(code::TARGET_CELL_UNSUPPORTED);
                    }
                    let new = want_val(&c.attrs);
                    if xml[c.start..c.end] != new {
                        report.changed_cells += 1;
                        edits.push((c.start, c.end, new));
                    }
                }
                None => {
                    let style = match styles.get(&col) {
                        Some(s) => s.clone(),
                        None => {
                            let s = style_for(&sheet, &data, col)?;
                            styles.insert(col, s.clone());
                            s
                        }
                    };
                    let mut a = vec![("r".to_string(), format!("{}{}", col_letters(col), row.r))];
                    if let Some(s) = style {
                        a.push(("s".to_string(), s));
                    }
                    let at = row.cells.iter().find(|c| c.col > col).map(|c| c.start).unwrap_or(close);
                    edits.push((at, at, want_val(&a)));
                    report.changed_cells += 1;
                    report.inserted_cells += 1;
                    inserted_max = col;
                }
            }
        }
        // 셀을 넣었고 행의 spans 가 그 열을 덮지 않으면 끝 값만 넓힌다(행의 다른 속성은 그대로)
        if inserted_max > 0 {
            if let Some((_, v)) = row.attrs.iter().find(|(n, _)| n == "spans") {
                if let Some((lo, hi)) = v.split_once(':') {
                    if hi.parse::<u32>().map(|h| h < inserted_max).unwrap_or(true) {
                        let tag = &xml[row.start..row.open_end];
                        let old = format!("spans=\"{v}\"");
                        let pos = tag.find(&old).ok_or(code::UNSUPPORTED_SHEET_XML)?;
                        let s = row.start + pos;
                        edits.push((s, s + old.len(), format!("spans=\"{lo}:{inserted_max}\"")));
                    }
                }
            }
        }
    }
    if sheet.rows.iter().filter(|r| data.contains(&r.r)).count() != data.len() {
        return Err(code::ROW_NOT_FOUND);
    }

    edits.sort_by_key(|e| (e.0, e.1));
    let mut out_xml = String::with_capacity(xml.len() + edits.len() * 16);
    let mut at = 0;
    for (s, e, rep) in &edits {
        if *s < at {
            return Err(code::UNSUPPORTED_SHEET_XML);
        }
        out_xml.push_str(&xml[at..*s]);
        out_xml.push_str(rep);
        at = *e;
    }
    out_xml.push_str(&xml[at..]);

    // 패키지 — 시트 밖은 압축된 그대로, 차례도 그대로
    let mut zin = ZipArchive::new(Cursor::new(src)).map_err(|_| code::PACKAGE_CORRUPT)?;
    let mut w = ZipWriter::new(Cursor::new(Vec::with_capacity(src.len() + 1024)));
    for i in 0..zin.len() {
        let name = zin.by_index_raw(i).map_err(|_| code::PACKAGE_CORRUPT)?.name().to_string();
        if name == part {
            let method = match target.method {
                CompressionMethod::Stored => CompressionMethod::Stored,
                _ => CompressionMethod::Deflated,
            };
            let mut opt = SimpleFileOptions::default().compression_method(method);
            if let Some(t) = target.modified {
                opt = opt.last_modified_time(t);
            }
            w.start_file(name, opt).map_err(|_| code::PACKAGE_CORRUPT)?;
            w.write_all(out_xml.as_bytes()).map_err(|_| code::PACKAGE_CORRUPT)?;
        } else {
            let f = zin.by_index_raw(i).map_err(|_| code::PACKAGE_CORRUPT)?;
            w.raw_copy_file(f).map_err(|_| code::PACKAGE_CORRUPT)?;
        }
    }
    let out = w.finish().map_err(|_| code::PACKAGE_CORRUPT)?.into_inner();
    Ok((out, report))
}

// ─────────────────────────────────────────────── 재검증

fn sha(b: &[u8]) -> String {
    crate::crypto::sha256_hex(b)
}

/// 값 셀의 내용 — `<v>…</v>` 의 글자.
fn v_text(inner: &str) -> Option<&str> {
    let s = inner.find("<v>")? + 3;
    let e = inner[s..].find("</v>")? + s;
    Some(&inner[s..e])
}

/// 결과를 원본과 맞대어 다시 본다. 계획과 다르거나 허용 밖의 변경이 있으면 오류.
pub fn verify(src: &[u8], out: &[u8], plan: &PatchPlan, data_rows: &[u32]) -> Result<VerifyReport, &'static str> {
    let by_row = check_plan(plan, data_rows)?;
    let data: BTreeSet<u32> = by_row.keys().copied().collect();
    let a = read_entries(src)?;
    let b = read_entries(out).map_err(|_| code::VERIFY_PARSE_FAILED)?;
    let part = sheet_part(&a)?;
    if sheet_part(&b).map_err(|_| code::VERIFY_ENTRIES_CHANGED)? != part {
        return Err(code::VERIFY_ENTRIES_CHANGED);
    }
    // 1) 패키지 — 항목 이름 · 차례 · 압축 방식이 같고, 시트 밖 항목은 내용이 같다
    if a.len() != b.len() || a.iter().zip(&b).any(|(x, y)| x.name != y.name) {
        return Err(code::VERIFY_ENTRIES_CHANGED);
    }
    let mut rep = VerifyReport {
        entries: a.len(),
        data_rows: data.len(),
        ..Default::default()
    };
    for (x, y) in a.iter().zip(&b) {
        if x.name == part {
            continue;
        }
        if sha(&x.data) != sha(&y.data) || x.method != y.method {
            return Err(code::VERIFY_NON_TARGET_CHANGED);
        }
        rep.non_target_same += 1;
    }

    // 2) 시트 XML — sheetData 밖은 바이트가 같고, 행 차례 · 행 속성이 같고, 대상 밖 셀은 그대로
    let xa = utf8(entry(&a, &part).expect("있다"))?;
    let xb = utf8(entry(&b, &part).expect("있다")).map_err(|_| code::VERIFY_PARSE_FAILED)?;
    let sa = scan_sheet(xa)?;
    let sb = scan_sheet(xb).map_err(|_| code::VERIFY_PARSE_FAILED)?;
    if xa[..sa.data_start] != xb[..sb.data_start] || xa[sa.data_end..] != xb[sb.data_end..] {
        return Err(code::VERIFY_SHEET_STRUCTURE_CHANGED);
    }
    if sa.rows.len() != sb.rows.len() || sa.rows.iter().zip(&sb.rows).any(|(x, y)| x.r != y.r) {
        return Err(code::VERIFY_ROWS_CHANGED);
    }
    rep.sheet_rows = sa.rows.len();
    for (ra, rb) in sa.rows.iter().zip(&sb.rows) {
        let is_data = data.contains(&ra.r);
        // 행 여는 태그 — 같아야 한다. 결제행은 spans 끝 값만 넓어질 수 있다
        if xa[ra.start..ra.open_end] != xb[rb.start..rb.open_end] {
            let strip = |v: &[(String, String)]| v.iter().filter(|(n, _)| n != "spans").cloned().collect::<Vec<_>>();
            if !is_data || strip(&ra.attrs) != strip(&rb.attrs) {
                return Err(code::VERIFY_ROWS_CHANGED);
            }
        }
        let keep_a: Vec<&CellX> = ra.cells.iter().filter(|c| !(is_data && c.col >= COL_O && c.col <= COL_R)).collect();
        let keep_b: Vec<&CellX> = rb.cells.iter().filter(|c| !(is_data && c.col >= COL_O && c.col <= COL_R)).collect();
        if keep_a.len() != keep_b.len()
            || keep_a.iter().zip(&keep_b).any(|(x, y)| xa[x.start..x.end] != xb[y.start..y.end])
        {
            return Err(code::VERIFY_CELL_CHANGED);
        }
        if !is_data {
            continue;
        }
        let pr = by_row[&ra.r];
        for col in COL_O..=COL_R {
            let cb = rb.cells.iter().find(|c| c.col == col).ok_or(code::VERIFY_TARGET_VALUE)?;
            let ca = ra.cells.iter().find(|c| c.col == col);
            // 서식은 원래 셀의 것 그대로(새로 넣은 셀이면 패치가 정한 것)
            if let Some(ca) = ca {
                if ca.attr("s") != cb.attr("s") {
                    return Err(code::VERIFY_TARGET_VALUE);
                }
                if xa[ca.start..ca.end] != xb[cb.start..cb.end] {
                    rep.target_cells_changed += 1;
                }
            } else {
                rep.target_cells_changed += 1;
            }
            let inner = cb.inner(xb);
            let ok = match col {
                COL_R => {
                    cb.attr("t") == Some("inlineStr")
                        && inner == format!("<is><t>{}</t></is>", plan.transfer_date)
                }
                _ => {
                    let want = [pr.o, pr.p, pr.q][(col - COL_O) as usize];
                    matches!(cb.attr("t"), None | Some("n")) && v_text(inner) == Some(&want.to_string()) && !has_child(inner, "f")
                }
            };
            if !ok {
                return Err(code::VERIFY_TARGET_VALUE);
            }
        }
    }

    // 3) 결제파일로 다시 읽기 — 구조 · 행 · A~N 값과 타입 · O/P/Q 숫자 · R 글자
    let file = dream_file::parse("결과.xlsx", out);
    if file.file_issues.iter().any(|i| i.severity == Severity::Error) || file.rows.iter().any(|r| r.has_error()) {
        return Err(code::VERIFY_PARSE_FAILED);
    }
    let got: Vec<u32> = file.rows.iter().map(|r| r.source_row).collect();
    if got != data.iter().copied().collect::<Vec<_>>() {
        return Err(code::VERIFY_ROWS_CHANGED);
    }
    for r in &file.rows {
        let pr = by_row[&r.source_row];
        let vals = (r.amount_n, r.personal_before, r.voucher_before, r.free_voucher_before);
        if vals != (Some(pr.amount_n), Some(pr.o), Some(pr.p), Some(pr.q))
            || r.transfer_date.as_deref() != Some(plan.transfer_date.as_str())
        {
            return Err(code::VERIFY_TARGET_VALUE);
        }
    }
    let ca = cells::read_sheet(src, SHEET).map_err(|_| code::PACKAGE_CORRUPT)?;
    let cb = cells::read_sheet(out, SHEET).map_err(|_| code::VERIFY_PARSE_FAILED)?;
    let get = |s: &cells::SheetCells, r: usize, c: usize| s.rows.get(r).and_then(|x| x.get(c)).cloned().unwrap_or(Cell::Empty);
    let height = ca.rows.len().max(cb.rows.len());
    for r in 0..height {
        let is_data = data.contains(&(r as u32 + 1));
        for c in 0..18 {
            if is_data && c >= 14 {
                continue; // O~R 은 위에서 계획과 견줬다
            }
            if get(&ca, r, c) != get(&cb, r, c) {
                rep.a_to_n_changed += 1;
            }
        }
    }
    if rep.a_to_n_changed > 0 {
        return Err(code::VERIFY_A_TO_N_CHANGED);
    }
    Ok(rep)
}

#[cfg(test)]
#[path = "dream_patch_tests.rs"]
mod tests;
