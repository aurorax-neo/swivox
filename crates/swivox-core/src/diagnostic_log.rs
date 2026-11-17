use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};

static TEST_LOG_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

// 上限压到 24 MB：桥与请求的高频事件叠一天就能写到数十 MB，超过这个量级之后再往下翻
// 也找不到有用的线索，只会让每次打开日志都变慢。
const MAX_DIAGNOSTIC_LOG_BYTES: u64 = 24 * 1024 * 1024;
const COMPACTED_DIAGNOSTIC_LOG_BYTES: u64 = 6 * 1024 * 1024;
/// 文件体积只需要隔一会儿看一次，不必每行 stat。
const LOG_SIZE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// 压缩失败后的首次退避，之后每次翻倍，封顶 `COMPACT_RETRY_MAX_DELAY`。
const COMPACT_RETRY_BASE_DELAY: Duration = Duration::from_secs(30);
const COMPACT_RETRY_MAX_DELAY: Duration = Duration::from_secs(10 * 60);
/// 旧版本用 `settings::atomic_write` 压缩时临时文件固定叫 `swivox.log.tmp`；
/// 进程在替换重试中途被结束时会留下残留。修改时间超过这个值才清。
const LEGACY_TEMP_STALE_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
struct DiagnosticRecord {
    timestamp_ms: u64,
    pid: u32,
    event: String,
    detail: Value,
}

/// 进程内复用的写句柄：单次 `write_all` 原子写入整行（含 `\n`），避免并发写入交错半行。
struct LogSink {
    path: PathBuf,
    file: Option<std::fs::File>,
    next_size_check: Instant,
    compact_failures: u32,
    max_bytes: u64,
    compacted_bytes: u64,
}

impl LogSink {
    fn new() -> Self {
        Self::with_limits(MAX_DIAGNOSTIC_LOG_BYTES, COMPACTED_DIAGNOSTIC_LOG_BYTES)
    }

    fn with_limits(max_bytes: u64, compacted_bytes: u64) -> Self {
        Self {
            path: PathBuf::new(),
            file: None,
            next_size_check: Instant::now(),
            compact_failures: 0,
            max_bytes,
            compacted_bytes,
        }
    }

    fn close(&mut self) -> std::io::Result<()> {
        self.path = PathBuf::new();
        match self.file.take() {
            Some(mut file) => file.flush(),
            None => Ok(()),
        }
    }

    fn flush_now(&mut self) -> std::io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }

    fn open(&mut self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        self.file = Some(file);
        self.path = path.to_path_buf();
        self.next_size_check = Instant::now() + LOG_SIZE_CHECK_INTERVAL;
        Ok(())
    }

    /// 追加一行（含结尾换行）。`line` 必须带 `\n`。
    fn write_line(&mut self, path: &Path, line: &[u8]) -> std::io::Result<()> {
        let path_changed = self.file.is_none() || self.path != path;
        if path_changed || Instant::now() >= self.next_size_check {
            self.reopen(path, path_changed)?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("诊断日志句柄未就绪"))?;
        file.write_all(line)
    }

    fn reopen(&mut self, path: &Path, path_changed: bool) -> std::io::Result<()> {
        let _ = self.close();
        if path_changed {
            self.compact_failures = 0;
            remove_stale_legacy_temp(path);
        }
        let compact_result = compact_diagnostic_log(path, self.max_bytes, self.compacted_bytes);
        self.open(path)?;
        match compact_result {
            Ok(()) => self.compact_failures = 0,
            Err(error) => {
                self.compact_failures = self.compact_failures.saturating_add(1);
                let delay = compact_retry_delay(self.compact_failures);
                self.next_size_check = Instant::now() + delay;
                let record = DiagnosticRecord {
                    timestamp_ms: now_ms(),
                    pid: std::process::id(),
                    event: "diagnostic_log.compact_failed".to_string(),
                    detail: json!({
                        "error": error.to_string(),
                        "consecutive_failures": self.compact_failures,
                        "retry_in_ms": delay.as_millis() as u64,
                    }),
                };
                if let (Some(file), Ok(mut line)) =
                    (self.file.as_mut(), serde_json::to_string(&record))
                {
                    line.push('\n');
                    let _ = file.write_all(line.as_bytes());
                }
            }
        }
        Ok(())
    }
}

fn compact_retry_delay(failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(16);
    COMPACT_RETRY_BASE_DELAY
        .saturating_mul(1u32 << shift)
        .min(COMPACT_RETRY_MAX_DELAY)
}

fn remove_stale_legacy_temp(path: &Path) {
    let Some(name) = path.file_name() else {
        return;
    };
    let mut legacy_name = name.to_os_string();
    legacy_name.push(".tmp");
    let legacy = path.with_file_name(legacy_name);
    let stale = std::fs::metadata(&legacy)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= LEGACY_TEMP_STALE_AFTER);
    if stale {
        let _ = std::fs::remove_file(&legacy);
    }
}

fn log_sink() -> &'static Mutex<LogSink> {
    LOG_SINK.get_or_init(|| Mutex::new(LogSink::new()))
}

static LOG_SINK: OnceLock<Mutex<LogSink>> = OnceLock::new();

