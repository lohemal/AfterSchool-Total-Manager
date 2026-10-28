//! 방과후드림 매칭 · 검증 작업(처리 이력 한 건)의 흐름 (P3).
//!
//! ```text
//! 시작      파일 지문 = 미리보기 때 지문 · 파일 검사 통과 · 정산 FRESH · 키 사용 가능
//!           → dream_job 생성(정산 id · 자료판 기록) → 매칭 → 줄 기록
//! 다시 검증 파일 지문 = 작업 지문 · 정산이 작업을 만들 때 그대로 → 매칭 → 줄 기록 새로
//! 확인     (개인참조번호 등록 · 생년월일 등록 · 강좌코드 연결) 뒤에는 학생 전체를 다시 검증
//! ```
//!
//! 파일 내용은 매번 고른 자리에서 다시 읽고, 그 값으로만 등록한다(화면이 보낸 번호를
//! 믿지 않는다 — 화면은 가린 값만 가지고 있다). 정산 자료(학생 · 수강 · Charge · 정산)는
//! 고치지 않는다.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::crypto::{self, keys};
use crate::domain::dream_file::DreamFile;
use crate::error::{AppError, AppResult};
use crate::excel::dream_file;
use crate::repo::dream::matching::{self, Matching, State};
use crate::repo::dream::reset::{self, KeyStatus};
use crate::repo::dream::{amount, course, file_check, identity, job};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub job_id: i64,
    /// `OPEN`(확인 필요) | `BLOCKED`(오류 있음) | `READY`(모두 준비 완료) | `FAILED`
    pub job_status: String,
    pub matching: Matching,
}

fn file_changed() -> AppError {
    AppError::new(
        "FILE_CHANGED",
        "결제파일이 처음 읽은 뒤에 바뀌었습니다. 파일을 다시 골라 검사부터 해 주세요.",
    )
}

/// 파일을 읽고 P2 검사를 다시 통과하는지 본다.
pub(crate) fn parse_ok(conn: &Connection, ws: i64, file_name: &str, bytes: &[u8]) -> AppResult<DreamFile> {
    let c = file_check::check(conn, ws, file_name, bytes)?;
    if c.status == "ERROR" {
        return Err(AppError::invalid(
            "결제파일 검사를 통과하지 못했습니다. 검증 요약의 오류를 먼저 해결해 주세요.",
        ));
    }
    Ok(dream_file::parse(file_name, bytes))
}

pub(crate) fn usable_key(conn: &Connection) -> AppResult<Option<crypto::DataKey>> {
    match reset::key_status(conn)? {
        KeyStatus::KeyUnavailable => Err(AppError::new(
            "KEY_UNAVAILABLE",
            "이 PC에서는 저장된 방과후드림 개인참조번호를 열 수 없습니다. [개인정보 초기화] 후 다시 등록해 주세요.",
        )),
        KeyStatus::NotInitialized => Ok(None),
        KeyStatus::Ready => keys::current(conn),
    }
}

pub(crate) fn job_status_of(m: &Matching) -> &'static str {
    if m.p4_ready {
        "READY"
    } else if m.errors > 0 || m.file_reasons.iter().any(|r| r.state == State::Error) {
        "BLOCKED"
    } else {
        "OPEN"
    }
}

/// 매칭 결과를 줄 기록으로 남긴다 — 내부 id · 행 번호 · 금액 · 상태 · 코드만.
///
/// 배분 결과(`after`, 행 번호 → O · P · Q)는 P4 배분이 검산까지 마친 때만 함께 적는다.
/// 그 밖에는 언제나 비운다 — 다시 검증하면 이전 배분 결과는 남지 않는다.
pub(crate) fn record(
    conn: &Connection,
    job_id: i64,
    file: &DreamFile,
    m: &Matching,
    after: Option<&HashMap<u32, (i64, i64, i64)>>,
) -> AppResult<()> {
    job::clear_rows(conn, job_id)?;
    let by_row: HashMap<u32, &crate::domain::dream_file::DreamRow> = file.rows.iter().map(|r| (r.source_row, r)).collect();
    for s in &m.students {
        let status = match s.state {
            State::Ready => "MATCHED",
            State::NeedsConfirmation => "NEEDS_REVIEW",
            State::Error => "ERROR",
        };
        // 개인참조번호로 확정된 학생만 id 를 적는다(후보는 적지 않는다)
        let sid = if s.ref_state == "MATCHED" { s.student_id } else { None };
        for l in &s.lines {
            let r = by_row.get(&l.source_row);
            let reason = l
                .reasons
                .first()
                .or_else(|| s.reasons.iter().find(|x| x.state != State::Ready))
                .map(|x| x.code)
                .unwrap_or("");
            job::add_row(
                conn,
                job_id,
                &job::RowRecord {
                    source_row: l.source_row as i64,
                    student_id: sid,
                    department_id: l.department_id,
                    dream_item_code: r.map(|x| x.dream_item_code.clone()).unwrap_or_default(),
                    item_code: l.item_code.map(str::to_string),
                    amount_n: Some(l.amount_n),
                    before: r.and_then(|x| match (x.personal_before, x.voucher_before, x.free_voucher_before) {
                        (Some(o), Some(p), Some(q)) => Some((o, p, q)),
                        _ => None,
                    }),
                    after: after.and_then(|a| a.get(&l.source_row).copied()),
                    match_status: status.to_string(),
                    reason_code: reason.to_string(),
                },
            )?;
        }
    }
    Ok(())
}

