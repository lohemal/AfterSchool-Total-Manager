//! 정산 상세내역 — 학생 × 부서 × 항목 × 재원 (v0.1.10).
//!
//! 수익자 · 방과후 이용권 · 자유수강권 세 화면의 **[상세내역 받기]** 가 쓴다.
//!
//! ## 새로 셈하지 않는다
//!
//! 지원금을 다시 계산하거나 `charge` 에서 배분하지 않는다. 학생을 눌렀을 때
//! 뜨는 '정산 상세'와 **같은 함수**([`settle::student_allocs`])로 저장된 정산
//! 스냅샷의 배분액을 읽고, 그것을 부서 한 줄 · 항목 한 칸으로 펼칠 뿐이다.
//! 그래서 화면의 상세와 파일의 상세가 어긋날 길이 없다.
//!
//! ## 세 종류는 재원만 다르다
//!
//! | 종류 | 묶음 | 재원 |
//! |---|---|---|
//! | 수익자 | (하나) | `SELF_PAY` + `VOUCHER_OVER` — 수익자 화면과 같은 범위 |
//! | 방과후 이용권 | 지원금 · 초과금 | `VOUCHER` · `VOUCHER_OVER` |
//! | 자유수강권 | (하나) | `FREE_VOUCHER` |
//!
//! 자유수강권에는 **초과금 재원이 없다.** 한도를 다 쓴 뒤 남은 금액은 이용권
//! 자격이 있으면 `VOUCHER_OVER`, 없으면 `SELF_PAY` 가 되어 수익자 쪽으로 간다
//! (`domain::settle` 3번). 없는 '자유수강권 초과금'을 여기서 만들지 않는다.
//!
//! `VOUCHER_OVER` 는 수익자 상세와 이용권 상세(초과금)에 **함께** 나온다. 같은
//! 돈을 두 관점으로 보는 것이므로 두 파일을 더하면 안 된다.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::domain::class_no;
use crate::error::{AppError, AppResult};
use crate::model::{CostItem, Fee};
use crate::repo::settle;

/// 어느 화면의 상세인가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    SelfPay,
    Voucher,
    FreeVoucher,
}

/// 열 묶음 하나 — 이름과, 그 묶음에 들어가는 재원.
pub struct DetailGroup {
    /// 2단 머리글의 윗칸. 묶음이 하나뿐인 종류는 비워 둔다.
    pub title: &'static str,
    pub funds: &'static [&'static str],
}

/// 종류마다 다른 것은 이것뿐이다. 나머지(행 단위 · 피벗 · 서식)는 모두 같다.
pub struct DetailSpec {
    pub groups: &'static [DetailGroup],
    /// 파일 이름 앞머리
    pub base: &'static str,
    pub sheet: &'static str,
}

const SELF_PAY: DetailSpec = DetailSpec {
    groups: &[DetailGroup {
        title: "",
        funds: &["SELF_PAY", "VOUCHER_OVER"],
    }],
    base: "수익자_상세내역",
    sheet: "수익자 상세내역",
};

const VOUCHER: DetailSpec = DetailSpec {
    groups: &[
        DetailGroup {
            title: "지원금",
            funds: &["VOUCHER"],
        },
        DetailGroup {
            title: "초과금",
            funds: &["VOUCHER_OVER"],
        },
    ],
    base: "방과후이용권_상세내역",
    sheet: "이용권 상세내역",
};

const FREE_VOUCHER: DetailSpec = DetailSpec {
    groups: &[DetailGroup {
        title: "",
        funds: &["FREE_VOUCHER"],
    }],
    base: "자유수강권_상세내역",
    sheet: "자유수강권 상세내역",
};

impl DetailKind {
    /// 기존 Excel 내려받기와 같은 이름을 쓴다 (`self_pay` | `voucher` | `free_voucher`).
    pub fn from_code(code: &str) -> AppResult<Self> {
        match code {
            "self_pay" => Ok(Self::SelfPay),
            "voucher" => Ok(Self::Voucher),
            "free_voucher" => Ok(Self::FreeVoucher),
            _ => Err(AppError::invalid("알 수 없는 내려받기 종류입니다.")),
        }
    }

    pub fn spec(self) -> &'static DetailSpec {
        match self {
            Self::SelfPay => &SELF_PAY,
            Self::Voucher => &VOUCHER,
            Self::FreeVoucher => &FREE_VOUCHER,
        }
    }
}

/// 한 묶음의 항목별 금액. `fees` 는 비용항목 차례이고 없는 항목은 0원이다.
#[derive(Debug, Clone)]
pub struct GroupAmounts {
    pub fees: Vec<Fee>,
    pub total: i64,
}

/// 학생 1명 × 부서 1개 = 한 줄.
#[derive(Debug, Clone)]
pub struct DetailRow {
    pub student_id: i64,
    pub grade: i64,
    pub class_no: String,
    pub student_no: i64,
    pub name: String,
    pub department_id: i64,
    pub dept_label: String,
    /// `spec().groups` 와 같은 차례
    pub groups: Vec<GroupAmounts>,
    pub total: i64,
}

struct StudentInfo {
    grade: i64,
    class_no: String,
    student_no: i64,
    name: String,
}

