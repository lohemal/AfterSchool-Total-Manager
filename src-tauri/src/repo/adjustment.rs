//! 추가징수 · 환불 기록 (v0.1.5, 설계안 26장).
//!
//! ## 수강생 명단과 다른 자료다
//!
//! 수강생 명단은 **지금 이 학생이 얼마를 내는가**를 관리한다. 여기는 **이미 한
//! 번 걷은 뒤에 얼마를 더 걷거나 돌려주어야 하는가**를 관리한다. 그래서 금액을
//! `charge` 에서 그때그때 읽어 오지 않고 만든 시점의 값으로 굳혀 둔다.
//!
//! ## 확인 필요는 깃발이 아니라 값 비교로 정한다
//!
//! 금액을 고치는 경로는 여럿이다 — 학생별 수정, 취소, 부서 기준금액 재반영,
//! Excel 일괄수정. 그 모두가 깃발을 세우도록 만들면 한 곳만 빠져도 조용히
//! 어긋나고, 같은 값을 다시 쓰는 것(no-op)까지 확인 필요로 잡힌다.
//!
//! 그래서 **마지막으로 사람이 확인한 그때의 charge**(`checked_charge`)를 적어
//! 두고, 읽을 때 지금 charge 와 견준다.
//!
//! ```text
//! 확인 필요  ⇔  charge.amount ≠ checked_charge   (항목별)
//! ```
//!
//! 값을 견주므로 no-op 은 애초에 잡히지 않고, 고치는 경로가 늘어도 손댈 것이
//! 없다. [기존 금액 유지]는 `checked_charge` 만 지금 값으로 옮기고, [현재 금액
//! 반영]은 `amount` 까지 옮긴다. 그 뒤 또 바뀌면 다시 달라지므로 새로운 확인
//! 필요가 된다.
//!
//! ## 정산을 낡게 만들지 않는다
//!
//! 이 표에는 자료판 트리거가 없다. 추가징수·환불 기록을 만드는 것은 정산의
//! 원본을 바꾸는 일이 아니기 때문이다.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::error::{AppError, AppResult};
use crate::model::{
    Adjustment, AdjustmentDiff, AdjustmentFee, AdjustmentFilter, AdjustmentInput,
    AdjustmentReport, AdjustmentView, BatchResult, BillingBatch, CostItem, DeltaRegister, Enrollment,
    RevertPreview, RevertRow,
    Fee, FeePick, StudentSumRow,
};
use crate::repo;
use crate::repo::enrollment::{dept_label, student_label, won};

pub const ADDITIONAL: &str = "ADDITIONAL_CHARGE";
pub const REFUND: &str = "REFUND";

/// `기존 금액 유지` / `현재 금액 반영`.
pub const KEEP: &str = "KEEP";
pub const APPLY: &str = "APPLY";

/// 오늘 날짜를 SQLite 에게 묻는다.
///
/// 프로그램 안의 다른 시각(`created_at` 등)이 모두 SQLite 의 지역시각이므로
/// 여기서도 같은 시계를 쓴다. 두 시계를 섞으면 자정 무렵에 날짜가 어긋난다.
fn today_of(conn: &Connection) -> AppResult<String> {
    Ok(conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?)
}

/// 발생일을 다듬는다. 비어 있으면 오늘.
fn occurred_of(conn: &Connection, input: &AdjustmentInput) -> AppResult<String> {
    let raw = input
        .occurred_on
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match raw {
        None => today_of(conn),
        Some(v) => {
            if v.len() != 10 || v.as_bytes()[4] != b'-' || v.as_bytes()[7] != b'-' {
                return Err(AppError::invalid("발생일은 2026-09-22 꼴이어야 합니다."));
            }
            Ok(v.to_string())
        }
    }
}

// ─────────────────────────────────────────────── 만들기

/// 추가징수 — 지금 확정된 charge 를 그대로 굳혀 둔다.
///
/// 수강 건을 만드는 것과 **같은 트랜잭션**에서 불러야 한다. 부르는 쪽
/// (`commands::enrollment`)이 `Db::write` 하나로 감싼다.
pub fn create_additional(
    conn: &Connection,
    enrollment_id: i64,
    input: &AdjustmentInput,
    items: &[CostItem],
) -> AppResult<i64> {
    let e = repo::enrollment::get(conn, enrollment_id, items)?;
    let occurred = occurred_of(conn, input)?;
    let id = insert(conn, &e, ADDITIONAL, SRC_ENROLL, None, &occurred, input)?;

    for it in items {
        let amount = fee_of(&e.fees, &it.code);
        conn.execute(
            "INSERT INTO billing_adjustment_amount
                 (adjustment_id, item_code, amount, base_amount, checked_charge)
             VALUES (?1, ?2, ?3, 0, ?3)",
            params![id, it.code, amount],
        )?;
    }
    Ok(id)
}