fn finish(conn: &Connection, job_id: i64, file: &DreamFile, m: Matching) -> AppResult<Session> {
    record(conn, job_id, file, &m, None)?;
    let st = job_status_of(&m);
    job::set_status(conn, job_id, st, None)?;
    job::add_event(
        conn,
        job_id,
        "VALIDATE",
        &format!(
            "students={} ready={} confirm={} error={} p4={}",
            m.students.len(),
            m.ready,
            m.needs_confirmation,
            m.errors,
            m.p4_ready
        ),
    )?;
    Ok(Session {
        job_id,
        job_status: st.to_string(),
        matching: m,
    })
}

/// [매칭 및 검증 시작]. 쓰기 트랜잭션 안에서 부른다.
pub fn start(
    conn: &Connection,
    workspace_id: i64,
    file_name: &str,
    bytes: &[u8],
    expected_sha256: &str,
    app_version: &str,
) -> AppResult<Session> {
    let sha = crypto::sha256_hex(bytes);
    if sha != expected_sha256.to_ascii_lowercase() {
        return Err(file_changed());
    }
    let file = parse_ok(conn, workspace_id, file_name, bytes)?;
    let key = usable_key(conn)?;
    let fresh = amount::fresh_settlement(conn, workspace_id)?; // NONE · STALE_* 는 여기서 거절
    let job_id = job::create(
        conn,
        &job::NewJob {
            workspace_id,
            source_name: file_name,
            source_sha256: &sha,
            source_size: bytes.len() as i64,
            settlement: Some(&fresh),
            app_version,
        },
    )?;
    job::add_event(conn, job_id, "START", &format!("rows={}", file.rows.len()))?;
    let m = matching::run(conn, workspace_id, &file, key.as_ref())?;
    finish(conn, job_id, &file, m)
}

/// 작업이 아직 쓸 수 있는가 — 파일 · 정산이 작업을 만들 때 그대로인가.
/// 어긋나면 작업을 FAILED 로 닫고 오류를 돌려준다(호출하는 쪽이 커밋해야 기록이 남는다).
pub(crate) fn check_job(conn: &Connection, job_id: i64, bytes: &[u8]) -> AppResult<job::JobHead> {
    let head = job::get(conn, job_id)?;
    if !matches!(head.status.as_str(), "OPEN" | "BLOCKED" | "READY") {
        return Err(AppError::invalid("끝난 작업입니다. 매칭 및 검증을 다시 시작해 주세요."));
    }
    if crypto::sha256_hex(bytes) != head.source_sha256 {
        return Err(file_changed());
    }
    let now = match amount::fresh_settlement(conn, head.workspace_id) {
        Ok(f) => f,
        Err(e) => return Err(AppError::new("SETTLEMENT_NOT_FRESH", e.message)),
    };
    if Some(now.settlement_id) != head.settlement_id
        || Some(now.ws_data_version) != head.ws_data_version
        || Some(now.year_data_version) != head.year_data_version
    {
        return Err(AppError::new(
            "SETTLEMENT_CHANGED",
            "이 작업을 시작한 뒤 정산이 다시 만들어졌거나 바뀌었습니다. 매칭 및 검증을 다시 시작해 주세요.",
        ));
    }
    Ok(head)
}

/// 작업을 실패로 닫는다(파일이 바뀌었거나 정산이 바뀌었을 때).
pub fn fail(conn: &Connection, job_id: i64, why: &str) -> AppResult<()> {
    job::set_status(conn, job_id, "FAILED", None)?;
    job::add_event(conn, job_id, "FAILED", why)
}

/// 다시 검증 — 원본에서 다시 계산한다.
pub fn revalidate(conn: &Connection, job_id: i64, bytes: &[u8]) -> AppResult<Session> {
    let head = check_job(conn, job_id, bytes)?;
    let file = parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let key = usable_key(conn)?;
    let m = matching::run(conn, head.workspace_id, &file, key.as_ref())?;
    finish(conn, job_id, &file, m)
}

