//! 방과후드림 결제관리 — 저장소.
//!
//! 이 모듈은 `dream_*` 표와 `app_setting` 의 `dream.*` 열쇠에만 쓴다. 정산 ·
//! 배분 · 한도 · charge · 추가징수/환불 · 기존 우선순위는 **읽기만** 한다.
//! 그래서 여기서 무엇을 저장하든 작업공간 · 학년도 자료판이 오르지 않고, 정산이
//! 낡음이 되지 않는다(시험이 지문으로 확인한다).
//!
//! | 모듈 | 하는 일 |
//! |---|---|
//! | `identity` | 학생 개인참조번호 · 생년월일 (암호문 + 찾기용 HMAC) |
//! | `setting` | 기관코드 |
//! | `course` | 강좌명코드 ↔ 부서 (작업공간 단위) |
//! | `priority` | 방과후드림 전용 부서 · 부서별 항목 우선순위 |
//! | `job` | 처리 이력 (30일) |
//! | `amount` | 최신 유효 정산에서 학생별 기준 금액 읽기 |
//! | `workspace_setting` | 작업공간별 결제 기준월 (P2) |
//! | `file_check` | 결제파일 검사 · 읽기 전용 미리보기 (P2) |
//! | `reset` | 개인정보 키 상태 · 초기화 (P2) |
//! | `matching` | 학생 · 강좌 · 항목 · 금액 정확 매칭, 학생 단위 판정 (P3) |
//! | `session` | 매칭 · 검증 작업 시작 · 다시 검증 · 확인 (P3) |
//! | `allocation` | 방과후드림 우선순위 화면 · 지원금 재배분 · 검산 · 결과 무효화 (P4) |
//! | `export` | 검증된 배분 결과를 원본 XLSX 에 옮긴 결과 파일 · 성공 기록 (P5) |

pub mod allocation;
pub mod amount;
pub mod course;
pub mod export;
pub mod file_check;
pub mod identity;
pub mod job;
pub mod matching;
pub mod priority;
pub mod reset;
pub mod session;
pub mod setting;
pub mod workspace_setting;

#[cfg(test)]
mod dream_tests;