/// 환불 — `취소 직전 금액 - 취소 후 최종 금액`.
///
/// `before` 는 **취소하기 직전**의 수강 건이다. 취소가 끝난 뒤에는 그 값을
/// 다시 구할 수 없으므로 부르는 쪽이 미리 떠서 넘긴다.
///
/// 어느 항목이든 환불액이 음수가 되면 **만들지 않고 오류**다. 업무적으로
/// 확인이 필요한 상태를 0원으로 뭉개면 그 사실이 사라진다.
pub fn create_refund(
    conn: &Connection,
    enrollment_id: i64,
    before: &Enrollment,
    input: &AdjustmentInput,
    items: &[CostItem],
) -> AppResult<i64> {
    let after = repo::enrollment::get(conn, enrollment_id, items)?;
    let occurred = occurred_of(conn, input)?;

    // 먼저 전부 따져 본다 — 한 항목이라도 음수면 아무것도 만들지 않는다.
    let mut cells: Vec<(String, i64, i64)> = Vec::new();
    for it in items {
        let base = fee_of(&before.fees, &it.code);
        let now = fee_of(&after.fees, &it.code);
        let refund = base - now;
        if refund < 0 {
            return Err(AppError::invalid(format!(
                "{}의 취소 후 금액({}원)이 취소 전({}원)보다 많아 환불액이 음수가 됩니다. \
                 금액을 확인하시거나 [환불 대상] 체크를 풀고 취소해 주세요.",
                it.name,
                won(now),
                won(base)
            )));
        }
        cells.push((it.code.clone(), refund, base));
    }

    let id = insert(conn, &after, REFUND, SRC_CANCEL, None, &occurred, input)?;
    for (code, refund, base) in cells {
        let now = fee_of(&after.fees, &code);
        conn.execute(
            "INSERT INTO billing_adjustment_amount
                 (adjustment_id, item_code, amount, base_amount, checked_charge)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, code, refund, base, now],
        )?;
    }
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
fn insert(
    conn: &Connection,
    e: &Enrollment,
    kind: &str,
    source: &str,
    batch_id: Option<i64>,
    occurred_on: &str,
    input: &AdjustmentInput,
) -> AppResult<i64> {
    let ws: i64 = conn.query_row(
        "SELECT workspace_id FROM enrollment WHERE id = ?1",
        params![e.id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO billing_adjustment
             (workspace_id, student_id, enrollment_id, department_id, kind,
              occurred_on, note, student_label, dept_label, source, batch_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            ws,
            e.student_id,
            e.id,
            e.department_id,
            kind,
            occurred_on,
            input.note.as_deref().unwrap_or("").trim(),
            student_label(e.grade, &e.class_no, e.student_no, &e.name),
            e.dept_label,
            source,
            batch_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

// ─────────────────────────────────────────────── 발생 유형 (v0.1.9)

pub const SRC_LEGACY: &str = "LEGACY";
pub const SRC_ENROLL: &str = "ENROLL";
pub const SRC_CANCEL: &str = "CANCEL";
pub const SRC_DEPT_BATCH: &str = "DEPT_BATCH";
pub const SRC_INDIVIDUAL: &str = "INDIVIDUAL";

/// 차액을 적어 둔 기록인가. [현재 금액 반영] 공식이 여기서 갈린다.
///
/// ```text
/// 차액 기록     REFUND      amount = base - 지금
///               ADDITIONAL  amount = 지금 - base
/// 그 밖(전액)   REFUND      amount = base - 지금        (취소 환불, 예전 그대로)
///               ADDITIONAL  amount = 지금               (중도 추가 전액, 예전 그대로)
/// ```
///
/// 차액 추가징수를 전액 공식으로 다시 잡으면 5,000원이 40,000원으로 부푼다.
/// v0.1.8 코드에서 실제로 그렇게 되는 것을 재 보고 이 갈림을 넣었다.
pub fn is_delta(source: &str) -> bool {
    source == SRC_DEPT_BATCH || source == SRC_INDIVIDUAL
}

/// [현재 금액 반영] 때 이 칸의 금액을 다시 낸다. 음수면 확인이 필요하다.
///
/// ## 두 갈래
///
/// **같은 수강 건에 차액 기록이 하나도 없으면** (`has_delta = false`) v0.1.8
/// 공식을 그대로 쓴다. v0.1.8 이전의 모든 기록이 이 갈래이므로, 기존 취소 환불·
/// 중도 추가의 계산은 한 글자도 바뀌지 않는다.
///
/// ```text
/// REFUND      base - 지금
/// ADDITIONAL  지금                (중도 추가 전액)
/// ```
///
/// **차액 기록이 함께 있으면** 한 번의 금액 변화를 **한 기록만** 설명해야 한다.
/// `base` 에서 다시 잡으면, 앞 기록이 뒤 기록이 이미 설명한 변화까지 제 몫으로
/// 다시 세어 같은 돈이 두 번 적힌다(v0.1.8 코드로 실제로 재 보니 8,000 이 11,000
/// 이 되었다). 그래서 이 갈래에서는 **아직 아무도 설명하지 않은 변화분**
/// (`확인값 → 지금`)만 반영한다.
///
/// ```text
/// 확인값 = 지금   → 그대로 (이미 다른 기록이 설명했다)
/// REFUND          금액 + (확인값 - 지금)
/// ADDITIONAL      금액 + (지금 - 확인값)
/// ```
///
/// 기록이 하나뿐일 때는 두 갈래의 결과가 같다(`금액 = base - 확인값` 이므로).
pub fn recompute(
    kind: &str,
    has_delta: bool,
    amount: i64,
    base: i64,
    checked: i64,
    current: i64,
) -> i64 {
    if !has_delta {
        return if kind == REFUND { base - current } else { current };
    }
    if checked == current {
        return amount;
    }
    if kind == REFUND {
        amount + (checked - current)
    } else {
        amount + (current - checked)
    }
}

/// 이 수강 건에 차액 기록이 있는가 — [현재 금액 반영] 공식이 여기서 갈린다.
fn has_delta_on(conn: &Connection, enrollment_id: i64) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM billing_adjustment
          WHERE enrollment_id = ?1 AND source IN ('DEPT_BATCH', 'INDIVIDUAL')",
        params![enrollment_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 고치기 직전(`before`)과 지금 charge 를 견주어 **차액**을 기록한다.
///
/// 줄어든 항목은 환불 한 건에, 늘어난 항목은 추가징수 한 건에 담는다. 한
/// 학생의 강사료는 줄고 재료비는 늘었다면 두 건이 생긴다 — 한 기록 안에서
/// 금액이 음수가 될 수 없기 때문이다(`amount >= 0`).
///
/// ## charge 를 고치지 않는다
///
/// 금액을 바꾸는 것은 부르는 쪽이 **먼저** 한다. 여기서는 그 결과를 적기만
/// 한다. 그래서 학생이 최종으로 내는 돈은 늘 charge 하나이고, 이 기록이 그
/// 위에 한 번 더 더해지거나 빠지는 일이 없다.
///
/// 차액이 하나도 없으면 아무것도 만들지 않는다. 같은 요청이 두 번 와도 첫
/// 번째에서 charge 가 이미 바뀌었으므로 두 번째는 차액이 0 이다 — 중복 기록이
/// 생기지 않는 까닭이다.
#[allow(clippy::too_many_arguments)]
pub fn create_delta(
    conn: &Connection,
    enrollment_id: i64,
    before: &Enrollment,
    source: &str,
    batch_id: Option<i64>,
    input: &AdjustmentInput,
    items: &[CostItem],
) -> AppResult<Vec<i64>> {
    if !is_delta(source) {
        return Err(AppError::invalid("차액 기록의 발생 유형이 아닙니다."));
    }
    let after = repo::enrollment::get(conn, enrollment_id, items)?;
    let occurred = occurred_of(conn, input)?;

    // (항목, |차액|, 고치기 전, 고친 뒤)
    let mut down: Vec<(String, i64, i64, i64)> = Vec::new();
    let mut up: Vec<(String, i64, i64, i64)> = Vec::new();
    for it in items {
        let base = fee_of(&before.fees, &it.code);
        let now = fee_of(&after.fees, &it.code);
        match now.cmp(&base) {
            std::cmp::Ordering::Less => down.push((it.code.clone(), base - now, base, now)),
            std::cmp::Ordering::Greater => up.push((it.code.clone(), now - base, base, now)),
            std::cmp::Ordering::Equal => {}
        }
    }

    let mut ids = Vec::new();
    for (kind, cells) in [(REFUND, &down), (ADDITIONAL, &up)] {
        if cells.is_empty() {
            continue;
        }
        let id = insert(conn, &after, kind, source, batch_id, &occurred, input)?;
        // 기록에 오르지 않은 항목도 0 으로 둔다 — 화면이 항목 칸을 늘 같은 차례로
        // 그리기 때문이다. checked_charge 는 지금 값이라 바로 '확인 필요'가 되지 않는다.
        for it in items {
            let now = fee_of(&after.fees, &it.code);
            let mine = cells.iter().find(|(code, ..)| code == &it.code);
            let amount = mine.map(|(_, d, ..)| *d).unwrap_or(0);
            // 이 기록에 속한 항목만 '고치기 전'을 기준으로 삼는다. 속하지 않은
            // 항목(반대 방향으로 움직였거나 그대로인 항목)은 기준을 지금 값으로
            // 둔다 — 그러지 않으면 환불 기록 안에서 늘어난 항목이 [현재 금액
            // 반영] 때 음수로 다시 잡힌다.
            let base = match mine {
                Some((_, _, b, _)) => *b,
                None => now,
            };
            conn.execute(
                "INSERT INTO billing_adjustment_amount
                     (adjustment_id, item_code, amount, base_amount, checked_charge)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, it.code, amount, base, now],
            )?;
        }
        ids.push(id);
    }

    if !ids.is_empty() {
        acknowledge_siblings(conn, enrollment_id, &ids, before, &after, items)?;
    }
    Ok(ids)
}

/// 같은 수강 건의 **다른 기록**이 이번 변화를 한 번 더 세지 않게 한다.
///
/// ## 왜 필요한가
///
/// 김민수가 10/10 에 5,000원, 10/25 에 3,000원을 환불받았다고 하자.
///
/// ```text
/// 10/10  35,000 → 30,000   환불 기록 ①  (기준 35,000)
/// 10/25  30,000 → 27,000   환불 기록 ②  (기준 30,000)
/// ```
///
/// ②를 만든 순간 charge 가 27,000 으로 바뀌므로 ①은 '확인 필요'로 뜬다. 이때
/// ①에서 [현재 금액 반영]을 누르면 ① = 35,000 - 27,000 = 8,000 이 되어, ②의
/// 3,000 과 합쳐 **11,000원**을 돌려주라고 적힌다. 실제로는 8,000원이다.
///
/// 그 3,000원 변화는 방금 사람이 ②로 **설명한** 것이다. 그래서 ①의 '여기까지
/// 확인했다'를 지금 값으로 옮겨, ①이 그 변화를 다시 주장하지 못하게 한다.
///
/// ## 설명하지 않은 변화는 건드리지 않는다
///
/// 옮기는 것은 **이번 변화 직전 값에 맞춰져 있던 칸만**이다. 이미 다른 까닭으로
/// '확인 필요'였던 칸은 그대로 둔다 — 아직 아무도 그 변화를 설명하지 않았다.
fn acknowledge_siblings(
    conn: &Connection,
    enrollment_id: i64,
    mine: &[i64],
    before: &Enrollment,
    after: &Enrollment,
    items: &[CostItem],
) -> AppResult<()> {
    for it in items {
        let was = fee_of(&before.fees, &it.code);
        let now = fee_of(&after.fees, &it.code);
        if was == now {
            continue;
        }
        // 번호를 붙여 둔다 — 번호 없는 `?` 와 섞으면 차례가 글 위치에 기대게 된다.
        let placeholders = (0..mine.len())
            .map(|i| format!("?{}", i + 5))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE billing_adjustment_amount
                SET checked_charge = ?1
              WHERE item_code = ?2
                AND checked_charge = ?3
                AND adjustment_id IN (
                      SELECT id FROM billing_adjustment
                       WHERE enrollment_id = ?4 AND id NOT IN ({placeholders}))"
        );
        let mut args: Vec<Box<dyn ToSql>> = vec![
            Box::new(now),
            Box::new(it.code.clone()),
            Box::new(was),
            Box::new(enrollment_id),
        ];
        for id in mine {
            args.push(Box::new(*id));
        }
        let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
    }
    Ok(())
}

fn fee_of(fees: &[Fee], code: &str) -> i64 {
    fees.iter()
        .find(|f| f.item_code == code)
        .map(|f| f.amount)
        .unwrap_or(0)
}

// ─────────────────────────────────────────────── 읽기

struct Raw {
    id: i64,
    kind: String,
    student_id: i64,
    enrollment_id: i64,
    department_id: i64,
    grade: i64,
    class_no: String,
    student_no: i64,
    name: String,
    dept_label: String,
    status: String,
    student_label_at: String,
    dept_label_at: String,
    occurred_on: String,
    note: String,
    created_at: String,
    source: String,
    batch_id: Option<i64>,
}

const SELECT: &str = "
SELECT a.id, a.kind, a.student_id, a.enrollment_id, a.department_id,
       s.grade, s.class_no, s.student_no, s.name,
       d.name, d.class_name, e.status,
       a.student_label, a.dept_label, a.occurred_on, a.note, a.created_at,
       a.source, a.batch_id
  FROM billing_adjustment a
  JOIN student s    ON s.id = a.student_id
  JOIN department d ON d.id = a.department_id
  JOIN enrollment e ON e.id = a.enrollment_id";

const ORDER: &str = "
 ORDER BY s.grade, s.class_sort, s.class_no, s.student_no, s.name,
          d.name, d.class_name, a.occurred_on, a.id";

/// 항목별 금액을 조정 id 로 묶어 읽는다. 지금 charge 도 함께 가져온다.
fn amounts_of(
    conn: &Connection,
    workspace_id: i64,
) -> AppResult<HashMap<i64, Vec<(String, i64, i64, i64, i64)>>> {
    let mut st = conn.prepare(
        "SELECT m.adjustment_id, m.item_code, m.amount, m.base_amount, m.checked_charge,
                COALESCE(c.amount, 0)
           FROM billing_adjustment_amount m
           JOIN billing_adjustment a ON a.id = m.adjustment_id
           LEFT JOIN charge c
                  ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
          WHERE a.workspace_id = ?1",
    )?;
    let mut out: HashMap<i64, Vec<(String, i64, i64, i64, i64)>> = HashMap::new();
    for row in st.query_map(params![workspace_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })? {
        let (aid, code, amount, base, checked, current) = row?;
        out.entry(aid)
            .or_default()
            .push((code, amount, base, checked, current));
    }
    Ok(out)
}

fn map_raw(r: &rusqlite::Row) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        kind: r.get(1)?,
        student_id: r.get(2)?,
        enrollment_id: r.get(3)?,
        department_id: r.get(4)?,
        grade: r.get(5)?,
        class_no: r.get(6)?,
        student_no: r.get(7)?,
        name: r.get(8)?,
        dept_label: dept_label(&r.get::<_, String>(9)?, &r.get::<_, String>(10)?),
        status: r.get(11)?,
        student_label_at: r.get(12)?,
        dept_label_at: r.get(13)?,
        occurred_on: r.get(14)?,
        note: r.get(15)?,
        created_at: r.get(16)?,
        source: r.get(17)?,
        batch_id: r.get(18)?,
    })
}

