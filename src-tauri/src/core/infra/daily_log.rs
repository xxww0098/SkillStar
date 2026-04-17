use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

use super::paths;

fn log_path(scope: &str) -> std::path::PathBuf {
    let date = Local::now().format("%Y-%m-%d");
    paths::logs_dir().join(format!("{scope}-{date}.log"))
}

pub fn append_ndjson_line(scope: &str, line: &str) {
    let path = log_path(scope);
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };

    let _ = writeln!(file, "{line}");
}
