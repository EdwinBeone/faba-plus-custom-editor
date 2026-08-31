use crate::domain::{CardKind, CardSnapshot};
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSession {
    pub endpoint: String,
    pub email: String,
    pub display_name: String,
    pub token: String,
    pub expires_at: String,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub endpoint: String,
    pub authenticated: bool,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub expires_at: Option<String>,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManagedTrackRecord {
    pub position: u16,
    pub label: String,
    pub audio_size_bytes: u64,
    pub audio_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ManagedPlaylistRecord {
    pub figure_id: String,
    pub name: String,
    pub updated_at: String,
    pub dirty: bool,
    pub needs_audio_upload: bool,
    pub deleted: bool,
    pub tracks: Vec<ManagedTrackRecord>,
}

#[derive(Debug, Clone, Copy)]
pub struct ManagedLibraryState {
    pub version: i64,
    pub storage_limit_bytes: u64,
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
             );
             CREATE TABLE IF NOT EXISTS cloud_session (
               id INTEGER PRIMARY KEY CHECK (id = 1),
               endpoint TEXT NOT NULL,
               email TEXT NOT NULL,
               display_name TEXT NOT NULL,
               token TEXT NOT NULL,
               expires_at TEXT NOT NULL,
               last_sync_at TEXT
             );
             CREATE TABLE IF NOT EXISTS managed_playlists (
               owner TEXT NOT NULL,
               figure_id TEXT NOT NULL,
               name TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               dirty INTEGER NOT NULL DEFAULT 1,
               needs_audio_upload INTEGER NOT NULL DEFAULT 1,
               deleted INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (owner, figure_id)
             );
             CREATE TABLE IF NOT EXISTS managed_tracks (
               owner TEXT NOT NULL,
               figure_id TEXT NOT NULL,
               position INTEGER NOT NULL,
               label TEXT NOT NULL,
               audio_size_bytes INTEGER NOT NULL,
               audio_sha256 TEXT NOT NULL,
               PRIMARY KEY (owner, figure_id, position),
               FOREIGN KEY (owner, figure_id) REFERENCES managed_playlists(owner, figure_id)
                 ON DELETE CASCADE ON UPDATE CASCADE
             );
             CREATE TABLE IF NOT EXISTS managed_library_state (
               owner TEXT PRIMARY KEY,
               version INTEGER NOT NULL DEFAULT 0,
               storage_limit_bytes INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS library_settings (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
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

    pub fn cloud_session(&self) -> Result<Option<CloudSession>> {
        let connection = self.connection()?;
        let result = connection.query_row(
            "SELECT endpoint, email, display_name, token, expires_at, last_sync_at
             FROM cloud_session WHERE id=1",
            [],
            |row| {
                Ok(CloudSession {
                    endpoint: row.get(0)?,
                    email: row.get(1)?,
                    display_name: row.get(2)?,
                    token: row.get(3)?,
                    expires_at: row.get(4)?,
                    last_sync_at: row.get(5)?,
                })
            },
        );
        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn cloud_status(&self, default_endpoint: &str) -> Result<CloudStatus> {
        Ok(match self.cloud_session()? {
            Some(session) => CloudStatus {
                endpoint: session.endpoint,
                authenticated: true,
                email: Some(session.email),
                display_name: Some(session.display_name),
                expires_at: Some(session.expires_at),
                last_sync_at: session.last_sync_at,
            },
            None => CloudStatus {
                endpoint: default_endpoint.to_owned(),
                authenticated: false,
                email: None,
                display_name: None,
                expires_at: None,
                last_sync_at: None,
            },
        })
    }

    pub fn save_cloud_session(&self, session: &CloudSession) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO cloud_session(id, endpoint, email, display_name, token, expires_at, last_sync_at)
             VALUES(1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               endpoint=excluded.endpoint,
               email=excluded.email,
               display_name=excluded.display_name,
               token=excluded.token,
               expires_at=excluded.expires_at,
               last_sync_at=excluded.last_sync_at",
            params![
                session.endpoint,
                session.email,
                session.display_name,
                session.token,
                session.expires_at,
                session.last_sync_at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO library_settings(key, value) VALUES('active_owner', ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![session.email.to_ascii_lowercase()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_cloud_synced(&self) -> Result<()> {
        self.connection()?.execute(
            "UPDATE cloud_session SET last_sync_at=?1 WHERE id=1",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn clear_cloud_session(&self) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM cloud_session WHERE id=1", [])?;
        Ok(())
    }

    pub fn active_library_owner(&self) -> Result<String> {
        if let Some(session) = self.cloud_session()? {
            return Ok(session.email.to_ascii_lowercase());
        }
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT value FROM library_settings WHERE key='active_owner'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "local".into()))
    }

    pub fn adopt_unassigned_library(&self, owner: &str) -> Result<bool> {
        let normalized_owner = owner.to_ascii_lowercase();
        if normalized_owner == "local" {
            return Ok(false);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let target_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM managed_playlists WHERE owner=?1",
            params![normalized_owner],
            |row| row.get(0),
        )?;
        let local_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM managed_playlists WHERE owner='local'",
            [],
            |row| row.get(0),
        )?;
        let adopted = target_count == 0 && local_count > 0;
        if adopted {
            transaction.execute(
                "UPDATE managed_playlists SET owner=?1 WHERE owner='local'",
                params![normalized_owner],
            )?;
            let local_state_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM managed_library_state WHERE owner='local'",
                [],
                |row| row.get(0),
            )?;
            if local_state_count > 0 {
                transaction.execute(
                    "DELETE FROM managed_library_state WHERE owner=?1",
                    params![normalized_owner],
                )?;
                transaction.execute(
                    "UPDATE managed_library_state SET owner=?1 WHERE owner='local'",
                    params![normalized_owner],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO library_settings(key, value) VALUES('active_owner', ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![normalized_owner],
        )?;
        transaction.commit()?;
        Ok(adopted)
    }

    pub fn managed_playlist_count(&self, owner: &str) -> Result<u64> {
        let count = self.connection()?.query_row(
            "SELECT COUNT(*) FROM managed_playlists WHERE owner=?1",
            params![owner],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn managed_playlists(
        &self,
        owner: &str,
        include_deleted: bool,
    ) -> Result<Vec<ManagedPlaylistRecord>> {
        let connection = self.connection()?;
        let sql = if include_deleted {
            "SELECT figure_id, name, updated_at, dirty, needs_audio_upload, deleted
             FROM managed_playlists WHERE owner=?1 ORDER BY figure_id"
        } else {
            "SELECT figure_id, name, updated_at, dirty, needs_audio_upload, deleted
             FROM managed_playlists WHERE owner=?1 AND deleted=0 ORDER BY figure_id"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement
            .query_map(params![owner], |row| {
                Ok(ManagedPlaylistRecord {
                    figure_id: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                    dirty: row.get::<_, i64>(3)? != 0,
                    needs_audio_upload: row.get::<_, i64>(4)? != 0,
                    deleted: row.get::<_, i64>(5)? != 0,
                    tracks: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);

        let mut playlists = Vec::with_capacity(rows.len());
        for mut playlist in rows {
            let mut track_statement = connection.prepare(
                "SELECT position, label, audio_size_bytes, audio_sha256
                 FROM managed_tracks WHERE owner=?1 AND figure_id=?2 ORDER BY position",
            )?;
            playlist.tracks = track_statement
                .query_map(params![owner, playlist.figure_id], |row| {
                    Ok(ManagedTrackRecord {
                        position: row.get(0)?,
                        label: row.get(1)?,
                        audio_size_bytes: row.get::<_, i64>(2)?.max(0) as u64,
                        audio_sha256: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            playlists.push(playlist);
        }
        Ok(playlists)
    }

    pub fn save_managed_playlist(
        &self,
        owner: &str,
        playlist: &ManagedPlaylistRecord,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO managed_playlists(owner, figure_id, name, updated_at, dirty, needs_audio_upload, deleted)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, 0)
             ON CONFLICT(owner, figure_id) DO UPDATE SET
               name=excluded.name, updated_at=excluded.updated_at, dirty=excluded.dirty,
               needs_audio_upload=excluded.needs_audio_upload, deleted=0",
            params![
                owner,
                playlist.figure_id,
                playlist.name,
                playlist.updated_at,
                i64::from(playlist.dirty),
                i64::from(playlist.needs_audio_upload),
            ],
        )?;
        transaction.execute(
            "DELETE FROM managed_tracks WHERE owner=?1 AND figure_id=?2",
            params![owner, playlist.figure_id],
        )?;
        for track in &playlist.tracks {
            transaction.execute(
                "INSERT INTO managed_tracks(owner, figure_id, position, label, audio_size_bytes, audio_sha256)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    owner,
                    playlist.figure_id,
                    track.position,
                    track.label,
                    track.audio_size_bytes as i64,
                    track.audio_sha256,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn rename_managed_playlist(&self, owner: &str, figure_id: &str, name: &str) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE managed_playlists
             SET name=?3, updated_at=?4, dirty=1
             WHERE owner=?1 AND figure_id=?2 AND deleted=0",
            params![owner, figure_id, name, Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            anyhow::bail!("Cette playlist n'existe pas dans la bibliothèque locale.");
        }
        Ok(())
    }

    pub fn mark_managed_deleted(&self, owner: &str, figure_id: &str) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE managed_playlists
             SET deleted=1, dirty=1, needs_audio_upload=0, updated_at=?3
             WHERE owner=?1 AND figure_id=?2 AND deleted=0",
            params![owner, figure_id, Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            anyhow::bail!("Cette playlist n'existe pas dans la bibliothèque locale.");
        }
        Ok(())
    }

    pub fn mark_managed_clean(&self, owner: &str, figure_id: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE managed_playlists SET dirty=0, needs_audio_upload=0
             WHERE owner=?1 AND figure_id=?2",
            params![owner, figure_id],
        )?;
        Ok(())
    }

    pub fn purge_managed_playlist(&self, owner: &str, figure_id: &str) -> Result<()> {
        self.connection()?.execute(
            "DELETE FROM managed_playlists WHERE owner=?1 AND figure_id=?2",
            params![owner, figure_id],
        )?;
        Ok(())
    }

    pub fn managed_library_state(&self, owner: &str) -> Result<ManagedLibraryState> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT version, storage_limit_bytes FROM managed_library_state WHERE owner=?1",
                params![owner],
                |row| {
                    Ok(ManagedLibraryState {
                        version: row.get(0)?,
                        storage_limit_bytes: row.get::<_, i64>(1)?.max(0) as u64,
                    })
                },
            )
            .unwrap_or(ManagedLibraryState {
                version: 0,
                storage_limit_bytes: 0,
            }))
    }

    pub fn save_managed_library_state(
        &self,
        owner: &str,
        version: i64,
        storage_limit_bytes: u64,
    ) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO managed_library_state(owner, version, storage_limit_bytes)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(owner) DO UPDATE SET
               version=excluded.version, storage_limit_bytes=excluded.storage_limit_bytes",
            params![owner, version, storage_limit_bytes as i64],
        )?;
        Ok(())
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

    #[test]
    fn adopts_offline_library_even_when_account_state_already_exists() {
        let temp = tempdir().unwrap();
        let db = LibraryDatabase::new(temp.path().join("library.sqlite3"));
        db.initialize().unwrap();
        db.save_managed_library_state("local", 4, 1_000).unwrap();
        db.save_managed_library_state("edwin@example.be", 7, 2_000)
            .unwrap();
        db.save_managed_playlist(
            "local",
            &ManagedPlaylistRecord {
                figure_id: "2000".into(),
                name: "Histoire".into(),
                updated_at: "2026-08-31T00:00:00Z".into(),
                dirty: true,
                needs_audio_upload: true,
                deleted: false,
                tracks: vec![ManagedTrackRecord {
                    position: 0,
                    label: "Piste".into(),
                    audio_size_bytes: 12,
                    audio_sha256: "abc".into(),
                }],
            },
        )
        .unwrap();

        assert!(db.adopt_unassigned_library("EDWIN@example.be").unwrap());
        assert_eq!(db.managed_playlist_count("local").unwrap(), 0);
        assert_eq!(db.managed_playlist_count("edwin@example.be").unwrap(), 1);
        assert_eq!(
            db.managed_library_state("edwin@example.be")
                .unwrap()
                .version,
            4
        );
    }
}