/// 조건에 맞는 조정을 종류별로 읽는다.
///
/// **부서는 학생을 찾는 조건이다** — 학생별 징수 내역과 같은 원칙이다. 기간·
/// 학년·반·이름은 그대로 거른다(학생의 성질이거나, 기간처럼 본래 범위를
/// 정하는 조건이므로 합계의 뜻이 흔들리지 않는다).
fn load(
    conn: &Connection,
    workspace_id: i64,
    kind: &str,
    f: &AdjustmentFilter,
    items: &[CostItem],
) -> AppResult<Vec<Adjustment>> {
    let mut sql = format!("{SELECT} WHERE a.workspace_id = ?1 AND a.kind = ?2");
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(workspace_id), Box::new(kind.to_string())];

    let push = |sql: &mut String, args: &mut Vec<Box<dyn ToSql>>, frag: &str, v: Box<dyn ToSql>| {
        args.push(v);
        sql.push_str(&frag.replace("?N", &format!("?{}", args.len())));
    };

    if let Some(v) = f.from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        push(&mut sql, &mut args, " AND a.occurred_on >= ?N", Box::new(v.to_string()));
    }
    if let Some(v) = f.to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        push(&mut sql, &mut args, " AND a.occurred_on <= ?N", Box::new(v.to_string()));
    }
    if let Some(g) = f.grade {
        push(&mut sql, &mut args, " AND s.grade = ?N", Box::new(g));
    }
    if let Some(c) = f
        .class_no
        .as_deref()
        .map(crate::domain::class_no::normalize)
        .filter(|s| !s.is_empty())
    {
        push(&mut sql, &mut args, " AND s.class_no = ?N", Box::new(c));
    }
    if let Some(q) = f.query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        args.push(Box::new(format!("%{q}%")));
        let i = args.len();
        sql.push_str(&format!(
            " AND (s.name LIKE ?{i} OR d.name LIKE ?{i} OR d.class_name LIKE ?{i})"
        ));
    }
    // 발생 유형 (v0.1.9). 정해 둔 값만 받는다 — 사용자 글자를 SQL 에 그대로
    // 넣지 않으려고 목록에 있는 것만 통과시킨다.
    if let Some(src) = f.source.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if ![SRC_LEGACY, SRC_ENROLL, SRC_CANCEL, SRC_DEPT_BATCH, SRC_INDIVIDUAL].contains(&src) {
            return Err(AppError::invalid("알 수 없는 발생 유형입니다."));
        }
        push(&mut sql, &mut args, " AND a.source = ?N", Box::new(src.to_string()));
    }
    sql.push_str(ORDER);

    let mut st = conn.prepare(&sql)?;
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let raws = st
        .query_map(refs.as_slice(), map_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    // 부서 필터 — 그 부서의 조정이 있는 **학생**을 고르고, 그 학생의 줄은
    // 부서를 가리지 않고 모두 남긴다.
    let raws: Vec<Raw> = match f.department_id {
        Some(d) => {
            let ids: std::collections::HashSet<i64> = raws
                .iter()
                .filter(|r| r.department_id == d)
                .map(|r| r.student_id)
                .collect();
            raws.into_iter()
                .filter(|r| ids.contains(&r.student_id))
                .collect()
        }
        None => raws,
    };

    let amounts = amounts_of(conn, workspace_id)?;
    let name_of: HashMap<&str, &str> = items
        .iter()
        .map(|i| (i.code.as_str(), i.name.as_str()))
        .collect();

    Ok(raws
        .into_iter()
        .map(|r| {
            let mine = amounts.get(&r.id).cloned().unwrap_or_default();
            let by_code: HashMap<&str, &(String, i64, i64, i64, i64)> =
                mine.iter().map(|x| (x.0.as_str(), x)).collect();
            let fees: Vec<AdjustmentFee> = items
                .iter()
                .map(|it| {
                    let v = by_code.get(it.code.as_str());
                    AdjustmentFee {
                        item_code: it.code.clone(),
                        item_name: name_of.get(it.code.as_str()).copied().unwrap_or("").to_string(),
                        amount: v.map(|x| x.1).unwrap_or(0),
                        base_amount: v.map(|x| x.2).unwrap_or(0),
                        checked_charge: v.map(|x| x.3).unwrap_or(0),
                        current_charge: v.map(|x| x.4).unwrap_or(0),
                    }
                })
                .collect();
            let total = fees.iter().map(|f| f.amount).sum();
            let needs_check = fees.iter().any(|f| f.checked_charge != f.current_charge);
            Adjustment {
                id: r.id,
                kind: r.kind,
                student_id: r.student_id,
                enrollment_id: r.enrollment_id,
                department_id: r.department_id,
                grade: r.grade,
                class_no: r.class_no,
                student_no: r.student_no,
                name: r.name,
                dept_label: r.dept_label,
                enrollment_status: r.status,
                student_label_at: r.student_label_at,
                dept_label_at: r.dept_label_at,
                occurred_on: r.occurred_on,
                note: r.note,
                fees,
                total,
                needs_check,
                created_at: r.created_at,
                source: r.source,
                batch_id: r.batch_id,
            }
        })
        .collect())
}