pub fn append_diagnostic_log(event: &str, detail: impl Serialize) -> std::io::Result<()> {
    let path = diagnostic_log_path();
    let detail = serde_json::to_value(detail).unwrap_or_else(|error| {
        json!({
            "serialization_error": error.to_string()
        })
    });
    let record = DiagnosticRecord {
        timestamp_ms: now_ms(),
        pid: std::process::id(),
        event: event.to_string(),
        detail,
    };
    let line = serde_json::to_string(&record).unwrap_or_else(|error| {
        json!({
            "timestamp_ms": now_ms(),
            "pid": std::process::id(),
            "event": "diagnostic_log.serialization_failed",
            "detail": {
                "message": error.to_string()
            }
        })
        .to_string()
    });
    let mut buffer = String::with_capacity(line.len() + 1);
    buffer.push_str(&line);
    buffer.push('\n');
    let mut sink = log_sink()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sink.write_line(&path, buffer.as_bytes())
}

pub fn flush_diagnostic_log() -> std::io::Result<()> {
    let mut sink = log_sink()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sink.flush_now()
}

pub fn clear_diagnostic_log() -> std::io::Result<()> {
    let path = diagnostic_log_path();
    clear_diagnostic_log_path(&path)
}

fn clear_diagnostic_log_path(path: &Path) -> std::io::Result<()> {
    {
        let mut sink = log_sink()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sink.close()?;
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn diagnostic_log_path() -> PathBuf {
    if let Some(lock) = TEST_LOG_PATH.get() {
        if let Ok(guard) = lock.lock() {
            if let Some(path) = &*guard {
                return path.clone();
            }
        }
    }
    crate::paths::default_diagnostic_log_path()
}

#[doc(hidden)]
pub fn set_diagnostic_log_path_for_tests(path: Option<PathBuf>) {
    swap_diagnostic_log_path_for_tests(path);
}

#[doc(hidden)]
pub fn swap_diagnostic_log_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    let test_path = TEST_LOG_PATH.get_or_init(|| Mutex::new(None));
    let replaced = {
        let mut guard = test_path.lock().expect("test log path lock poisoned");
        std::mem::replace(&mut *guard, path.clone())
    };
    let mut sink = log_sink()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = sink.close();
    replaced
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn compact_diagnostic_log(
    path: &Path,
    max_bytes: u64,
    compacted_bytes: u64,
) -> std::io::Result<()> {
    let len = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if len <= max_bytes {
        return Ok(());
    }

    let keep = compacted_bytes.min(len);
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(len - keep))?;
    let mut tail = Vec::with_capacity(keep as usize);
    file.read_to_end(&mut tail)?;
    drop(file);
    if len > keep {
        if let Some(pos) = tail.iter().position(|byte| *byte == b'\n') {
            tail.drain(..=pos);
        }
    }

    replace_log_contents(path, &tail)
}

fn replace_log_contents(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut temp_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    temp_name.push(format!(".{}.compact.tmp", std::process::id()));
    let temp_path = path.with_file_name(temp_name);
    let result =
        std::fs::write(&temp_path, contents).and_then(|()| std::fs::rename(&temp_path, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_diagnostic_log_keeps_tail_and_drops_partial_first_line() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("swivox.log");
        std::fs::write(&path, "line-1\nline-2\nline-3\nline-4\n").unwrap();

        compact_diagnostic_log(&path, 12, 16).unwrap();

        let contents = std::fs::read_to_string(path).unwrap();
        assert_eq!(contents, "line-3\nline-4\n");
    }

    #[test]
    fn clear_diagnostic_log_ignores_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing.log");

        clear_diagnostic_log_path(&path).unwrap();
    }

    #[test]
    fn log_sink_keeps_each_record_on_its_own_line() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("swivox.log");
        let mut sink = LogSink::new();
        for index in 0..10 {
            sink.write_line(&path, format!("{{\"i\":{index}}}\n").as_bytes())
                .unwrap();
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.lines().count(), 10);
        assert!(
            contents
                .lines()
                .all(|line| serde_json::from_str::<Value>(line).is_ok()),
            "{contents}"
        );
    }

    fn write_oversized_log(path: &Path, lines: usize) {
        let body = (0..lines)
            .map(|index| format!("{{\"old\":{index}}}\n"))
            .collect::<String>();
        std::fs::write(path, body).unwrap();
    }

    fn compact_temp_path(path: &Path) -> PathBuf {
        let mut name = path.file_name().unwrap().to_os_string();
        name.push(format!(".{}.compact.tmp", std::process::id()));
        path.with_file_name(name)
    }

    #[test]
    fn log_sink_backs_off_when_compaction_fails_and_keeps_appending() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("swivox.log");
        write_oversized_log(&path, 20);
        std::fs::create_dir(compact_temp_path(&path)).unwrap();
        let mut sink = LogSink::with_limits(64, 32);

        let before = Instant::now();
        for index in 0..5 {
            sink.write_line(&path, format!("{{\"new\":{index}}}\n").as_bytes())
                .unwrap();
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.starts_with("{\"old\":0}\n"), "{contents}");
        for index in 0..5 {
            assert!(
                contents.contains(&format!("{{\"new\":{index}}}")),
                "{contents}"
            );
        }
        assert_eq!(
            contents.matches("diagnostic_log.compact_failed").count(),
            1,
            "{contents}"
        );
        assert_eq!(sink.compact_failures, 1);
        assert!(sink.next_size_check >= before + COMPACT_RETRY_BASE_DELAY);

        std::fs::remove_dir(compact_temp_path(&path)).unwrap();
        sink.next_size_check = Instant::now();
        sink.write_line(&path, b"{\"new\":6}\n").unwrap();
        assert_eq!(sink.compact_failures, 0);
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("{\"old\":0}"), "{contents}");
        assert!(contents.ends_with("{\"new\":6}\n"), "{contents}");
    }
}
