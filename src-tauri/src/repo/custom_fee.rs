//! 맞춤형 프로그램 강사료 (v0.1.8, 설계안 30장).
//!
//! ## 학생 수강료와 다른 돈이다
//!
//! 여기서 내는 금액은 **학교가 강사에게 주는 돈**이다. 학생이 내는 수강료
//! (`charge`)와 그 정산·지원금에는 한 푼도 섞이지 않는다. 두 회계를 섞으면
//! 수익자 명세와 지출 명세가 같은 숫자를 두고 다투게 된다.
//!
//! ## 날짜를 미리 적어 두지 않는다
//!
//! 예정 수업일은 `작업공간 기간 + 부서 요일` 에서 늘 다시 센다. DB 에는
//! **사람이 손댄 예외만** 남는다(`dept_session`, `workspace_closure`).
//! 날짜를 통째로 복제해 두면 기간이나 요일을 고친 날 적어 둔 것과 실제가
//! 어긋나고, 어느 쪽이 맞는지 알 수 없게 된다.
//!
//! ## 셈 차례 (설계안 30-6)
//!
//! ```text
//! 1. 기간 + 요일            → 예정일
//! 2. 작업공간 공통 휴업일    → 그날은 0차시
//! 3. 부서 예외(dept_session) → 그 날짜의 **최종 결정**. 공통 휴업일도 덮는다
//! 4. 예정일이 아닌 날의 예외 → 보강·추가 수업
//! ```
//!
//! 3번이 2번을 덮는 까닭: 학교는 쉬었지만 그 반만 수업한 날이 실제로 있다.
//! 사람이 그 날짜를 직접 적어 넣은 것이므로 자동 규칙보다 앞선다.
//!
//! ## 질의 횟수
//!
//! 부서 묶음 한 번 · 휴업일 한 번 · 예외 한 번. 부서가 몇이든 **세 번**이다.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rusqlite::{params, Connection};

use crate::domain::school_date::{dates_on_weekdays, fmt, parse, weekday_label};
use crate::domain::parse_days;
use crate::error::{AppError, AppResult};
use crate::model::{
    Closure, CustomFeeReport, CustomFeeRow, CustomFeeSummary, OrphanSession, SessionDay,
    SessionKind,
};
use crate::repo;

/// 사람이 손댄 날짜 한 줄.
#[derive(Debug, Clone)]
struct Exception {
    sessions: i64,
    reason: String,
}

/// 한 부서를 셈하는 데 필요한 것만 담는다.
struct Dept {
    id: i64,
    name: String,
    class_name: String,
    teacher: String,
    days: String,
    base_sessions: i64,
}