/// 최신 정산의 상세내역. 정산이 없으면 빈 목록이다.
///
/// 정산이 최신인지는 여기서 따지지 않는다 — 화면 목록과 같은 자리에서 읽는다.
/// 파일로 내보낼 때만 `excel::settle_detail` 이 기존 Excel 과 같은 규칙으로 막는다.
pub fn rows(
    conn: &Connection,
    workspace_id: i64,
    kind: DetailKind,
    items: &[CostItem],
) -> AppResult<Vec<DetailRow>> {
    let Some((sid, _)) = settle::latest_id(conn, workspace_id)? else {
        return Ok(Vec::new());
    };
    let spec = kind.spec();
    let wanted: HashSet<&str> = spec.groups.iter().flat_map(|g| g.funds.iter().copied()).collect();

    // 이 종류의 재원이 한 푼이라도 있는 학생만. 재원 코드는 위 상수에서만 온다.
    let mut ids: Vec<i64> = Vec::new();
    {
        let mut st = conn.prepare(
            "SELECT DISTINCT student_id, fund FROM settlement_alloc WHERE settlement_id = ?1",
        )?;
        let mut seen = HashSet::new();
        for row in st.query_map(params![sid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, fund) = row?;
            if wanted.contains(fund.as_str()) && seen.insert(id) {
                ids.push(id);
            }
        }
    }

    let students = student_info(conn, &ids)?;
    let depts = dept_names(conn, sid)?;

    let mut out: Vec<DetailRow> = Vec::new();
    for id in &ids {
        let Some(info) = students.get(id) else {
            continue; // 정산 뒤 학생이 지워졌다 — 화면 목록(JOIN student)에도 나오지 않는다
        };
        // 학생 정산 상세 팝업과 같은 함수다
        let allocs = settle::student_allocs(conn, workspace_id, *id)?;

        // (부서 → 묶음 → 항목) 금액. 같은 칸이 여러 줄이면 모두 더한다.
        let mut acc: HashMap<i64, Vec<HashMap<String, i64>>> = HashMap::new();
        let mut labels: HashMap<i64, String> = HashMap::new();
        let mut dept_order: Vec<i64> = Vec::new();
        for a in &allocs {
            let Some(g) = spec.groups.iter().position(|g| g.funds.contains(&a.fund.as_str())) else {
                continue; // 다른 화면의 재원
            };
            let slot = acc.entry(a.department_id).or_insert_with(|| {
                dept_order.push(a.department_id);
                vec![HashMap::new(); spec.groups.len()]
            });
            *slot[g].entry(a.item_code.clone()).or_insert(0) += a.amount;
            labels.entry(a.department_id).or_insert_with(|| a.dept_label.clone());
        }

        for dept_id in dept_order {
            let maps = &acc[&dept_id];
            let groups: Vec<GroupAmounts> = maps.iter().map(|m| amounts_of(items, m)).collect();
            let total = groups.iter().map(|g| g.total).sum();
            if total == 0 {
                continue; // 비용항목에 없는 항목만 있던 칸 — 화면 합계에도 들어가지 않는다
            }
            out.push(DetailRow {
                student_id: *id,
                grade: info.grade,
                class_no: info.class_no.clone(),
                student_no: info.student_no,
                name: info.name.clone(),
                department_id: dept_id,
                dept_label: labels.remove(&dept_id).unwrap_or_default(),
                groups,
                total,
            });
        }
    }

    // 학년 · 반 · 번호 · 이름 · 부서. 반은 숫자 반이 먼저 오는 반 정렬 규칙을,
    // 부서는 정산 상세와 같은 '부서명 → 반 이름' 차례를 쓴다. 동점은 id 로 가른다.
    let empty = (String::new(), String::new());
    out.sort_by(|a, b| {
        let da = depts.get(&a.department_id).unwrap_or(&empty);
        let db = depts.get(&b.department_id).unwrap_or(&empty);
        a.grade
            .cmp(&b.grade)
            .then(class_no::cmp(&a.class_no, &b.class_no))
            .then(a.student_no.cmp(&b.student_no))
            .then(a.name.cmp(&b.name))
            .then(a.student_id.cmp(&b.student_id))
            .then(da.cmp(db))
            .then(a.department_id.cmp(&b.department_id))
    });
    Ok(out)
}

fn amounts_of(items: &[CostItem], map: &HashMap<String, i64>) -> GroupAmounts {
    let fees: Vec<Fee> = items
        .iter()
        .map(|it| Fee {
            item_code: it.code.clone(),
            amount: map.get(&it.code).copied().unwrap_or(0),
        })
        .collect();
    let total = fees.iter().map(|f| f.amount).sum();
    GroupAmounts { fees, total }
}

fn student_info(conn: &Connection, ids: &[i64]) -> AppResult<HashMap<i64, StudentInfo>> {
    let mut st = conn.prepare("SELECT grade, class_no, student_no, name FROM student WHERE id = ?1")?;
    let mut out = HashMap::new();
    for id in ids {
        let mut rows = st.query(params![id])?;
        if let Some(r) = rows.next()? {
            out.insert(
                *id,
                StudentInfo {
                    grade: r.get(0)?,
                    class_no: r.get(1)?,
                    student_no: r.get(2)?,
                    name: r.get(3)?,
                },
            );
        }
    }
    Ok(out)
}

/// 정렬용 (부서명, 반 이름). 정산 상세와 같은 열쇠다.
fn dept_names(conn: &Connection, settlement_id: i64) -> AppResult<HashMap<i64, (String, String)>> {
    let mut st = conn.prepare(
        "SELECT DISTINCT d.id, d.name, d.class_name
           FROM settlement_alloc a JOIN department d ON d.id = a.department_id
          WHERE a.settlement_id = ?1",
    )?;
    let rows = st
        .query_map(params![settlement_id], |r| {
            Ok((r.get::<_, i64>(0)?, (r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        })?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}