/// 상세를 학생 단위로 모은다 — 학생별 징수 내역과 같은 모양.
fn summarize(details: Vec<Adjustment>, items: &[CostItem]) -> AdjustmentReport {
    let mut order: Vec<i64> = Vec::new();
    let mut rows: HashMap<i64, StudentSumRow> = HashMap::new();
    let mut per_student: HashMap<i64, HashMap<String, i64>> = HashMap::new();
    let mut grand: HashMap<String, i64> = HashMap::new();

    for a in &details {
        if !rows.contains_key(&a.student_id) {
            order.push(a.student_id);
            rows.insert(
                a.student_id,
                StudentSumRow {
                    student_id: a.student_id,
                    grade: a.grade,
                    class_no: a.class_no.clone(),
                    student_no: a.student_no,
                    name: a.name.clone(),
                    programs: Vec::new(),
                    fees: Vec::new(),
                    total: 0,
                    details: 0,
                },
            );
        }
        let row = rows.get_mut(&a.student_id).expect("방금 넣었다");
        row.details += 1;
        let mine = per_student.entry(a.student_id).or_default();
        for f in &a.fees {
            *mine.entry(f.item_code.clone()).or_insert(0) += f.amount;
            *grand.entry(f.item_code.clone()).or_insert(0) += f.amount;
        }
    }

    let sum = |map: &HashMap<String, i64>| -> (Vec<Fee>, i64) {
        let fees: Vec<Fee> = items
            .iter()
            .map(|it| Fee {
                item_code: it.code.clone(),
                amount: map.get(&it.code).copied().unwrap_or(0),
            })
            .collect();
        let total = fees.iter().map(|f| f.amount).sum();
        (fees, total)
    };

    let out: Vec<StudentSumRow> = order
        .into_iter()
        .filter_map(|id| {
            let mut row = rows.remove(&id)?;
            let empty = HashMap::new();
            let (fees, total) = sum(per_student.get(&id).unwrap_or(&empty));
            row.fees = fees;
            row.total = total;
            Some(row)
        })
        .collect();

    let (fees, total) = sum(&grand);
    AdjustmentReport {
        students: out.len() as i64,
        count: details.len() as i64,
        needs_check: details.iter().filter(|a| a.needs_check).count() as i64,
        rows: out,
        details,
        fees,
        total,
    }
}

pub fn view(
    conn: &Connection,
    workspace_id: i64,
    filter: &AdjustmentFilter,
    items: &[CostItem],
) -> AppResult<AdjustmentView> {
    Ok(AdjustmentView {
        additional: summarize(load(conn, workspace_id, ADDITIONAL, filter, items)?, items),
        refund: summarize(load(conn, workspace_id, REFUND, filter, items)?, items),
        needs_check_all: needs_check_count(conn, workspace_id)?,
    })
}

/// 작업공간 전체에서 확인이 필요한 조정 건수 — 사이드바 배지에 쓴다.
///
/// 화면 필터와 무관하다. 걸러 놓은 바깥에 확인할 것이 있는데 배지가 0이면
/// 그것을 영영 못 본다.
pub fn needs_check_count(conn: &Connection, workspace_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT a.id)
           FROM billing_adjustment a
           JOIN billing_adjustment_amount m ON m.adjustment_id = a.id
           LEFT JOIN charge c
                  ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
          WHERE a.workspace_id = ?1
            AND m.checked_charge <> COALESCE(c.amount, 0)",
        params![workspace_id],
        |r| r.get(0),
    )?)
}

