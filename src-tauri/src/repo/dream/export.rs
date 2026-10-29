//! 방과후드림 결과 파일 만들기 (P5).
//!
//! **돈을 계산하지 않는다.** P4 가 저장하고 검증한 배분 결과(`dream_job_row.after_*`)를 행 번호
//! 그대로 원본 XLSX 의 O · P · Q 에 옮기고, 사람이 고른 이체예정일을 R 에 적을 뿐이다.
//!
//! ```text
//! prepare       이체예정일 확인 → allocation::require_valid(파일 지문 · 정산 FRESH · P3 전체 ·
//!               정산 전체성 · 우선순위 · 지문 · 저장된 after) → 행마다 적을 값(PatchPlan)
//!               → 줄 · 학생 · 파일 전체 검산(다시 계산이 아니라 저장 전 확인)
//! write_result  원본과 같은 파일이면 거절 → 메모리에서 patch → 재검증 → 같은 폴더 임시 파일
//!               → 디스크에서 다시 읽어 재검증 → 원본이 그대로인지 확인 → 최종 이름으로 옮김
//! record        모두 성공한 뒤에만 RESULT_EXPORTED(날짜 · 파일 이름 · SHA-256 · 줄 수) 기록
//! ```
//!
//! 파일 쓰기와 DB 트랜잭션은 하나로 묶을 수 없다. 그래서 기록은 파일이 확정된 **뒤에** 남기고,
//! 기록만 실패하면 파일은 지우지 않고 그 사실을 그대로 알린다. 전체 경로는 적지 않는다.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::crypto;
use crate::domain::dream as rules;
use crate::domain::dream_alloc::Totals;
use crate::error::{AppError, AppResult};
use crate::excel::dream_patch::{self, PatchPlan, PatchReport, PatchRow, VerifyReport};
use crate::repo::dream::{allocation, job, session};

/// 결과 파일에 적을 것과, 화면에 보일 합계.
#[derive(Debug, Clone)]
pub struct ExportPlan {
    pub plan: PatchPlan,
    /// 결제파일 읽기가 정한 실제 결제행
    pub data_rows: Vec<u32>,
    pub totals: Totals,
    pub students: usize,
    pub source_sha256: String,
    /// 이 결과가 다룬 방과후드림 적용 학년(P6)
    pub applied_grades: Vec<i64>,
}

fn invalid_date() -> AppError {
    AppError::new(
        "INVALID_TRANSFER_DATE",
        "이체예정일을 실제 날짜로 골라 주세요. 결과 파일을 만들려면 이체예정일이 꼭 있어야 합니다.",
    )
}

fn inconsistent(what: &str) -> AppError {
    AppError::new(
        "EXPORT_INCONSISTENT",
        "배분 결과가 현재 자료와 일치하지 않습니다. 다시 검증 · 계산한 뒤 결과 파일을 만들어 주세요.",
    )
    .detail(what.to_string())
}

/// 결과 파일을 만들 준비 — 읽기만 한다.
///
/// `transfer_date` 는 `YYYYMMDD`(화면이 `YYYY-MM-DD` 에서 바꿔 보낸다). 배분 결과가 지금
/// 자료와 조금이라도 다르면(`require_valid`) 만들지 않는다.
pub fn prepare(conn: &Connection, job_id: i64, bytes: &[u8], transfer_date: &str) -> AppResult<ExportPlan> {
    let date = rules::check_transfer_date(transfer_date).map_err(|_| invalid_date())?;
    if date != transfer_date {
        return Err(invalid_date());
    }
    // P4 최종 확인 — 파일 지문 · 정산 · P3 전체(취소 학생 포함 전체성) · 우선순위 · 지문 · after
    let a = allocation::require_valid(conn, job_id, bytes)?;
    let head = job::get(conn, job_id)?;
    let file = session::parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let data_rows: Vec<u32> = file.rows.iter().map(|r| r.source_row).collect();

    // 적을 값의 근거는 저장된 after 다(require_valid 가 지금 다시 계산한 것과 같음을 확인했다)
    let stored = job::stored_after(conn, job_id)?;
    let mut rows = Vec::with_capacity(stored.len());
    for (row, after) in &stored {
        let Some((o, p, q)) = after else { return Err(inconsistent("AFTER_MISSING")) };
        let n = a
            .students
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.source_row == *row)
            .map(|l| l.amount_n)
            .ok_or_else(|| inconsistent("ROW_NOT_ALLOCATED"))?;
        if *o < 0 || *p < 0 || *q < 0 || o + p + q != n {
            return Err(inconsistent("ROW_SUM"));
        }
        rows.push(PatchRow {
            source_row: *row,
            amount_n: n,
            o: *o,
            p: *p,
            q: *q,
        });
    }
    if rows.len() != data_rows.len() || rows.iter().map(|r| r.source_row).ne(data_rows.iter().copied()) {
        return Err(inconsistent("ROW_SET"));
    }
    // 학생마다 · 파일 전체 — 저장된 값의 합이 정산 기준액과 같은가
    let by_row: std::collections::HashMap<u32, &PatchRow> = rows.iter().map(|r| (r.source_row, r)).collect();
    let mut totals = Totals::default();
    for s in &a.students {
        let mut t = Totals::default();
        for l in &s.lines {
            let r = by_row[&l.source_row];
            t.add(r.amount_n, r.o, r.p, r.q);
        }
        let x = &s.amount;
        if (t.n, t.o, t.p, t.q) != (x.total, x.personal, x.voucher, x.free_voucher) {
            return Err(inconsistent("STUDENT_SUM"));
        }
        totals.add(t.n, t.o, t.p, t.q);
    }
    if totals != a.settlement || totals.n != totals.o + totals.p + totals.q {
        return Err(inconsistent("FILE_SUM"));
    }
    Ok(ExportPlan {
        plan: PatchPlan {
            rows,
            transfer_date: date,
        },
        data_rows,
        totals,
        students: a.students.len(),
        source_sha256: head.source_sha256,
        applied_grades: a.applied_grades.clone(),
    })
}

