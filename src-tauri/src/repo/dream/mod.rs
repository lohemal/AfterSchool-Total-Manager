//! 방과후드림 결제관리 — 저장소 (P1: 기반만).
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

pub mod amount;
pub mod course;
pub mod identity;
pub mod job;
pub mod priority;
pub mod setting;

#[cfg(test)]
mod dream_tests;