fn load_departments(conn: &Connection, workspace_id: i64, all: bool) -> AppResult<Vec<Dept>> {
    let sql = if all {
        "SELECT id, name, class_name, teacher, days, custom_sessions
           FROM department WHERE workspace_id = ?1 ORDER BY name, class_name"
    } else {
        "SELECT id, name, class_name, teacher, days, custom_sessions
           FROM department WHERE workspace_id = ?1 AND custom_sessions > 0
          ORDER BY name, class_name"
    };
    let mut st = conn.prepare(sql)?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok(Dept {
                id: r.get(0)?,
                name: r.get(1)?,
                class_name: r.get(2)?,
                teacher: r.get(3)?,
                days: r.get(4)?,
                base_sessions: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 작업공간 공통 휴업일 목록.
pub fn closures(conn: &Connection, workspace_id: i64) -> AppResult<Vec<Closure>> {
    let mut st = conn.prepare(
        "SELECT id, on_date, reason FROM workspace_closure
          WHERE workspace_id = ?1 ORDER BY on_date",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, date, reason)| {
            let weekday = parse(&date).map(weekday_label).unwrap_or("").to_string();
            Closure {
                id,
                date,
                weekday,
                reason,
            }
        })
        .collect())
}

/// 이 작업공간 부서들의 예외를 한 번에 읽는다.
fn load_exceptions(
    conn: &Connection,
    workspace_id: i64,
) -> AppResult<HashMap<i64, HashMap<String, Exception>>> {
    let mut st = conn.prepare(
        "SELECT s.department_id, s.on_date, s.sessions, s.reason
           FROM dept_session s
           JOIN department d ON d.id = s.department_id
          WHERE d.workspace_id = ?1
          ORDER BY s.department_id, s.on_date",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out: HashMap<i64, HashMap<String, Exception>> = HashMap::new();
    for (dept, date, sessions, reason) in rows {
        out.entry(dept)
            .or_default()
            .insert(date, Exception { sessions, reason });
    }
    Ok(out)
}

/// 부서 한 줄을 셈한다. 날짜 차례는 늘 이른 것부터.
fn reckon(
    d: &Dept,
    start: NaiveDate,
    end: NaiveDate,
    closed: &HashMap<String, String>,
    exceptions: &HashMap<String, Exception>,
    rate: i64,
) -> CustomFeeRow {
    let day_names = parse_days(&d.days);
    let planned: Vec<NaiveDate> = dates_on_weekdays(start, end, &day_names);
    let planned_set: HashSet<String> = planned.iter().map(|x| fmt(*x)).collect();

    let mut list: Vec<SessionDay> = Vec::new();

    // 1~3. 예정일을 훑는다
    for date in &planned {
        let key = fmt(*date);
        let closure_reason = closed.get(&key);
        let (sessions, reason, overridden) = match exceptions.get(&key) {
            // 사람이 적어 둔 줄이 그 날짜의 최종 결정이다
            Some(e) => (e.sessions, e.reason.clone(), true),
            None => match closure_reason {
                Some(why) => (0, why.clone(), false),
                None => (d.base_sessions, String::new(), false),
            },
        };
        list.push(SessionDay {
            weekday: weekday_label(*date).to_string(),
            kind: if sessions > 0 {
                SessionKind::Normal
            } else {
                SessionKind::Closed
            },
            date: key.clone(),
            sessions,
            reason,
            overridden,
            from_closure: exceptions.get(&key).is_none() && closure_reason.is_some(),
        });
    }

    // 4. 예정일이 아닌 날의 예외 — 보강·추가
    for (key, e) in exceptions {
        if planned_set.contains(key) {
            continue;
        }
        let Ok(date) = parse(key) else { continue };
        if date < start || date > end {
            continue; // 기간 밖은 고아로 따로 알린다
        }
        if e.sessions == 0 {
            continue; // 예정일도 아닌데 0차시 — 효력이 없다. 고아로 알린다
        }
        list.push(SessionDay {
            date: key.clone(),
            weekday: weekday_label(date).to_string(),
            kind: SessionKind::Extra,
            sessions: e.sessions,
            reason: e.reason.clone(),
            overridden: true,
            from_closure: false,
        });
    }

    list.sort_by(|a, b| a.date.cmp(&b.date));

    let planned_days = planned.len() as i64;
    let closed_days = list
        .iter()
        .filter(|s| s.kind == SessionKind::Closed)
        .count() as i64;
    let extra_days = list.iter().filter(|s| s.kind == SessionKind::Extra).count() as i64;
    let counted_days = list.iter().filter(|s| s.sessions > 0).count() as i64;
    let total_sessions: i64 = list.iter().map(|s| s.sessions).sum();

    CustomFeeRow {
        department_id: d.id,
        name: d.name.clone(),
        class_name: d.class_name.clone(),
        teacher: d.teacher.clone(),
        days: d.days.clone(),
        base_sessions: d.base_sessions,
        planned_days,
        closed_days,
        extra_days,
        counted_days,
        total_sessions,
        rate,
        fee: total_sessions * rate,
        sessions_list: list,
    }
}

/// 기본 예정일과 이어지지 않는 예외를 찾는다. **지우지 않고 알리기만 한다.**
///
/// 작업공간 기간이나 부서 요일을 바꾸면 예전에 적어 둔 조정이 허공에 뜬다.
/// 조용히 지우면 담당자가 적어 둔 사유까지 사라지므로, 눈에 보이게만 한다.
fn find_orphans(
    depts: &[Dept],
    start: NaiveDate,
    end: NaiveDate,
    exceptions: &HashMap<i64, HashMap<String, Exception>>,
) -> Vec<OrphanSession> {
    let mut out = Vec::new();
    for d in depts {
        let Some(ex) = exceptions.get(&d.id) else {
            continue;
        };
        let day_names = parse_days(&d.days);
        let planned: HashSet<String> = dates_on_weekdays(start, end, &day_names)
            .iter()
            .map(|x| fmt(*x))
            .collect();
        for (date, e) in ex {
            let parsed = parse(date).ok();
            let why = match parsed {
                None => Some("날짜를 읽을 수 없습니다".to_string()),
                Some(x) if x < start || x > end => {
                    Some("작업공간 기간 밖의 날짜입니다".to_string())
                }
                Some(_) if !planned.contains(date) && e.sessions == 0 => {
                    Some("운영 요일이 아닌 날의 0차시 기록이라 강사료에 영향이 없습니다".into())
                }
                _ => None,
            };
            if let Some(why) = why {
                out.push(OrphanSession {
                    department_id: d.id,
                    department_label: format!("{}{}", d.name, d.class_name),
                    date: date.clone(),
                    sessions: e.sessions,
                    reason: e.reason.clone(),
                    why,
                });
            }
        }
    }
    out.sort_by_key(|o| (o.department_label.clone(), o.date.clone()));
    out
}

/// 학년도의 차시당 단가.
pub fn rate_of_year(conn: &Connection, year_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT custom_session_rate FROM academic_year WHERE id = ?1",
        params![year_id],
        |r| r.get(0),
    )?)
}