/// 만든 결과.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// 저장한 파일 이름(경로 없음)
    pub file_name: String,
    /// 화면의 [폴더 열기] 용 — DB · 로그에는 적지 않는다
    pub path: String,
    pub sha256: String,
    pub patch: PatchReport,
    pub verify: VerifyReport,
}

fn patch_error(c: &'static str) -> AppError {
    let msg = match c {
        dream_patch::code::TARGET_CELL_HAS_FORMULA => "결과를 적을 칸(O~R)에 수식이 있어 만들지 않았습니다. 원본 파일을 확인해 주세요.",
        dream_patch::code::TARGET_CELL_MERGED => "결과를 적을 칸(O~R)이 병합되어 있어 만들지 않았습니다. 원본 파일을 확인해 주세요.",
        dream_patch::code::TARGET_SHEET_NOT_FOUND => "원본 파일에서 '정산내역' 시트를 정확히 찾지 못해 만들지 않았습니다.",
        dream_patch::code::MISSING_CELL_STYLE_UNKNOWN => "결과를 적을 칸이 비어 있고 서식을 정할 수 없어 만들지 않았습니다.",
        c if c.starts_with("VERIFY_") => "만든 결과 파일을 다시 확인하는 중 원본과 맞지 않는 곳이 있어 저장하지 않았습니다.",
        _ => "결과 파일을 만들지 못했습니다. 원본 파일 구조가 예상과 다릅니다.",
    };
    AppError::new(c, msg)
}

/// 비교용 경로 — 실제 파일이면 그 정식 경로, 아직 없으면 폴더의 정식 경로 + 이름. Windows 는
/// 대소문자를 가리지 않는다.
fn canonical(p: &Path) -> AppResult<String> {
    let full = match p.canonicalize() {
        Ok(c) => c,
        Err(_) => {
            let parent = p.parent().filter(|x| !x.as_os_str().is_empty()).unwrap_or(Path::new("."));
            let dir = parent
                .canonicalize()
                .map_err(|_| AppError::invalid("결과 파일을 저장할 폴더를 찾지 못했습니다."))?;
            dir.join(p.file_name().ok_or_else(|| AppError::invalid("결과 파일 이름이 없습니다."))?)
        }
    };
    Ok(full.to_string_lossy().to_lowercase())
}

/// 실패하면 지워지는 임시 파일.
struct Temp(Option<PathBuf>);

impl Drop for Temp {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn io_error(what: &str, e: &std::io::Error) -> AppError {
    AppError::new("EXPORT_WRITE_FAILED", format!("{what} 다른 프로그램에서 그 파일을 열고 있지 않은지, 쓸 수 있는 폴더인지 확인해 주세요."))
        .detail(e.kind().to_string())
}

/// 결과 파일을 쓴다 — 원본은 읽기만 한다. `make` 는 patch 함수(시험에서 잘못된 결과를 흉내 낼 때
/// 바꿔 넣는다). 재검증은 언제나 `dream_patch::verify` 가 한다.
pub fn write_result(
    source_path: &Path,
    out_path: &Path,
    source_bytes: &[u8],
    ep: &ExportPlan,
    make: impl Fn(&[u8], &PatchPlan, &[u32]) -> Result<(Vec<u8>, PatchReport), &'static str>,
) -> AppResult<Written> {
    let file_name = out_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| n.to_lowercase().ends_with(".xlsx"))
        .ok_or_else(|| AppError::invalid("결과 파일은 .xlsx 이름으로 저장해 주세요."))?;
    if out_path.is_dir() {
        return Err(AppError::invalid("결과 파일 이름을 골라 주세요."));
    }
    // 원본 덮어쓰기 금지 — 정식 경로(대소문자 무시)로 견준다
    if canonical(source_path)? == canonical(out_path)? {
        return Err(AppError::new(
            "SAME_AS_SOURCE",
            "원본 결제파일 자리에는 저장할 수 없습니다. 다른 이름이나 다른 폴더를 골라 주세요.",
        ));
    }
    if crypto::sha256_hex(source_bytes) != ep.source_sha256 {
        return Err(AppError::new("FILE_CHANGED", "결제파일이 처음 읽은 뒤에 바뀌었습니다. 파일을 다시 골라 검사부터 해 주세요."));
    }

