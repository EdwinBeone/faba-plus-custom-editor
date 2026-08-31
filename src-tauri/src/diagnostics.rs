use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 1_500_000;

#[derive(Debug)]
pub struct DiagnosticLogger {
    path: PathBuf,
    write_lock: Mutex<()>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub content: String,
    pub path: String,
}

impl DiagnosticLogger {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    pub fn info(&self, event: &str, details: impl AsRef<str>) {
        let _ = self.write("INFO", event, details.as_ref());
    }

    pub fn error(&self, event: &str, details: impl AsRef<str>) {
        let _ = self.write("ERROR", event, details.as_ref());
    }

    pub fn report(&self) -> Result<DiagnosticReport> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Le verrou du journal est indisponible."))?;
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error).context("Impossible de lire le journal technique."),
        };
        Ok(DiagnosticReport {
            content,
            path: self.path.to_string_lossy().into_owned(),
        })
    }

    pub fn clear(&self) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Le verrou du journal est indisponible."))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, "").context("Impossible d'effacer le journal technique.")
    }

    fn write(&self, level: &str, event: &str, details: &str) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Le verrou du journal est indisponible."))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        self.rotate_if_needed()?;
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let safe_event = single_line(event);
        let safe_details = single_line(details);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .context("Impossible d'ouvrir le journal technique.")?;
        writeln!(file, "{timestamp} [{level}] {safe_event} | {safe_details}")?;
        file.flush()?;
        Ok(())
    }

    fn rotate_if_needed(&self) -> Result<()> {
        let Ok(metadata) = fs::metadata(&self.path) else {
            return Ok(());
        };
        if metadata.len() < MAX_LOG_BYTES {
            return Ok(());
        }
        let previous = self.path.with_extension("previous.log");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&self.path, previous)?;
        Ok(())
    }
}

fn single_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_and_clears_diagnostic_entries() {
        let temp = tempdir().unwrap();
        let logger = DiagnosticLogger::new(temp.path().join("diagnostics.log"));
        logger.info("figure.save.start", "figure=K0742 tracks=2");
        logger.error("figure.save.error", "Access denied\n(os error 5)");

        let report = logger.report().unwrap();
        assert!(report.content.contains("figure.save.start"));
        assert!(report.content.contains("Access denied (os error 5)"));
        assert_eq!(report.content.lines().count(), 2);

        logger.clear().unwrap();
        assert!(logger.report().unwrap().content.is_empty());
    }
}