/// 화면과 Excel 이 함께 쓰는 한 벌. **둘 다 이 값을 그대로 적는다.**
pub fn report(conn: &Connection, workspace_id: i64, include_all: bool) -> AppResult<CustomFeeReport> {
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    let year = repo::year::get_year(conn, ws.year_id)?;
    let rate = rate_of_year(conn, ws.year_id)?;

    let start = parse(&ws.start_date)?;
    let end = parse(&ws.end_date)?;

    let depts = load_departments(conn, workspace_id, include_all)?;
    let closure_rows = closures(conn, workspace_id)?;
    let closed: HashMap<String, String> = closure_rows
        .iter()
        .map(|c| {
            let why = if c.reason.trim().is_empty() {
                "휴업일".to_string()
            } else {
                c.reason.clone()
            };
            (c.date.clone(), why)
        })
        .collect();
    let exceptions = load_exceptions(conn, workspace_id)?;

    let empty: HashMap<String, Exception> = HashMap::new();
    let rows: Vec<CustomFeeRow> = depts
        .iter()
        .map(|d| {
            reckon(
                d,
                start,
                end,
                &closed,
                exceptions.get(&d.id).unwrap_or(&empty),
                rate,
            )
        })
        .collect();

    // 맞춤형이 아닌 부서까지 보여 줄 때도 합계는 **맞춤형만** 더한다 —
    // 0차시 부서는 강사료가 없으니 합계가 달라지지 않지만, 반 수가 부풀면
    // '맞춤형 운영 반'이라는 이름과 맞지 않는다.
    let 맞춤형: Vec<&CustomFeeRow> = rows.iter().filter(|r| r.base_sessions > 0).collect();
    let summary = CustomFeeSummary {
        classes: 맞춤형.len() as i64,
        planned_days: 맞춤형.iter().map(|r| r.planned_days).sum(),
        closed_days: 맞춤형.iter().map(|r| r.closed_days).sum(),
        extra_days: 맞춤형.iter().map(|r| r.extra_days).sum(),
        counted_days: 맞춤형.iter().map(|r| r.counted_days).sum(),
        total_sessions: 맞춤형.iter().map(|r| r.total_sessions).sum(),
        total_fee: 맞춤형.iter().map(|r| r.fee).sum(),
    };

    Ok(CustomFeeReport {
        year_name: year.name,
        workspace_name: ws.name,
        start_date: ws.start_date,
        end_date: ws.end_date,
        rate,
        summary,
        orphans: find_orphans(&depts, start, end, &exceptions),
        closures: closure_rows,
        rows,
    })
}

// ─────────────────────────────────────────────── 고치기

fn ws_year(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT year_id FROM workspace WHERE id = ?1",
        params![workspace_id],
        |r| r.get(0),
    )?)
}

/// 공통 휴업일을 넣는다. 같은 날짜를 두 번 넣으면 사유만 갱신한다.
pub fn closure_add(
    conn: &Connection,
    workspace_id: i64,
    date: &str,
    reason: &str,
) -> AppResult<()> {
    let d = parse(date)?;
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    // 기간 밖 휴업일은 아무 날에도 걸리지 않아 사람을 헷갈리게 한다.
    if fmt(d) < ws.start_date || fmt(d) > ws.end_date {
        return Err(AppError::invalid(format!(
            "휴업일은 작업공간 기간({} ~ {}) 안의 날짜여야 합니다.",
            ws.start_date, ws.end_date
        )));
    }
    conn.execute(
        "INSERT INTO workspace_closure (workspace_id, on_date, reason) VALUES (?1, ?2, ?3)
         ON CONFLICT(workspace_id, on_date) DO UPDATE SET reason = excluded.reason",
        params![workspace_id, fmt(d), reason.trim()],
    )?;
    repo::change_log::write(
        conn,
        ws.year_id,
        Some(workspace_id),
        repo::change_log::WS_CLOSURE_ADD,
        None,
        None,
        &format!("{} ({})", fmt(d), weekday_label(d)),
        "",
        reason.trim(),
        "",
    )
}