/// 원본이 달라진 칸만 모아 준다 — [변경내역 확인] 화면이 쓴다.
pub fn diffs(
    conn: &Connection,
    workspace_id: i64,
    items: &[CostItem],
) -> AppResult<Vec<AdjustmentDiff>> {
    let mut out = Vec::new();
    for kind in [ADDITIONAL, REFUND] {
        for a in load(conn, workspace_id, kind, &AdjustmentFilter::default(), items)? {
            if !a.needs_check {
                continue;
            }
            // 화면이 보여 주는 제안값도 [현재 금액 반영]과 같은 공식에서 낸다.
            let has_delta = has_delta_on(conn, a.enrollment_id)?;
            for f in &a.fees {
                if f.checked_charge == f.current_charge {
                    continue;
                }
                let suggested = recompute(
                    &a.kind,
                    has_delta,
                    f.amount,
                    f.base_amount,
                    f.checked_charge,
                    f.current_charge,
                );
                out.push(AdjustmentDiff {
                    adjustment_id: a.id,
                    kind: a.kind.clone(),
                    student_label: student_label(a.grade, &a.class_no, a.student_no, &a.name),
                    dept_label: a.dept_label.clone(),
                    occurred_on: a.occurred_on.clone(),
                    item_code: f.item_code.clone(),
                    item_name: f.item_name.clone(),
                    saved: f.amount,
                    suggested: suggested.max(0),
                    checked_charge: f.checked_charge,
                    current_charge: f.current_charge,
                    base_amount: f.base_amount,
                    negative: suggested < 0,
                });
            }
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────── 확인

/// 변경내역을 확인 처리한다.
///
/// * `KEEP`  — 금액은 그대로 두고 확인만 한다. 이미 그 금액으로 안내했을 때.
/// * `APPLY` — 지금 원본을 기준으로 금액을 다시 잡는다.
///
/// 어느 쪽이든 `checked_charge` 를 지금 값으로 옮기므로 경고가 사라진다.
/// 그 뒤 원본이 또 바뀌면 다시 달라져 새로운 확인 필요가 된다.
pub fn confirm(conn: &Connection, ids: &[i64], mode: &str) -> AppResult<i64> {
    if mode != KEEP && mode != APPLY {
        return Err(AppError::invalid("알 수 없는 확인 방식입니다."));
    }
    let mut done = 0;
    for id in ids {
        let (kind, enrollment_id): (String, i64) = conn
            .query_row(
                "SELECT kind, enrollment_id FROM billing_adjustment WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("기록을 찾지 못했습니다."))?;

        if mode == APPLY {
            // 칸마다 값을 읽어 공식은 한 곳(`recompute`)에서만 쓴다. SQL 안에
            // 공식을 따로 적어 두면 두 갈래가 두 곳에 생긴다.
            let has_delta = has_delta_on(conn, enrollment_id)?;
            let mut st = conn.prepare(
                "SELECT m.item_code, m.amount, m.base_amount, m.checked_charge,
                        COALESCE(c.amount, 0)
                   FROM billing_adjustment_amount m
                   JOIN billing_adjustment a ON a.id = m.adjustment_id
                   LEFT JOIN charge c
                          ON c.enrollment_id = a.enrollment_id AND c.item_code = m.item_code
                  WHERE m.adjustment_id = ?1",
            )?;
            #[allow(clippy::type_complexity)]
            let cells: Vec<(String, i64, i64, i64, i64)> = st
                .query_map(params![id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(st);

            let next: Vec<(String, i64, i64, i64)> = cells
                .iter()
                .map(|(code, amount, base, checked, now)| {
                    (
                        code.clone(),
                        recompute(&kind, has_delta, *amount, *base, *checked, *now),
                        *checked,
                        *now,
                    )
                })
                .collect();

            // 음수가 생기면 반영하지 않는다 — 확인이 필요한 상태다.
            if next.iter().any(|(_, v, ..)| *v < 0) {
                return Err(AppError::invalid(if kind == REFUND {
                    "고친 뒤 금액이 기준보다 많아 환불액이 음수가 됩니다. \
                     수강생 명단에서 금액을 먼저 확인해 주세요."
                } else {
                    "고친 뒤 금액이 기준보다 적어 추가징수액이 음수가 됩니다. \
                     수강생 명단에서 금액을 먼저 확인해 주세요."
                }));
            }

            for (code, value, checked, now) in &next {
                conn.execute(
                    "UPDATE billing_adjustment_amount SET amount = ?3
                      WHERE adjustment_id = ?1 AND item_code = ?2",
                    params![id, code, value],
                )?;
                // 차액 기록이 있는 수강 건에서는, 이 기록이 방금 이 변화를 제 몫으로
                // 가져갔으므로 형제 기록이 같은 변화를 다시 가져가지 못하게 한다.
                // 차액 기록이 없는 수강 건은 v0.1.8 그대로 둔다 — 형제를 건드리지 않는다.
                if has_delta && checked != now {
                    conn.execute(
                        "UPDATE billing_adjustment_amount
                            SET checked_charge = ?1
                          WHERE item_code = ?2
                            AND checked_charge = ?3
                            AND adjustment_id <> ?4
                            AND adjustment_id IN (
                                  SELECT id FROM billing_adjustment WHERE enrollment_id = ?5)",
                        params![now, code, checked, id, enrollment_id],
                    )?;
                }
            }
        }

        // 어느 쪽이든 '여기까지 확인했다'를 남긴다.
        conn.execute(
            "UPDATE billing_adjustment_amount AS m
                SET checked_charge = COALESCE(
                      (SELECT c.amount FROM charge c
                        JOIN billing_adjustment a ON a.id = m.adjustment_id
                       WHERE c.enrollment_id = a.enrollment_id
                         AND c.item_code = m.item_code), 0)
              WHERE m.adjustment_id = ?1",
            params![id],
        )?;
        conn.execute(
            "UPDATE billing_adjustment
                SET updated_at = datetime('now', 'localtime') WHERE id = ?1",
            params![id],
        )?;
        done += 1;
    }
    Ok(done)
}

/// 기록을 지운다. 사람이 고른 것만 지운다.
///
/// ## 차액 기록은 금액도 함께 되돌린다 (v0.1.9)
///
/// 부서일괄·개별조정 기록은 **charge 를 바꾸면서** 만들어졌다. 그 기록만 지우면
/// 환불은 사라졌는데 깎인 금액은 남아, 왜 30,000 인지 아무도 설명할 수 없게 된다.
/// 그래서 지우면 **그 기록의 차액만큼만** 되돌린다 (`revert_plan`).
///
/// 기존·중도추가·수강취소 기록은 예전처럼 기록만 지운다. 그 기록들은 charge 를
/// 바꾼 것이 아니라 이미 정해진 금액을 적어 둔 것이기 때문이다.
///
/// 하나라도 되돌릴 수 없으면 **아무것도 지우지 않는다** — 부르는 쪽이 한
/// 트랜잭션으로 감싸므로, 앞에서 되돌린 것도 함께 취소된다.
pub fn delete_many(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let items = repo::cost_items(conn)?;
    let mut n = 0;
    let mut batches_touched: Vec<i64> = Vec::new();
    for id in ids {
        let batch: Option<i64> = conn
            .query_row(
                "SELECT batch_id FROM billing_adjustment WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        revert_and_delete(conn, *id, &items, true)?;
        n += 1;
        if let Some(b) = batch {
            batches_touched.push(b);
        }
    }
    // 한 건씩 지우다 묶음이 비면 묶음도 치운다 — 딸린 기록 없는 묶음은 뜻이 없다.
    batches_touched.sort_unstable();
    batches_touched.dedup();
    for b in batches_touched {
        conn.execute(
            "DELETE FROM billing_batch
              WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM billing_adjustment WHERE batch_id = ?1)",
            params![b],
        )?;
    }
    Ok(n)
}

/// 기록 하나를 지우면 학생 금액이 어떻게 되는가. **아무것도 고치지 않는다.**
///
/// ## 왜 '차액만' 되돌리는가
///
/// '처음 금액'으로 되돌리면 안 된다. 35,000 에서 5,000·3,000 을 차례로 환불했다면
/// 5,000 기록을 지웠을 때 3,000 환불은 여전히 살아 있어야 한다.
///
/// ```text
/// 환불 기록을 지운다      금액 += 그 기록의 금액
/// 추가징수 기록을 지운다  금액 -= 그 기록의 금액
/// 27,000 에서 5,000 환불을 지우면 32,000 (35,000 이 아니다)
/// ```
///
/// ## 언제 막는가 — 확인값이 지금 금액과 다를 때
///
/// 기록의 `checked_charge` 는 '이 기록이 마지막으로 맞춰 본 그때의 금액'이다.
/// 지금 금액과 같다면, 이 기록 뒤의 변화는 모두 다른 기록(또는 [현재 금액 반영])이
/// 설명한 것이다 — 그때는 차액만 빼고 더해도 정확하다. 다르다면 **아무도 설명하지
/// 않은 변화**가 끼어 있다(누가 '금액만' 다시 고쳤다). 그 변화와 이 기록이 서로
/// 어떤 관계인지 프로그램은 알 수 없으므로, 짐작으로 고치지 않고 막는다.
///
/// 여러 차액 기록이 있어도 막히지 않는 까닭: 새 차액 기록을 만들 때와 기록을
/// 되돌릴 때마다 형제 기록의 확인값을 함께 옮겨 두기 때문이다.
///
/// 취소한 수강 건도 막는다. 그 금액은 취소 환불의 기준이 되므로, 차액을 되돌리면
/// 취소 환불 기록이 틀어진다.
pub fn revert_plan(conn: &Connection, id: i64, items: &[CostItem]) -> AppResult<RevertRow> {
    #[allow(clippy::type_complexity)]
    let (kind, source, enrollment_id): (String, String, i64) = conn
        .query_row(
            "SELECT kind, source, enrollment_id FROM billing_adjustment WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("기록을 찾지 못했습니다."))?;
    let e = repo::enrollment::get(conn, enrollment_id, items)?;
    let charge_before: i64 = e.fees.iter().map(|f| f.amount).sum();

    let mut st = conn.prepare(
        "SELECT m.item_code, m.amount, m.checked_charge, COALESCE(c.amount, 0)
           FROM billing_adjustment_amount m
           LEFT JOIN charge c ON c.enrollment_id = ?2 AND c.item_code = m.item_code
          WHERE m.adjustment_id = ?1",
    )?;
    let cells: Vec<(String, i64, i64, i64)> = st
        .query_map(params![id, enrollment_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let amount: i64 = cells.iter().map(|c| c.1).sum();

    let mut row = RevertRow {
        adjustment_id: id,
        kind: kind.clone(),
        source: source.clone(),
        student_label: student_label(e.grade, &e.class_no, e.student_no, &e.name),
        dept_label: e.dept_label.clone(),
        amount,
        reverts: is_delta(&source),
        charge_before,
        charge_after: charge_before,
        blocked: None,
    };
    if !row.reverts {
        return Ok(row); // 기존 기록 — 기록만 지운다 (v0.1.8 그대로)
    }

    if e.status != "ACTIVE" {
        row.blocked = Some(
            "취소한 수강 건이라 금액을 자동으로 되돌리지 않습니다. 이 금액은 취소 환불의 \
             기준이 되므로, 수강생 명단에서 확인한 뒤 직접 정정해 주세요."
                .into(),
        );
        return Ok(row);
    }

    let mut after = charge_before;
    for (code, amt, checked, now) in &cells {
        if *amt == 0 {
            continue; // 이 기록이 움직이지 않은 항목
        }
        if checked != now {
            row.blocked = Some(
                "이 조정 이후 금액이 따로 다시 변경되어 자동으로 되돌릴 수 없습니다. \
                 현재 금액을 확인한 후 직접 정정해 주세요."
                    .into(),
            );
            return Ok(row);
        }
        let next = if kind == REFUND { now + amt } else { now - amt };
        if next < 0 {
            let name = items
                .iter()
                .find(|i| &i.code == code)
                .map(|i| i.name.as_str())
                .unwrap_or("");
            row.blocked = Some(format!(
                "되돌리면 {name}이 0원보다 적어집니다. 현재 금액을 확인한 후 직접 정정해 주세요."
            ));
            return Ok(row);
        }
        after += if kind == REFUND { *amt } else { -*amt };
    }
    row.charge_after = after;
    Ok(row)
}

/// 여러 기록을 지웠을 때의 모습. 확인창이 쓴다. **아무것도 고치지 않는다.**
///
/// 같은 학생의 기록을 여럿 고르면 앞 기록을 되돌린 결과 위에서 뒤 기록을 따져야
/// 맞으므로, 실제로 되돌려 보고 트랜잭션을 버리는 대신 **차례대로 셈한다**:
/// 학생마다 금액이 바뀐 만큼을 이어서 더한다.
pub fn delete_preview(conn: &Connection, ids: &[i64], items: &[CostItem]) -> AppResult<RevertPreview> {
    let mut rows = Vec::new();
    let mut shift: HashMap<String, i64> = HashMap::new(); // 학생/부서 → 앞에서 바뀐 금액
    for id in ids {
        let mut r = revert_plan(conn, *id, items)?;
        let key = format!("{}|{}", r.student_label, r.dept_label);
        let moved = shift.get(&key).copied().unwrap_or(0);
        r.charge_before += moved;
        r.charge_after += moved;
        if r.reverts && r.blocked.is_none() {
            shift.insert(key, moved + (r.charge_after - r.charge_before));
        }
        rows.push(r);
    }
    Ok(summarize_revert(rows))
}

fn summarize_revert(rows: Vec<RevertRow>) -> RevertPreview {
    let mut students: Vec<&str> = rows
        .iter()
        .filter(|r| r.reverts)
        .map(|r| r.student_label.as_str())
        .collect();
    students.sort_unstable();
    students.dedup();
    let refund_total = rows.iter().filter(|r| r.kind == REFUND).map(|r| r.amount).sum();
    let additional_total = rows.iter().filter(|r| r.kind != REFUND).map(|r| r.amount).sum();
    let blocked = rows.iter().any(|r| r.blocked.is_some());
    RevertPreview {
        students: students.len() as i64,
        refund_total,
        additional_total,
        blocked,
        rows,
    }
}

/// 묶음을 지우면 어떻게 되는가.
pub fn batch_delete_preview(conn: &Connection, batch_id: i64, items: &[CostItem]) -> AppResult<RevertPreview> {
    let ids = batch_members(conn, batch_id)?;
    delete_preview(conn, &ids, items)
}

fn batch_members(conn: &Connection, batch_id: i64) -> AppResult<Vec<i64>> {
    let mut st = conn.prepare("SELECT id FROM billing_adjustment WHERE batch_id = ?1 ORDER BY id")?;
    let ids = st
        .query_map(params![batch_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

/// 기록 하나를 지우고, 차액 기록이면 그 차액을 charge 에 되돌린다.
///
/// 금액은 **표준 금액 쓰기 길**(`enrollment::update_fees`)로 바꾼다. 그래야 자료판
/// 트리거가 그대로 돌아 정산이 낡음이 되고, 학생별 금액 변화가 `CHARGE_EDIT` 로
/// 남는다 — 따로 자료판을 올리거나 이력을 꾸며 넣지 않는다.
fn revert_and_delete(conn: &Connection, id: i64, items: &[CostItem], log_each: bool) -> AppResult<()> {
    let plan = revert_plan(conn, id, items)?;
    if let Some(why) = &plan.blocked {
        return Err(AppError::invalid(format!(
            "{} / {} — {why}",
            plan.student_label, plan.dept_label
        )));
    }

    if plan.reverts {
        let (kind, enrollment_id, note): (String, i64, String) = conn.query_row(
            "SELECT kind, enrollment_id, note FROM billing_adjustment WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let before = repo::enrollment::get(conn, enrollment_id, items)?;

        let mut st = conn.prepare(
            "SELECT item_code, amount FROM billing_adjustment_amount WHERE adjustment_id = ?1",
        )?;
        let mine: HashMap<String, i64> = st
            .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(st);

        let fees: Vec<Fee> = items
            .iter()
            .map(|it| {
                let now = fee_of(&before.fees, &it.code);
                let d = mine.get(&it.code).copied().unwrap_or(0);
                Fee {
                    item_code: it.code.clone(),
                    amount: if kind == REFUND { now + d } else { now - d },
                }
            })
            .collect();
        let what = if kind == REFUND { "환불" } else { "추가징수" };
        repo::enrollment::update_fees(
            conn,
            enrollment_id,
            &fees,
            &format!("{what} 기록 삭제로 원복 — {note}"),
            items,
        )?;

        // 이 변화는 방금 지운 기록이 설명한다. 남은 형제 기록이 이것을 '아무도 설명하지
        // 않은 변화'로 잡아 확인 필요로 뜨지 않게, 그 확인값을 함께 옮긴다.
        let after = repo::enrollment::get(conn, enrollment_id, items)?;
        acknowledge_siblings(conn, enrollment_id, &[id], &before, &after, items)?;

        if log_each {
            let ws = workspace_of(conn, enrollment_id)?;
            repo::change_log::write(
                conn,
                repo::enrollment::year_of_workspace(conn, ws)?,
                Some(ws),
                repo::change_log::ADJUST_DELETE,
                Some(before.student_id),
                Some(before.department_id),
                &format!("{} / {}", plan.student_label, plan.dept_label),
                &format!("{what} {}원 · 금액 {}원", won(plan.amount), won(plan.charge_before)),
                &format!("기록 삭제 · 금액 {}원으로 원복", won(plan.charge_after)),
                &note,
            )?;
        }
    }

    conn.execute("DELETE FROM billing_adjustment WHERE id = ?1", params![id])?;
    Ok(())
}

// ─────────────────────────────────────────────── 지우기를 막는다

/// 이 조건에 걸리는 조정이 있으면 사람이 읽을 수 있는 까닭으로 막는다.
///
/// `ON DELETE RESTRICT` 가 마지막 방어선이지만, 그것만 두면 "FOREIGN KEY
/// constraint failed" 라는 말만 보게 된다.
fn block(conn: &Connection, sql: &str, args: &[&dyn ToSql], what: &str) -> AppResult<()> {
    let n: i64 = conn.query_row(sql, args, |r| r.get(0))?;
    if n > 0 {
        return Err(AppError::new(
            "ADJUSTMENT_EXISTS",
            format!(
                "{what}에 딸린 추가징수·환불 기록이 {n}건 있어 지울 수 없습니다. \
                 [수강 관리 › 추가·취소 관리]에서 그 기록을 먼저 지워 주세요."
            ),
        ));
    }
    Ok(())
}

pub fn block_for_students(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    for id in ids {
        block(
            conn,
            "SELECT COUNT(*) FROM billing_adjustment WHERE student_id = ?1",
            &[id],
            "학생",
        )?;
    }
    Ok(())
}

pub fn block_for_year(conn: &Connection, year_id: i64) -> AppResult<()> {
    block(
        conn,
        "SELECT COUNT(*) FROM billing_adjustment a
           JOIN student s ON s.id = a.student_id
          WHERE s.year_id = ?1",
        &[&year_id],
        "이 학년도",
    )
}

pub fn block_for_departments(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    for id in ids {
        block(
            conn,
            "SELECT COUNT(*) FROM billing_adjustment WHERE department_id = ?1",
            &[id],
            "부서",
        )?;
    }
    Ok(())
}

pub fn block_for_workspace(conn: &Connection, workspace_id: i64) -> AppResult<()> {
    block(
        conn,
        "SELECT COUNT(*) FROM billing_adjustment WHERE workspace_id = ?1",
        &[&workspace_id],
        "작업공간",
    )
}

// ─────────────────────────────────────────────── 부서 일괄 · 학생 개별 (v0.1.9)

/// 사유를 받는다. 돈이 오가는 기록은 까닭 없이 남기지 않는다.
fn required_note(reg: &DeltaRegister) -> AppResult<String> {
    let note = reg.note.as_deref().unwrap_or("").trim().to_string();
    if note.is_empty() {
        return Err(AppError::invalid(
            "추가징수·환불로 등록하려면 사유를 적어 주세요. 예) 강사 사정으로 1회 휴강",
        ));
    }
    Ok(note)
}

/// 차액을 기록할 수 있는 수강 건인가. **수강중(ACTIVE)만** 된다.
///
/// 취소한 수강 건에는 이미 '취소 환불' 기록이 있고, 그 기록은 [현재 금액 반영]
/// 으로 최종 금액을 따라간다. 거기에 차액 기록을 하나 더 얹으면 같은 돈을 두
/// 곳에서 세게 된다. 취소자의 금액을 고쳤다면 기존 기록의 '확인 필요'에서
/// 정리하는 것이 맞다.
fn guard_active(conn: &Connection, enrollment_id: i64) -> AppResult<()> {
    let status: String = conn
        .query_row(
            "SELECT status FROM enrollment WHERE id = ?1",
            params![enrollment_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("수강 건을 찾지 못했습니다."))?;
    if status != "ACTIVE" {
        return Err(AppError::invalid(
            "취소한 수강 건에는 차액을 따로 등록하지 않습니다. 이미 있는 '취소 환불' \
             기록의 [확인 필요]에서 금액을 정리해 주세요 — 두 곳에 적으면 같은 돈이 \
             두 번 셈됩니다.",
        ));
    }
    Ok(())
}

fn workspace_of(conn: &Connection, enrollment_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT workspace_id FROM enrollment WHERE id = ?1",
        params![enrollment_id],
        |r| r.get(0),
    )?)
}

/// 학생 한 명의 금액을 고치고, 원하면 그 차액을 추가징수·환불로 남긴다.
///
/// 금액 수정과 기록은 **한 트랜잭션**이다 — 부르는 쪽이 `Db::write` 하나로 감싼다.
/// 기록만 남고 금액은 안 바뀌거나 그 반대가 되면, 남은 쪽이 거짓이 된다.
pub fn update_fees_with_delta(
    conn: &Connection,
    enrollment_id: i64,
    fees: &[Fee],
    reason: &str,
    reg: &DeltaRegister,
    items: &[CostItem],
) -> AppResult<Vec<i64>> {
    let note = if reg.register {
        guard_active(conn, enrollment_id)?;
        Some(required_note(reg)?)
    } else {
        None
    };

    // 고치기 직전 값은 **지금 DB 에서** 뜬다. 화면이 보낸 '이전 값'을 믿지 않는다 —
    // 같은 요청이 두 번 오면 두 번째는 이미 바뀐 값을 '이전'으로 보게 되어
    // 차액이 0 이 되고, 기록이 겹치지 않는다.
    let before = repo::enrollment::get(conn, enrollment_id, items)?;
    repo::enrollment::update_fees(conn, enrollment_id, fees, reason, items)?;

    let Some(note) = note else {
        return Ok(Vec::new()); // 금액만 고친다 — 잘못 넣은 값을 바로잡는 일
    };
    let input = AdjustmentInput {
        occurred_on: reg.occurred_on.clone(),
        note: Some(note.clone()),
    };
    let ids = create_delta(conn, enrollment_id, &before, SRC_INDIVIDUAL, None, &input, items)?;

    if !ids.is_empty() {
        let after = repo::enrollment::get(conn, enrollment_id, items)?;
        let ws = workspace_of(conn, enrollment_id)?;
        let year_id = repo::enrollment::year_of_workspace(conn, ws)?;
        repo::change_log::write(
            conn,
            year_id,
            Some(ws),
            repo::change_log::ADJUST_ADD,
            Some(before.student_id),
            Some(before.department_id),
            &format!(
                "{} / {}",
                student_label(before.grade, &before.class_no, before.student_no, &before.name),
                before.dept_label
            ),
            &repo::enrollment::fees_text(items, &before.fees),
            &repo::enrollment::fees_text(items, &after.fees),
            &note,
        )?;
    }
    Ok(ids)
}

/// 부서 일괄 — 고른 학생의 금액을 부서 기준금액으로 맞추고, 원하면 그 차액을
/// 묶음 하나로 남긴다.
///
/// ## 기존 [부서금액 반영]을 그대로 쓴다
///
/// 금액을 맞추는 일은 v0.1.4 부터 있던 `enrollment::apply_fees` 가 한다. 대상도
/// 그대로 **수강중(ACTIVE)** 이고, 학생별·항목별로 골라 뺄 수 있다. 여기서는 그
/// 앞뒤로 '고치기 직전'을 떠 두었다가 차액을 기록하는 일만 더한다.
///
/// ## 반쪽 저장이 없다
///
/// 부르는 쪽이 `Db::write` 하나로 감싼다. 18명 가운데 15번째에서 실패하면 앞의
/// 14명도 함께 되돌려진다 — 금액도, 기록도, 묶음도.
///
/// ## 두 번 눌러도 두 번 생기지 않는다
///
/// 첫 요청에서 charge 가 이미 기준금액이 되었으므로, 같은 요청이 다시 오면
/// 미리보기 차이가 0 이 되어 고칠 칸이 없다. 묶음도 만들지 않는다.
pub fn create_batch(
    conn: &Connection,
    workspace_id: i64,
    department_id: i64,
    picks: &[FeePick],
    reg: &DeltaRegister,
    items: &[CostItem],
) -> AppResult<BatchResult> {
    let note = if reg.register {
        Some(required_note(reg)?)
    } else {
        None
    };

    // 지금 실제로 차이가 있는 칸만 받는다. 취소한 수강 건·다른 부서·이미 맞춘
    // 칸은 여기서 걸러진다 (`fee_diff` 가 ACTIVE 이고 금액이 다른 칸만 낸다).
    let diffs = repo::enrollment::fee_diff(conn, workspace_id, Some(department_id), items)?;
    let live: std::collections::HashSet<(i64, String)> = diffs
        .iter()
        .map(|d| (d.enrollment_id, d.item_code.clone()))
        .collect();
    let picks: Vec<FeePick> = picks
        .iter()
        .filter(|p| live.contains(&(p.enrollment_id, p.item_code.clone())))
        .cloned()
        .collect();

    let mut targets: Vec<i64> = picks.iter().map(|p| p.enrollment_id).collect();
    targets.sort_unstable();
    targets.dedup();

    let mut befores: HashMap<i64, Enrollment> = HashMap::new();
    for id in &targets {
        befores.insert(*id, repo::enrollment::get(conn, *id, items)?);
    }

    let reason = note.as_deref().unwrap_or("부서금액 반영");
    let applied = repo::enrollment::apply_fees(
        conn,
        workspace_id,
        Some(department_id),
        "SELECTED",
        &picks,
        reason,
        items,
    )?;

    let mut out = BatchResult {
        changed: applied.changed,
        batch_id: None,
        refunds: 0,
        additionals: 0,
        refund_total: 0,
        additional_total: 0,
    };
    let Some(note) = note else {
        return Ok(out); // 금액만 맞춘다
    };
    if targets.is_empty() {
        return Ok(out); // 고칠 것이 없었다 — 빈 묶음을 만들지 않는다
    }

    let occurred = occurred_of(
        conn,
        &AdjustmentInput {
            occurred_on: reg.occurred_on.clone(),
            note: None,
        },
    )?;
    let (dname, dclass) = repo::department::name_of(conn, department_id)?;
    let dlabel = dept_label(&dname, &dclass);
    conn.execute(
        "INSERT INTO billing_batch (workspace_id, department_id, occurred_on, note, dept_label)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![workspace_id, department_id, occurred, note, dlabel],
    )?;
    let batch_id = conn.last_insert_rowid();
    out.batch_id = Some(batch_id);

    let input = AdjustmentInput {
        occurred_on: Some(occurred.clone()),
        note: Some(note.clone()),
    };
    for id in &targets {
        let before = &befores[id];
        for aid in create_delta(conn, *id, before, SRC_DEPT_BATCH, Some(batch_id), &input, items)? {
            let (kind, total): (String, i64) = conn.query_row(
                "SELECT a.kind, COALESCE(SUM(m.amount), 0)
                   FROM billing_adjustment a
                   JOIN billing_adjustment_amount m ON m.adjustment_id = a.id
                  WHERE a.id = ?1",
                params![aid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if kind == REFUND {
                out.refunds += 1;
                out.refund_total += total;
            } else {
                out.additionals += 1;
                out.additional_total += total;
            }
        }
    }

    // 묶음 하나에 한 줄. 학생마다 줄을 남기지 않는다 — 학생별 금액 변화는
    // [부서금액 반영]이 이미 학생마다 `DEPT_APPLY` 로 남겼고, 누가 얼마를
    // 받는지는 묶음에 딸린 기록에서 그대로 읽힌다.
    let year_id = repo::enrollment::year_of_workspace(conn, workspace_id)?;
    repo::change_log::write(
        conn,
        year_id,
        Some(workspace_id),
        repo::change_log::ADJUST_BATCH_ADD,
        None,
        Some(department_id),
        &format!("{dlabel} · {occurred}"),
        "",
        &batch_summary(targets.len() as i64, &out),
        &note,
    )?;
    Ok(out)
}

fn batch_summary(students: i64, r: &BatchResult) -> String {
    let mut parts = vec![format!("학생 {students}명")];
    if r.refunds > 0 {
        parts.push(format!("환불 {}건 {}원", r.refunds, won(r.refund_total)));
    }
    if r.additionals > 0 {
        parts.push(format!("추가징수 {}건 {}원", r.additionals, won(r.additional_total)));
    }
    parts.join(" · ")
}

/// 이 작업공간의 부서 일괄 묶음. 합계는 딸린 기록을 더해 낸다.
pub fn batches(conn: &Connection, workspace_id: i64) -> AppResult<Vec<BillingBatch>> {
    let mut st = conn.prepare(
        "SELECT b.id, b.department_id, b.dept_label, b.occurred_on, b.note, b.created_at,
                COUNT(DISTINCT a.id), COUNT(DISTINCT a.student_id),
                COALESCE(SUM(CASE WHEN a.kind = 'REFUND' THEN m.amount END), 0),
                COALESCE(SUM(CASE WHEN a.kind = 'ADDITIONAL_CHARGE' THEN m.amount END), 0)
           FROM billing_batch b
           LEFT JOIN billing_adjustment a        ON a.batch_id = b.id
           LEFT JOIN billing_adjustment_amount m ON m.adjustment_id = a.id
          WHERE b.workspace_id = ?1
          GROUP BY b.id
          ORDER BY b.occurred_on DESC, b.id DESC",
    )?;
    let rows = st
        .query_map(params![workspace_id], |r| {
            Ok(BillingBatch {
                id: r.get(0)?,
                department_id: r.get(1)?,
                dept_label: r.get(2)?,
                occurred_on: r.get(3)?,
                note: r.get(4)?,
                created_at: r.get(5)?,
                count: r.get(6)?,
                students: r.get(7)?,
                refund_total: r.get(8)?,
                additional_total: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 부서 일괄 묶음을 한 번에 지운다.
///
/// ## 금액도 함께 되돌린다 (v0.1.9 후보 수정)
///
/// 묶음은 학생마다 charge 를 바꾸면서 만들어졌다. 기록만 지우면 환불은 사라졌는데
/// 깎인 금액이 남아 추적이 오히려 어려워진다. 그래서 딸린 기록마다 **그 기록의
/// 차액만큼** 되돌린다 — 묶음에서 뺐던 학생은 처음부터 기록이 없으니 그대로다.
///
/// 한 학생이라도 되돌릴 수 없으면(그 뒤 금액을 따로 다시 고쳤다면) **아무도
/// 되돌리지 않는다.** 부르는 쪽이 한 트랜잭션으로 감싼다.
///
/// 이력은 묶음에 한 줄(`ADJUST_BATCH_DELETE`)이고, 학생마다 바뀐 금액은 표준 금액
/// 쓰기가 `CHARGE_EDIT` 로 남긴다 — 누구의 얼마가 되돌려졌는지 그대로 읽힌다.
pub fn delete_batch(conn: &Connection, batch_id: i64) -> AppResult<usize> {
    #[allow(clippy::type_complexity)]
    let (workspace_id, department_id, dlabel, occurred, note): (i64, i64, String, String, String) =
        conn.query_row(
            "SELECT workspace_id, department_id, dept_label, occurred_on, note
               FROM billing_batch WHERE id = ?1",
            params![batch_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("일괄 조정을 찾지 못했습니다."))?;

    let summary = batches(conn, workspace_id)?
        .into_iter()
        .find(|b| b.id == batch_id)
        .map(|b| {
            format!(
                "학생 {}명 · 환불 {}원 · 추가징수 {}원",
                b.students,
                won(b.refund_total),
                won(b.additional_total)
            )
        })
        .unwrap_or_default();

    // 먼저 모두 따져 본다 — 하나라도 막히면 무엇이 막혔는지 한꺼번에 알려 준다.
    let items = repo::cost_items(conn)?;
    let ids = batch_members(conn, batch_id)?;
    let preview = delete_preview(conn, &ids, &items)?;
    if preview.blocked {
        let who: Vec<String> = preview
            .rows
            .iter()
            .filter(|r| r.blocked.is_some())
            .map(|r| r.student_label.clone())
            .collect();
        return Err(AppError::invalid(format!(
            "이 묶음 뒤에 금액이 따로 다시 변경된 학생이 있어 자동으로 되돌릴 수 없습니다 \
             ({}). 현재 금액을 확인한 후 직접 정정해 주세요. 아무것도 지우지 않았습니다.",
            who.join(" · ")
        )));
    }

    for id in &ids {
        revert_and_delete(conn, *id, &items, false)?; // 이력은 묶음에 한 줄로 남긴다
    }
    let n = ids.len();
    conn.execute("DELETE FROM billing_batch WHERE id = ?1", params![batch_id])?;

    let year_id = repo::enrollment::year_of_workspace(conn, workspace_id)?;
    repo::change_log::write(
        conn,
        year_id,
        Some(workspace_id),
        repo::change_log::ADJUST_BATCH_DELETE,
        None,
        Some(department_id),
        &format!("{dlabel} · {occurred}"),
        &summary,
        "기록 삭제 · 대상 학생 금액 원복",
        &note,
    )?;
    Ok(n)
}

/// 발생 유형을 사람이 읽는 말로. 화면과 Excel 이 같은 말을 쓴다.
///
/// `LEGACY` 는 '기존 기록'이다 — 이 갱신 이전 기록을 '수강취소'라고 단정하지 않는다.
pub fn source_label(source: &str) -> &'static str {
    match source {
        SRC_ENROLL => "중도추가",
        SRC_CANCEL => "수강취소",
        SRC_DEPT_BATCH => "부서일괄",
        SRC_INDIVIDUAL => "개별조정",
        _ => "기존 기록",
    }
}