/// 개인참조번호 확인 · 등록 — `(학생 열쇠(첫 행 번호), 학생 id)` 여럿을 한꺼번에.
///
/// 서버가 후보를 다시 계산해, 그 학생이 **그 파일 학생의 정확한 후보**일 때만 등록한다.
/// 번호 · 생년월일 값은 파일에서 읽은 것만 쓴다. 하나라도 안 되면 아무것도 등록하지 않는다.
pub fn confirm_identities(conn: &Connection, job_id: i64, bytes: &[u8], picks: &[(u32, i64)]) -> AppResult<usize> {
    let head = check_job(conn, job_id, bytes)?;
    let file = parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let key0 = usable_key(conn)?;
    let m = matching::run(conn, head.workspace_id, &file, key0.as_ref())?;
    let key = keys::current_or_create(conn)?;
    for (k, sid) in picks {
        let s = m
            .students
            .iter()
            .find(|s| s.key == *k)
            .ok_or_else(|| AppError::invalid("파일에서 그 학생을 찾지 못했습니다. 다시 검증해 주세요."))?;
        let ok = s.ref_state == "NOT_REGISTERED"
            && s.candidates.iter().any(|c| c.student_id == *sid)
            && !s.reasons.iter().any(|r| r.code == matching::code::STUDENT_INFO_INCONSISTENT);
        if !ok {
            return Err(AppError::invalid(
                "확인할 수 없는 학생입니다(정확한 후보가 아니거나 이미 등록됨). 다시 검증해 주세요.",
            ));
        }
        let row = file.rows.iter().find(|r| r.source_row == *k).expect("열쇠는 파일 행");
        let r = row.personal_ref.as_ref().expect("파일 검사를 통과했다").expose();
        let b = row.birth.as_ref().expect("파일 검사를 통과했다").expose();
        identity::set_ref(conn, &key, *sid, r)?;
        match identity::reveal_birth(conn, &key, *sid)? {
            Some(stored) if !crate::domain::dream::birth_matches(&stored, b) => {
                return Err(AppError::invalid("이미 저장된 생년월일이 파일과 다릅니다. 등록하지 않았습니다."));
            }
            Some(_) => {}
            None => identity::set_birth(conn, &key, *sid, b)?,
        }
    }
    job::add_event(conn, job_id, "CONFIRM_IDENTITY", &format!("count={}", picks.len()))?;
    Ok(picks.len())
}

/// 생년월일 등록 — 개인참조번호로 확정됐고 생년월일만 비어 있는 학생.
pub fn confirm_births(conn: &Connection, job_id: i64, bytes: &[u8], keys_: &[u32]) -> AppResult<usize> {
    let head = check_job(conn, job_id, bytes)?;
    let file = parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    let key = usable_key(conn)?.ok_or_else(|| AppError::invalid("등록된 개인참조번호가 없습니다."))?;
    let m = matching::run(conn, head.workspace_id, &file, Some(&key))?;
    for k in keys_ {
        let s = m
            .students
            .iter()
            .find(|s| s.key == *k)
            .ok_or_else(|| AppError::invalid("파일에서 그 학생을 찾지 못했습니다."))?;
        let sid = match (s.ref_state, s.birth_state, s.student_id) {
            ("MATCHED", "NOT_REGISTERED", Some(id)) => id,
            _ => return Err(AppError::invalid("생년월일을 등록할 수 있는 학생이 아닙니다.")),
        };
        // 학생 정보 교차검증에 실패한 학생에게는 등록하지 않는다
        if s.reasons.iter().any(|r| r.state == State::Error && r.code.starts_with("STUDENT_")) {
            return Err(AppError::invalid("학생 정보가 맞지 않는 학생에게는 생년월일을 등록하지 않습니다."));
        }
        let row = file.rows.iter().find(|r| r.source_row == *k).expect("열쇠는 파일 행");
        identity::set_birth(conn, &key, sid, row.birth.as_ref().expect("검사 통과").expose())?;
    }
    job::add_event(conn, job_id, "CONFIRM_BIRTH", &format!("count={}", keys_.len()))?;
    Ok(keys_.len())
}

/// 강좌코드 연결 확정 — 파일에 있는 코드를 이 작업공간 부서에 잇는다(1:1 은 DB 가 지킨다).
pub fn confirm_course(conn: &Connection, job_id: i64, bytes: &[u8], course_code: &str, department_id: i64) -> AppResult<()> {
    let head = check_job(conn, job_id, bytes)?;
    let file = parse_ok(conn, head.workspace_id, &head.source_name, bytes)?;
    if !file.rows.iter().any(|r| r.course_code.as_deref() == Some(course_code)) {
        return Err(AppError::invalid("이 파일에 없는 강좌명코드입니다."));
    }
    let ws: i64 = conn.query_row(
        "SELECT workspace_id FROM department WHERE id = ?1",
        [department_id],
        |r| r.get(0),
    )?;
    if ws != head.workspace_id {
        return Err(AppError::invalid("이 작업공간의 부서가 아닙니다."));
    }
    // 이미 다른 강좌코드에 이어진 부서를 이 확정으로 옮기지 않는다(덮어쓰기 금지)
    if let Some(other) = course::list(conn, ws)?
        .into_iter()
        .find(|m| m.department_id == department_id && m.course_code != course_code)
    {
        return Err(AppError::new(
            "CONFLICT",
            format!("이 부서는 이미 다른 강좌명코드({})에 연결되어 있습니다.", other.course_code),
        ));
    }
    course::set(conn, department_id, course_code)?;
    job::add_event(conn, job_id, "CONFIRM_COURSE", course_code)?;
    Ok(())
}