pub fn closure_delete(conn: &Connection, id: i64) -> AppResult<()> {
    let found: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT workspace_id, on_date, reason FROM workspace_closure WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((workspace_id, date, reason)) = found else {
        return Err(AppError::not_found("휴업일을 찾지 못했습니다."));
    };
    conn.execute("DELETE FROM workspace_closure WHERE id = ?1", params![id])?;
    repo::change_log::write(
        conn,
        ws_year(conn, workspace_id)?,
        Some(workspace_id),
        repo::change_log::WS_CLOSURE_DELETE,
        None,
        None,
        &date,
        &reason,
        "",
        "",
    )
}

/// 부서의 한 날짜를 조정한다. `sessions = 0` 이면 휴강이다.
///
/// 예정일이든 아니든 같은 함수다 — 예정일이면 조정, 아니면 보강이 된다.
/// 그 구분은 저장하지 않고 기간·요일에서 늘 다시 낸다.
pub fn session_set(
    conn: &Connection,
    department_id: i64,
    date: &str,
    sessions: i64,
    reason: &str,
) -> AppResult<()> {
    if sessions < 0 {
        return Err(AppError::invalid("차시는 0 이상이어야 합니다."));
    }
    let d = parse(date)?;
    let (workspace_id, name, class_name): (i64, String, String) = conn.query_row(
        "SELECT workspace_id, name, class_name FROM department WHERE id = ?1",
        params![department_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let ws = repo::year::get_workspace(conn, workspace_id)?;
    if fmt(d) < ws.start_date || fmt(d) > ws.end_date {
        return Err(AppError::invalid(format!(
            "수업일은 작업공간 기간({} ~ {}) 안의 날짜여야 합니다.",
            ws.start_date, ws.end_date
        )));
    }

    let before: Option<i64> = conn
        .query_row(
            "SELECT sessions FROM dept_session WHERE department_id = ?1 AND on_date = ?2",
            params![department_id, fmt(d)],
            |r| r.get(0),
        )
        .ok();

    conn.execute(
        "INSERT INTO dept_session (department_id, on_date, sessions, reason)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(department_id, on_date)
         DO UPDATE SET sessions = excluded.sessions, reason = excluded.reason",
        params![department_id, fmt(d), sessions, reason.trim()],
    )?;

    repo::change_log::write(
        conn,
        ws.year_id,
        Some(workspace_id),
        repo::change_log::DEPT_SESSION_EDIT,
        None,
        Some(department_id),
        &format!("{name}{class_name} / {} ({})", fmt(d), weekday_label(d)),
        &before
            .map(|n| format!("{n}차시"))
            .unwrap_or_else(|| "기본".into()),
        &format!("{sessions}차시"),
        reason.trim(),
    )
}

/// 조정을 지운다. 그 날짜는 다시 기본 규칙으로 돌아간다 —
/// '기본 차시로 되돌리기'와 '추가 수업일 삭제'가 같은 동작이다.
pub fn session_clear(conn: &Connection, department_id: i64, date: &str) -> AppResult<()> {
    let d = parse(date)?;
    let (workspace_id, name, class_name): (i64, String, String) = conn.query_row(
        "SELECT workspace_id, name, class_name FROM department WHERE id = ?1",
        params![department_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let before: Option<i64> = conn
        .query_row(
            "SELECT sessions FROM dept_session WHERE department_id = ?1 AND on_date = ?2",
            params![department_id, fmt(d)],
            |r| r.get(0),
        )
        .ok();
    if before.is_none() {
        return Ok(()); // 지울 것이 없다
    }
    conn.execute(
        "DELETE FROM dept_session WHERE department_id = ?1 AND on_date = ?2",
        params![department_id, fmt(d)],
    )?;
    repo::change_log::write(
        conn,
        ws_year(conn, workspace_id)?,
        Some(workspace_id),
        repo::change_log::DEPT_SESSION_CLEAR,
        None,
        Some(department_id),
        &format!("{name}{class_name} / {}", fmt(d)),
        &before.map(|n| format!("{n}차시")).unwrap_or_default(),
        "기본",
        "",
    )
}

/// 학년도의 차시당 단가를 고친다. **이미 만든 정산과 무관하다.**
pub fn rate_set(conn: &Connection, year_id: i64, rate: i64) -> AppResult<()> {
    if rate < 0 {
        return Err(AppError::invalid("단가는 0원 이상이어야 합니다."));
    }
    let before = rate_of_year(conn, year_id)?;
    if before == rate {
        return Ok(());
    }
    conn.execute(
        "UPDATE academic_year SET custom_session_rate = ?2 WHERE id = ?1",
        params![year_id, rate],
    )?;
    let year = repo::year::get_year(conn, year_id)?;
    repo::change_log::write(
        conn,
        year_id,
        None,
        repo::change_log::YEAR_RATE_EDIT,
        None,
        None,
        &year.name,
        &format!("{}원", repo::enrollment::won(before)),
        &format!("{}원", repo::enrollment::won(rate)),
        "",
    )
}
