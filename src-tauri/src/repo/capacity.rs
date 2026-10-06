//! 부서별 수강현황 — 정원 · 학년 · 요일 통계 (v0.1.7, 설계안 28장).
//!
//! ## 저장하지 않는다
//!
//! 현재 수강인원 · 남은 자리 · 충원율 · 상태 · 학년별 · 요일별 — 이 가운데
//! DB에 적어 두는 것은 하나도 없다. 모두 `department.capacity` ·
//! `department_allowed_grade` · `enrollment` · `student` 에서 그때그때 센다.
//! 통계를 따로 적어 두면 원본이 바뀐 뒤 둘이 어긋나고, 어느 쪽이 맞는지
//! 아무도 모르게 된다.
//!
//! ## 부서마다 다시 묻지 않는다
//!
//! 부서가 스물넷이면 질의도 스물넷이 되기 쉽다. 여기서는 **부서 묶음 한 번 ·
//! 학년 묶음 한 번 · 학년도 학생 수 한 번**으로 끝낸다. 요일은 SQL 로 가를 수
//! 없어(자유 입력 글자열이다) 읽어 온 줄을 Rust 에서 접는다.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::domain::{clean_grades, parse_days, DAY_ORDER, DAY_UNKNOWN};
use crate::error::AppResult;
use crate::model::{
    CapacityStats, CapacityStatus, CapacitySummary, DeptCapacityRow, GradeStatRow, SeatDayGroup,
    SeatFinding, SeatQuery, SeatResult, SeatRow, WeekdayStatRow,
};
use crate::repo;

/// 소수 첫째 자리까지. 화면·Excel 이 같은 값을 쓰도록 여기서 한 번만 깎는다.
fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// 부서 한 줄의 정원 셈. 정원이 없으면 남은 자리도 충원율도 **없다** — 0 이 아니다.
fn reckon(capacity: Option<i64>, current: i64) -> (Option<i64>, Option<f64>, CapacityStatus) {
    match capacity {
        None => (None, None, CapacityStatus::Unset),
        Some(cap) => {
            // 넘친 자리는 음수 그대로 둔다. 0 으로 올려 버리면 몇 명이 넘쳤는지
            // 담당자가 알 수 없다.
            let remaining = cap - current;
            let rate = round1(current as f64 / cap as f64 * 100.0);
            let status = if current < cap {
                CapacityStatus::Open
            } else if current == cap {
                CapacityStatus::Full
            } else {
                CapacityStatus::Over
            };
            (Some(remaining), Some(rate), status)
        }
    }
}

