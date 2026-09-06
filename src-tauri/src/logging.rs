//! 파일 로그 (요구사항 §16).
//!
//! 업데이트 · 마이그레이션 · 백업 · 복원처럼 나중에 원인을 되짚어야 하는 일만
//! 남긴다. **개인정보는 적지 않는다** — 학생 이름이나 금액 대신 건수와 파일
//! 이름만 남긴다. 사용자가 오류를 알려 줄 때 이 파일을 그대로 보내도 안전해야
//! 하기 때문이다.
//!
//! 파일은 날짜별로 하나(`afterschool-20260906.log`)이고 30일이 지나면 지운다.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use log::{Level, LevelFilter, Metadata, Record};

const KEEP_DAYS: i64 = 30;

struct FileLogger {
    dir: PathBuf,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = chrono::Local::now();
        let line = format!(
            "{} [{}] {}\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.args()
        );
        // 로그를 쓰지 못하는 것 때문에 업무가 멈추면 안 된다 — 조용히 넘어간다.
        let path = self.dir.join(format!("afterschool-{}.log", now.format("%Y%m%d")));
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

/// 로그 폴더를 만들고 로거를 붙인다. 두 번 불러도 문제가 없다.
pub fn init(dir: &Path) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    prune(dir);
    let logger = FileLogger {
        dir: dir.to_path_buf(),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

/// 30일이 지난 로그를 지운다.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = chrono::Local::now().naive_local().date() - chrono::Duration::days(KEEP_DAYS);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(stamp) = name
            .strip_prefix("afterschool-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        if let Ok(date) = chrono::NaiveDate::parse_from_str(stamp, "%Y%m%d") {
            if date < cutoff {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
