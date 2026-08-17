use crate::domain::{CardKind, CardSnapshot};
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct LibraryDatabase {
    path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCard {
    pub root_path: String,
    pub label: String,
    pub kind: String,
    pub last_seen_at: String,
}

impl LibraryDatabase {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn initialize(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS cards (
               root_path TEXT PRIMARY KEY,
               label TEXT NOT NULL,
               kind TEXT NOT NULL,
               last_seen_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS figures (
               root_path TEXT NOT NULL,
               figure_id TEXT NOT NULL,
               custom_name TEXT,
               last_seen_at TEXT NOT NULL,
               track_count INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (root_path, figure_id),
               FOREIGN KEY (root_path) REFERENCES cards(root_path) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS track_labels (
               root_path TEXT NOT NULL,
               figure_id TEXT NOT NULL,
               track_index INTEGER NOT NULL,
               label TEXT NOT NULL,
               PRIMARY KEY (root_path, figure_id, track_index),
               FOREIGN KEY (root_path, figure_id) REFERENCES figures(root_path, figure_id) ON DELETE CASCADE
             );",
        )?;
        Ok(())
    }

    pub fn sync_snapshot(&self, snapshot: &CardSnapshot) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        let root = &snapshot.root_path;
        let label = Path::new(root)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Carte FABA+");
        transaction.execute(
            "INSERT INTO cards(root_path, label, kind, last_seen_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(root_path) DO UPDATE SET kind=excluded.kind, last_seen_at=excluded.last_seen_at",
            params![root, label, kind_name(snapshot.kind), now],
        )?;
        for figure in &snapshot.figures {
            transaction.execute(
                "INSERT INTO figures(root_path, figure_id, last_seen_at, track_count)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_path, figure_id) DO UPDATE SET
                   last_seen_at=excluded.last_seen_at,
                   track_count=excluded.track_count",
                params![root, figure.id, now, figure.tracks.len() as i64],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn decorate_snapshot(&self, snapshot: &mut CardSnapshot) -> Result<()> {
        let connection = self.connection()?;
        for figure in &mut snapshot.figures {
            figure.custom_name = connection
                .query_row(
                    "SELECT custom_name FROM figures WHERE root_path=?1 AND figure_id=?2",
                    params![snapshot.root_path, figure.id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .unwrap_or(None);

            let mut statement = connection.prepare(
                "SELECT track_index, label FROM track_labels WHERE root_path=?1 AND figure_id=?2",
            )?;
            let labels = statement
                .query_map(params![snapshot.root_path, figure.id], |row| {
                    Ok((row.get::<_, u16>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|row| row.ok())
                .collect::<HashMap<_, _>>();
            for track in &mut figure.tracks {
                if let Some(label) = labels.get(&track.index) {
                    track.label.clone_from(label);
                }
            }
        }
        Ok(())
    }

    pub fn set_figure_name(&self, root: &str, figure_id: &str, name: Option<&str>) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE figures SET custom_name=?3 WHERE root_path=?1 AND figure_id=?2",
            params![root, figure_id, name],
        )?;
        Ok(())
    }

    pub fn set_track_labels(&self, root: &str, figure_id: &str, labels: &[String]) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM track_labels WHERE root_path=?1 AND figure_id=?2",
            params![root, figure_id],
        )?;
        for (index, label) in labels.iter().enumerate() {
            transaction.execute(
                "INSERT INTO track_labels(root_path, figure_id, track_index, label) VALUES (?1, ?2, ?3, ?4)",
                params![root, figure_id, index as i64, label],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_figure(&self, root: &str, figure_id: &str) -> Result<()> {
        self.connection()?.execute(
            "DELETE FROM figures WHERE root_path=?1 AND figure_id=?2",
            params![root, figure_id],
        )?;
        Ok(())
    }

    pub fn recent_cards(&self) -> Result<Vec<RecentCard>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT root_path, label, kind, last_seen_at FROM cards ORDER BY last_seen_at DESC LIMIT 8",
        )?;
        let cards = statement
            .query_map([], |row| {
                Ok(RecentCard {
                    root_path: row.get(0)?,
                    label: row.get(1)?,
                    kind: row.get(2)?,
                    last_seen_at: row.get(3)?,
                })
            })?
            .filter_map(|row| row.ok())
            .collect();
        Ok(cards)
    }
}

fn kind_name(kind: CardKind) -> &'static str {
    match kind {
        CardKind::FabaPlus => "FABA+",
        CardKind::LegacyFaba => "FABA classique",
        CardKind::Empty => "Dossier vide",
        CardKind::Unknown => "Inconnu",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::scan_card;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn remembers_names_without_writing_them_to_the_card() {
        let temp = tempdir().unwrap();
        let db = LibraryDatabase::new(temp.path().join("library.sqlite3"));
        db.initialize().unwrap();
        fs::create_dir_all(temp.path().join("card/K0123")).unwrap();
        fs::write(temp.path().join("card/K0123/CP00.faba"), b"audio").unwrap();
        fs::write(
            temp.path().join("card/K0123/info"),
            r#"{"totalTracks":1,"characterDir":"02190530012300"}"#,
        )
        .unwrap();

        let mut snapshot = scan_card(&temp.path().join("card")).unwrap();
        db.sync_snapshot(&snapshot).unwrap();
        db.set_figure_name(&snapshot.root_path, "0123", Some("Mes histoires"))
            .unwrap();
        db.decorate_snapshot(&mut snapshot).unwrap();
        assert_eq!(
            snapshot.figures[0].custom_name.as_deref(),
            Some("Mes histoires")
        );
    }
}