/// 부서별 줄. 수강현황 · 수강 가능 부서 찾기 · Excel 이 모두 이것을 쓴다.
///
/// 수강인원은 `enrollment_active_uq` 덕분에 **건수가 곧 학생 수**다 — 같은
/// 학생이 같은 반에 두 번 ACTIVE 로 들어갈 수 없다.
pub fn dept_rows(conn: &Connection, workspace_id: i64) -> AppResult<Vec<DeptCapacityRow>> {
    let mut grades = repo::department::grades_of_workspace(conn, workspace_id)?;

    let mut st = conn.prepare(
        "SELECT d.id, d.name, d.class_name, d.teacher, d.days, d.capacity,
                COUNT(e.id)
           FROM department d
           LEFT JOIN enrollment e
                  ON e.department_id = d.id AND e.status = 'ACTIVE'
          WHERE d.workspace_id = ?1
          GROUP BY d.id
          ORDER BY d.name, d.class_name",
    )?;
    let raw = st
        .query_map(params![workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    Ok(raw
        .into_iter()
        .map(|(id, name, class_name, teacher, days, capacity, current)| {
            let (remaining, fill_rate, status) = reckon(capacity, current);
            DeptCapacityRow {
                department_id: id,
                name,
                class_name,
                teacher,
                days,
                allowed_grades: grades.remove(&id).unwrap_or_default(),
                capacity,
                current_count: current,
                remaining,
                fill_rate,
                status,
            }
        })
        .collect())
}

/// 윗줄 요약.
///
/// ## 정원을 정하지 않은 반을 0명으로 치지 않는다
///
/// `총 정원` 과 `평균 충원율` 은 **정원을 정한 반만** 더한다. 미설정 반을 0 으로
/// 세면 총 정원이 줄고 충원율이 부풀어 숫자가 거짓이 된다. 그래서 몇 개 반이
/// 빠졌는지(`classes_without_capacity`)를 함께 돌려주고 화면이 그것을 적는다.
///
/// ## `남은 자리` 는 실제로 더 받을 수 있는 자리다
///
/// `sum(정원 - 현재)` 로 하면 2명 넘친 반이 다른 반의 빈자리 2개를 지운다.
/// "앞으로 몇 명을 더 받을 수 있나" 를 묻는 자리이므로 `sum(max(…, 0))` 을
/// 쓴다. 반별 표에서는 넘친 값을 그대로(-2) 보여 주므로 두 수가 다를 수 있고,
/// 화면이 그 뜻을 적는다.
fn summarize(
    conn: &Connection,
    workspace_id: i64,
    rows: &[DeptCapacityRow],
) -> AppResult<CapacitySummary> {
    let with: Vec<&DeptCapacityRow> = rows.iter().filter(|r| r.capacity.is_some()).collect();
    let total_capacity: i64 = with.iter().filter_map(|r| r.capacity).sum();
    let counted_current: i64 = with.iter().map(|r| r.current_count).sum();
    let open_seats: i64 = with
        .iter()
        .map(|r| (r.capacity.unwrap_or(0) - r.current_count).max(0))
        .sum();
    let avg_fill_rate = if total_capacity > 0 {
        Some(round1(counted_current as f64 / total_capacity as f64 * 100.0))
    } else {
        None
    };

    // 수강 건수와 수강 학생 수는 다르다 — 셋을 듣는 학생은 건수 3, 학생 1.
    let (total_enrollments, total_students): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT student_id)
           FROM enrollment WHERE workspace_id = ?1 AND status = 'ACTIVE'",
        params![workspace_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    Ok(CapacitySummary {
        classes: rows.len() as i64,
        classes_with_capacity: with.len() as i64,
        classes_without_capacity: (rows.len() - with.len()) as i64,
        total_capacity,
        counted_current,
        total_enrollments,
        total_students,
        open_seats,
        avg_fill_rate,
        over_classes: rows
            .iter()
            .filter(|r| r.status == CapacityStatus::Over)
            .count() as i64,
    })
}

/// 학년별 현황.
///
/// 참여율의 분모는 **그 학년도에 등록된 그 학년 전체 학생 수**다
/// (`student.year_id`). 학생명단은 학년도 단위라 작업공간 기간 중의 전입·전출을
/// 따로 담지 않는다 — 그래서 "그 시점의 재적 수" 가 아니라 "지금 명단에 있는
/// 수" 다. 분모가 0인 학년은 참여율을 내지 않는다(명단에 없는 학년이다).
fn grade_rows(conn: &Connection, workspace_id: i64, year_id: i64) -> AppResult<Vec<GradeStatRow>> {
    let mut all: HashMap<i64, i64> = HashMap::new();
    {
        let mut st =
            conn.prepare("SELECT grade, COUNT(*) FROM student WHERE year_id = ?1 GROUP BY grade")?;
        let rows = st
            .query_map(params![year_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        all.extend(rows);
    }

    let mut st = conn.prepare(
        "SELECT s.grade, COUNT(DISTINCT e.student_id), COUNT(*)
           FROM enrollment e JOIN student s ON s.id = e.student_id
          WHERE e.workspace_id = ?1 AND e.status = 'ACTIVE'
          GROUP BY s.grade",
    )?;
    let joined: HashMap<i64, (i64, i64)> = st
        .query_map(params![workspace_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect();
    drop(st);

    // 수강생이 없는 학년도 0으로 보여 준다 — 줄이 아예 없으면 "자료가 빠졌나"
    // 싶어진다. 학생도 수강도 없는 학년만 뺀다.
    let mut keys: Vec<i64> = all.keys().copied().chain(joined.keys().copied()).collect();
    keys.sort_unstable();
    keys.dedup();

    Ok(keys
        .into_iter()
        .map(|grade| {
            let (students, enrollments) = joined.get(&grade).copied().unwrap_or((0, 0));
            let total = all.get(&grade).copied().unwrap_or(0);
            GradeStatRow {
                grade,
                students,
                enrollments,
                total_students: total,
                join_rate: (total > 0).then(|| round1(students as f64 / total as f64 * 100.0)),
            }
        })
        .collect())
}

/// 한 부서가 걸리는 요일들. 읽히지 않으면 `미지정` 한 칸으로 본다.
fn day_keys(days: &str) -> Vec<String> {
    let parsed = parse_days(days);
    if parsed.is_empty() {
        vec![DAY_UNKNOWN.to_string()]
    } else {
        parsed.iter().map(|d| d.to_string()).collect()
    }
}

/// 요일별 현황.
///
/// ## 여러 요일에 운영하는 반은 각 요일에 모두 센다
///
/// `월,수` 반의 수강 1건은 **월에 1건 · 수에 1건**이다. 담당자가 이 표에서
/// 묻는 것은 "수요일에 몇 명이 학교에 남는가" 이기 때문이다. 수요일 수업에
/// 오는 학생을 수요일 칸에서 빼면 그 숫자는 쓸모가 없다.
///
/// 대신 **세로로 더한 값이 전체 수강 건수보다 커진다.** 그래서 이 표에는
/// 합계 줄을 두지 않고, 화면과 Excel 이 그 까닭을 적는다.
///
/// 요일을 읽을 수 없는 반(`미정`, 빈 칸)은 조용히 버리지 않고 `미지정` 줄에
/// 모은다 — 빠진 반이 있다는 것을 담당자가 알아야 한다.
fn weekday_rows(rows: &[DeptCapacityRow]) -> Vec<WeekdayStatRow> {
    let blank = |day: &str| WeekdayStatRow {
        day: day.to_string(),
        classes: 0,
        enrollments: 0,
        classes_with_capacity: 0,
        total_capacity: 0,
        open_seats: 0,
    };

    let mut acc: HashMap<String, WeekdayStatRow> = HashMap::new();
    for day in DAY_ORDER {
        acc.insert(day.to_string(), blank(day));
    }

    for r in rows {
        for key in day_keys(&r.days) {
            let slot = acc.entry(key.clone()).or_insert_with(|| blank(&key));
            slot.classes += 1;
            slot.enrollments += r.current_count;
            if let Some(cap) = r.capacity {
                slot.classes_with_capacity += 1;
                slot.total_capacity += cap;
                slot.open_seats += (cap - r.current_count).max(0);
            }
        }
    }

    let mut order: Vec<String> = DAY_ORDER.iter().map(|d| d.to_string()).collect();
    order.push(DAY_UNKNOWN.to_string());
    order
        .into_iter()
        .filter_map(|d| acc.remove(&d))
        // 요일 일곱은 비어 있어도 보여 준다(운영하지 않는 날도 정보다).
        // `미지정` 은 해당하는 반이 있을 때만 나온다.
        .filter(|r| r.day != DAY_UNKNOWN || r.classes > 0)
        .collect()
}

pub fn stats(conn: &Connection, workspace_id: i64) -> AppResult<CapacityStats> {
    let year_id = repo::enrollment::year_of_workspace(conn, workspace_id)?;
    let rows = dept_rows(conn, workspace_id)?;
    Ok(CapacityStats {
        summary: summarize(conn, workspace_id, &rows)?,
        weekdays: weekday_rows(&rows),
        grades: grade_rows(conn, workspace_id, year_id)?,
        rows,
    })
}

/// 수강 가능 부서 찾기 — **조회만 한다.** 수강을 만들거나 고치지 않는다.
///
/// ## 미설정을 조용히 버리지 않는다
///
/// 정원을 정하지 않은 반은 자리를 셀 수 없고, 수강 가능 학년을 정하지 않은
/// 반은 이 학년이 대상인지 알 수 없다. 둘 다 `신청 가능` 으로 확정해 보여
/// 주면 담당자가 잘못 안내하게 되므로 **`확인 필요` 로 따로 세운다.**
///
/// 대상 학년이 **정해져 있는데 이 학년이 거기 없는** 반은 다르다. 그것은
/// 모르는 것이 아니라 아닌 것이므로 아예 빼낸다.
pub fn find_seats(conn: &Connection, workspace_id: i64, q: &SeatQuery) -> AppResult<SeatResult> {
    let rows = dept_rows(conn, workspace_id)?;

    // 고른 학생이 지금 듣고 있는 반. 취소한 수강은 지금 듣는 것이 아니므로
    // 빼지 않는다 — 취소했다가 다시 넣는 일이 실제로 있다.
    let mut taken: HashSet<i64> = HashSet::new();
    if let Some(sid) = q.student_id {
        let mut st = conn.prepare(
            "SELECT department_id FROM enrollment
              WHERE workspace_id = ?1 AND student_id = ?2 AND status = 'ACTIVE'",
        )?;
        let got = st
            .query_map(params![workspace_id, sid], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        taken.extend(got);
    }

    let mut excluded: Vec<String> = Vec::new();
    let mut picked: Vec<SeatRow> = Vec::new();
    for r in rows {
        let grades = clean_grades(&r.allowed_grades);
        let finding = if grades.is_empty() {
            SeatFinding::GradeUnknown
        } else if !grades.contains(&q.grade) {
            continue;
        } else {
            match r.status {
                CapacityStatus::Unset => SeatFinding::CapacityUnknown,
                CapacityStatus::Open => SeatFinding::Open,
                CapacityStatus::Full => SeatFinding::Full,
                CapacityStatus::Over => SeatFinding::Over,
            }
        };

        // 이미 듣는 반은 여기서 뺀다 — 대상 학년이 아닌 반까지 "뺐다"고
        // 알리면 목록이 쓸데없이 길어진다.
        if taken.contains(&r.department_id) {
            excluded.push(format!("{}{}", r.name, r.class_name));
            continue;
        }

        let closed = matches!(finding, SeatFinding::Full | SeatFinding::Over);
        if closed && !q.include_closed {
            continue;
        }

        picked.push(SeatRow {
            department_id: r.department_id,
            name: r.name,
            class_name: r.class_name,
            teacher: r.teacher,
            days: r.days,
            allowed_grades: r.allowed_grades,
            capacity: r.capacity,
            current_count: r.current_count,
            remaining: r.remaining,
            finding,
        });
    }

    // 요일별로 묶는다. 여러 요일 반은 각 요일에 모두 나온다 — 담당자가 월요일
    // 안내를 하는 중이라면 그 반도 보여야 한다.
    let mut by: HashMap<String, Vec<SeatRow>> = HashMap::new();
    for row in &picked {
        for k in day_keys(&row.days) {
            by.entry(k).or_default().push(row.clone());
        }
    }

    let mut order: Vec<String> = DAY_ORDER.iter().map(|d| d.to_string()).collect();
    order.push(DAY_UNKNOWN.to_string());
    let by_day = order
        .into_iter()
        .filter_map(|day| by.remove(&day).map(|rows| SeatDayGroup { day, rows }))
        .collect();

    excluded.sort();
    excluded.dedup();
    Ok(SeatResult { by_day, excluded })
}
