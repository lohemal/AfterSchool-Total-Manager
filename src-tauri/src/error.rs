//! 앱 전역 오류 타입.
//!
//! 화면에 그대로 보여줄 한국어 문장(`message`)과 개발자용 원문(`detail`)을 나눈다.
//! Rust 쪽에서는 `panic!`을 쓰지 않는다 — 한 화면의 오류가 앱 전체를 끄면 안 된다.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// 사용자 입력이 잘못된 경우 — 화면에 그대로 보여준다.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        // 제약조건 위반은 사용자에게 뜻이 통하는 문장으로 바꿔 준다.
        let text = e.to_string();
        if text.contains("UNIQUE constraint failed") {
            let msg = if text.contains("student_key_uq") {
                "같은 학년·반·번호의 학생이 이미 있습니다."
            } else if text.contains("department_uq") {
                "같은 부서명·반명이 이미 있습니다."
            } else if text.contains("enrollment_active_uq") {
                "같은 학생이 같은 부서를 이미 수강 중입니다."
            } else if text.contains("workspace_seq_uq") {
                "작업공간 순서가 중복되었습니다."
            } else {
                "이미 등록된 자료입니다."
            };
            return AppError::new("CONFLICT", msg).detail(text);
        }
        if text.contains("CHECK constraint failed") {
            return AppError::invalid("입력값이 허용 범위를 벗어났습니다.").detail(text);
        }
        AppError::new("DB", "자료를 처리하지 못했습니다.").detail(text)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new("IO", "파일을 읽거나 쓰지 못했습니다.").detail(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("JSON", "자료 형식을 변환하지 못했습니다.").detail(e.to_string())
    }
}

impl From<calamine::Error> for AppError {
    fn from(e: calamine::Error) -> Self {
        AppError::new("EXCEL_READ", "Excel 파일을 읽지 못했습니다.").detail(e.to_string())
    }
}

impl From<calamine::XlsxError> for AppError {
    fn from(e: calamine::XlsxError) -> Self {
        AppError::new("EXCEL_READ", "Excel 파일을 읽지 못했습니다.").detail(e.to_string())
    }
}

impl From<rust_xlsxwriter::XlsxError> for AppError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        AppError::new("EXCEL_WRITE", "Excel 파일을 만들지 못했습니다.").detail(e.to_string())
    }
}