    // 1) 메모리에서 만들고 원본과 맞대어 본다
    let (out, patch) = make(source_bytes, &ep.plan, &ep.data_rows).map_err(patch_error)?;
    dream_patch::verify(source_bytes, &out, &ep.plan, &ep.data_rows).map_err(patch_error)?;

    // 2) 같은 폴더의 임시 파일 — 실패하면 지운다
    let dir = out_path.parent().filter(|x| !x.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if !dir.is_dir() {
        return Err(AppError::invalid("결과 파일을 저장할 폴더를 찾지 못했습니다."));
    }
    let tmp_path = dir.join(format!("~방과후드림_만드는중_{}.tmp", uuid::Uuid::new_v4().simple()));
    let mut temp = Temp(Some(tmp_path.clone()));
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| io_error("결과 파일을 만들 수 없습니다.", &e))?;
        f.write_all(&out).map_err(|e| io_error("결과 파일을 쓰지 못했습니다.", &e))?;
        f.sync_all().map_err(|e| io_error("결과 파일을 쓰지 못했습니다.", &e))?;
    }
    // 3) 디스크에 쓴 것을 다시 읽어 한 번 더 — "만들어졌다"만으로 성공 처리하지 않는다
    let disk = std::fs::read(&tmp_path).map_err(|e| io_error("만든 결과 파일을 다시 읽지 못했습니다.", &e))?;
    if disk != out {
        return Err(patch_error(dream_patch::code::VERIFY_PARSE_FAILED));
    }
    let verify = dream_patch::verify(source_bytes, &disk, &ep.plan, &ep.data_rows).map_err(patch_error)?;
    // 4) 원본은 그대로인가
    let again = std::fs::read(source_path).map_err(|e| io_error("원본 결제파일을 다시 읽지 못했습니다.", &e))?;
    if crypto::sha256_hex(&again) != ep.source_sha256 {
        return Err(AppError::new("FILE_CHANGED", "결과를 만드는 동안 원본 결제파일이 바뀌었습니다. 저장하지 않았습니다."));
    }
    // 5) 최종 이름으로 — 같은 폴더 안이라 옮기기만 한다(이미 있으면 저장 창에서 사람이 덮어쓰기를 골랐다)
    std::fs::rename(&tmp_path, out_path).map_err(|e| io_error("결과 파일을 저장하지 못했습니다.", &e))?;
    temp.0 = None;
    let fin = std::fs::read(out_path).map_err(|e| io_error("저장한 결과 파일을 다시 읽지 못했습니다.", &e))?;
    let sha = crypto::sha256_hex(&fin);
    if sha != crypto::sha256_hex(&out) {
        let _ = std::fs::remove_file(out_path);
        return Err(patch_error(dream_patch::code::VERIFY_PARSE_FAILED));
    }
    Ok(Written {
        file_name,
        path: out_path.to_string_lossy().to_string(),
        sha256: sha,
        patch,
        verify,
    })
}

/// 성공 기록 — 파일을 만들고 다시 확인까지 끝난 뒤에만. 경로 · 개인정보는 적지 않는다.
pub fn record(conn: &Connection, job_id: i64, w: &Written, ep: &ExportPlan) -> AppResult<()> {
    if w.file_name.contains(['/', '\\', ':']) {
        return Err(AppError::invalid("파일 이름만 적습니다."));
    }
    job::add_event(
        conn,
        job_id,
        "RESULT_EXPORTED",
        &format!(
            "date={} sha={} rows={} n={} o={} p={} q={} grades={} file={}",
            ep.plan.transfer_date,
            w.sha256,
            ep.plan.rows.len(),
            ep.totals.n,
            ep.totals.o,
            ep.totals.p,
            ep.totals.q,
            ep.applied_grades.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(","),
            w.file_name
        ),
    )
}
