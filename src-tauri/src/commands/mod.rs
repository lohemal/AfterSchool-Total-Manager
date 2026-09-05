//! `#[tauri::command]` 계층. **로직을 두지 않는다** — 입력을 받아 repo/domain에 넘기고
//! 결과를 돌려줄 뿐이다. 그래야 업무 규칙이 화면과 분리된 채로 테스트된다.

pub mod app;
pub mod department;
pub mod eligibility;
pub mod excel;
pub mod policy;
pub mod student;
pub mod year;
