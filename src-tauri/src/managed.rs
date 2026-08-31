use crate::{
    cloud::{self, CloudLibrary, CloudPlaylist},
    storage::{LibraryDatabase, ManagedPlaylistRecord, ManagedTrackRecord},
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_TRACK_BYTES: u64 = 200 * 1024 * 1024;
const MAX_BATCH_FILES: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedTrack {
    pub position: u16,
    pub label: String,
    pub audio_available: bool,
    pub audio_size_bytes: u64,
    pub audio_sha256: String,
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPlaylist {
    pub figure_id: String,
    pub name: String,
    pub nfc_payload: String,
    pub track_count: u16,
    pub tracks: Vec<ManagedTrack>,
    pub updated_at: String,
    pub pending_sync: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedLibrary {
    pub version: i64,
    pub playlists: Vec<ManagedPlaylist>,
    pub storage_used_bytes: u64,
    pub storage_limit_bytes: u64,
    pub offline: bool,
    pub pending_changes: usize,
    pub last_error: Option<String>,
}

pub fn cached_library(
    database: &LibraryDatabase,
    library_root: &Path,
    offline: bool,
    last_error: Option<String>,
) -> Result<ManagedLibrary> {
    let owner = database.active_library_owner()?;
    let records = database.managed_playlists(&owner, false)?;
    let pending_changes = database
        .managed_playlists(&owner, true)?
        .iter()
        .filter(|playlist| playlist.dirty)
        .count();
    let state = database.managed_library_state(&owner)?;
    let mut storage_used_bytes = 0_u64;
    let playlists = records
        .into_iter()
        .map(|playlist| {
            let tracks = playlist
                .tracks
                .iter()
                .map(|track| {
                    let path =
                        track_path(library_root, &owner, &playlist.figure_id, track.position);
                    let available = path.is_file()
                        && fs::metadata(&path)
                            .map(|metadata| metadata.len() == track.audio_size_bytes)
                            .unwrap_or(false);
                    if available {
                        storage_used_bytes =
                            storage_used_bytes.saturating_add(track.audio_size_bytes);
                    }
                    ManagedTrack {
                        position: track.position,
                        label: track.label.clone(),
                        audio_available: available,
                        audio_size_bytes: track.audio_size_bytes,
                        audio_sha256: track.audio_sha256.clone(),
                        local_path: available.then(|| path.to_string_lossy().into_owned()),
                    }
                })
                .collect::<Vec<_>>();
            ManagedPlaylist {
                figure_id: playlist.figure_id.clone(),
                name: playlist.name,
                nfc_payload: format!("02190530{}00", playlist.figure_id),
                track_count: tracks.len() as u16,
                tracks,
                updated_at: playlist.updated_at,
                pending_sync: playlist.dirty,
            }
        })
        .collect();
    Ok(ManagedLibrary {
        version: state.version,
        playlists,
        storage_used_bytes,
        storage_limit_bytes: state.storage_limit_bytes,
        offline,
        pending_changes,
        last_error,
    })
}

pub async fn synchronize(
    database: &LibraryDatabase,
    library_root: &Path,
) -> Result<ManagedLibrary> {
    if database.cloud_session()?.is_none() {
        return cached_library(database, library_root, true, None);
    }
    match synchronize_online(database, library_root).await {
        Ok(()) => cached_library(database, library_root, false, None),
        Err(error) => cached_library(database, library_root, true, Some(format!("{error:#}"))),
    }
}

pub fn adopt_unassigned_library(database: &LibraryDatabase, library_root: &Path) -> Result<bool> {
    let owner = database
        .cloud_session()?
        .map(|session| session.email.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("Aucun compte cloud n'est connecté."))?;
    if database.managed_playlist_count(&owner)? > 0
        || database.managed_playlist_count("local")? == 0
    {
        return Ok(false);
    }
    let source = owner_directory(library_root, "local");
    let destination = owner_directory(library_root, &owner);
    if source.exists() {
        if destination.exists() {
            bail!("Le cache du compte existe déjà ; la bibliothèque locale n'a pas été fusionnée.");
        }
        fs::create_dir_all(library_root)
            .context("Impossible de préparer la bibliothèque locale.")?;
        fs::rename(&source, &destination)
            .context("Impossible de rattacher les fichiers locaux au compte cloud.")?;
    }
    match database.adopt_unassigned_library(&owner) {
        Ok(adopted) => Ok(adopted),
        Err(error) => {
            if destination.exists() && !source.exists() {
                let _ = fs::rename(&destination, &source);
            }
            Err(error)
        }
    }
}

pub async fn import_batch(
    database: &LibraryDatabase,
    library_root: &Path,
    audio_paths: Vec<String>,
    mode: &str,
    playlist_name: Option<&str>,
) -> Result<ManagedLibrary> {
    let paths = audio_paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    validate_audio_paths(&paths, MAX_BATCH_FILES)?;
    if database.cloud_session()?.is_some() {
        let _ = synchronize(database, library_root).await?;
    }
    let owner = database.active_library_owner()?;
    let mut used_ids = database
        .managed_playlists(&owner, true)?
        .into_iter()
        .map(|playlist| playlist.figure_id)
        .collect::<HashSet<_>>();

    match mode {
        "onePerFile" => {
            for path in paths {
                let figure_id = next_available_id(&used_ids)?;
                used_ids.insert(figure_id.clone());
                let name = file_stem(&path);
                save_local_playlist(database, library_root, &owner, &figure_id, &name, &[path])?;
            }
        }
        "singlePlaylist" => {
            if paths.len() > 99 {
                bail!("Une playlist est limitée à 99 pistes.");
            }
            let name = normalize_name(playlist_name.unwrap_or_default());
            if name.is_empty() {
                bail!("Donnez un nom à la playlist qui regroupe les sons.");
            }
            let figure_id = next_available_id(&used_ids)?;
            save_local_playlist(database, library_root, &owner, &figure_id, &name, &paths)?;
        }
        _ => bail!("Mode d'import en lot inconnu."),
    }
    synchronize(database, library_root).await
}

pub async fn replace_playlist(
    database: &LibraryDatabase,
    library_root: &Path,
    figure_id: &str,
    audio_paths: Vec<String>,
) -> Result<ManagedLibrary> {
    validate_figure_id(figure_id)?;
    let paths = audio_paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    validate_audio_paths(&paths, 99)?;
    let owner = database.active_library_owner()?;
    let existing = database
        .managed_playlists(&owner, false)?
        .into_iter()
        .find(|playlist| playlist.figure_id == figure_id)
        .ok_or_else(|| anyhow!("Cette playlist n'existe pas dans la bibliothèque locale."))?;
    save_local_playlist(
        database,
        library_root,
        &owner,
        figure_id,
        &existing.name,
        &paths,
    )?;
    synchronize(database, library_root).await
}

pub async fn rename_playlist(
    database: &LibraryDatabase,
    library_root: &Path,
    figure_id: &str,
    name: &str,
) -> Result<ManagedLibrary> {
    validate_figure_id(figure_id)?;
    let name = normalize_name(name);
    if name.is_empty() {
        bail!("Le nom de la playlist ne peut pas être vide.");
    }
    let owner = database.active_library_owner()?;
    database.rename_managed_playlist(&owner, figure_id, &name)?;
    synchronize(database, library_root).await
}

pub async fn delete_playlist(
    database: &LibraryDatabase,
    library_root: &Path,
    figure_id: &str,
) -> Result<ManagedLibrary> {
    validate_figure_id(figure_id)?;
    let owner = database.active_library_owner()?;
    database.mark_managed_deleted(&owner, figure_id)?;
    let _ = fs::remove_dir_all(playlist_directory(library_root, &owner, figure_id));
    synchronize(database, library_root).await
}

pub fn playlist_audio_paths(
    database: &LibraryDatabase,
    library_root: &Path,
    figure_id: &str,
) -> Result<(ManagedPlaylist, Vec<PathBuf>)> {
    validate_figure_id(figure_id)?;
    let library = cached_library(database, library_root, true, None)?;
    let playlist = library
        .playlists
        .into_iter()
        .find(|playlist| playlist.figure_id == figure_id)
        .ok_or_else(|| anyhow!("Cette playlist n'existe pas dans la bibliothèque locale."))?;
    if playlist.tracks.is_empty() || playlist.tracks.iter().any(|track| !track.audio_available) {
        bail!("Les fichiers audio de cette playlist ne sont pas tous disponibles sur ce PC.");
    }
    let paths = playlist
        .tracks
        .iter()
        .map(|track| PathBuf::from(track.local_path.as_ref().expect("checked local path")))
        .collect();
    Ok((playlist, paths))
}

pub fn all_playlist_audio_paths(
    database: &LibraryDatabase,
    library_root: &Path,
) -> Result<Vec<(ManagedPlaylist, Vec<PathBuf>)>> {
    let library = cached_library(database, library_root, true, None)?;
    if library.playlists.is_empty() {
        bail!("La bibliothèque locale est vide.");
    }
    let mut result = Vec::with_capacity(library.playlists.len());
    for playlist in library.playlists {
        if playlist.tracks.is_empty() || playlist.tracks.iter().any(|track| !track.audio_available)
        {
            bail!(
                "K{} — {} n'a pas tous ses fichiers audio dans le cache local.",
                playlist.figure_id,
                playlist.name
            );
        }
        let paths = playlist
            .tracks
            .iter()
            .map(|track| PathBuf::from(track.local_path.as_ref().expect("checked local path")))
            .collect::<Vec<_>>();
        result.push((playlist, paths));
    }
    Ok(result)
}

async fn synchronize_online(database: &LibraryDatabase, library_root: &Path) -> Result<()> {
    let owner = database.active_library_owner()?;
    push_pending(database, library_root, &owner).await?;
    let remote = cloud::library(database).await?;
    pull_remote(database, library_root, &owner, &remote).await?;
    database.mark_cloud_synced()?;
    Ok(())
}

async fn push_pending(database: &LibraryDatabase, library_root: &Path, owner: &str) -> Result<()> {
    let session = database
        .cloud_session()?
        .ok_or_else(|| anyhow!("Connectez-vous d'abord à FABA Cloud."))?;
    let pending = database
        .managed_playlists(owner, true)?
        .into_iter()
        .filter(|playlist| playlist.dirty)
        .collect::<Vec<_>>();
    for playlist in pending {
        if playlist.deleted {
            cloud::delete_remote_playlist(database, &playlist.figure_id).await?;
            database.purge_managed_playlist(owner, &playlist.figure_id)?;
            let _ =
                fs::remove_dir_all(playlist_directory(library_root, owner, &playlist.figure_id));
            continue;
        }
        let labels = playlist
            .tracks
            .iter()
            .map(|track| track.label.clone())
            .collect::<Vec<_>>();
        cloud::save_remote_playlist(
            database,
            &playlist.figure_id,
            &playlist.name,
            &labels,
            playlist.needs_audio_upload,
        )
        .await?;
        if playlist.needs_audio_upload {
            for track in &playlist.tracks {
                let path = track_path(library_root, owner, &playlist.figure_id, track.position);
                if !path.is_file() {
                    bail!(
                        "La piste {} de K{} manque dans le cache local.",
                        track.position + 1,
                        playlist.figure_id
                    );
                }
                cloud::upload_track(&session, &playlist.figure_id, track.position, &path).await?;
            }
        }
        database.mark_managed_clean(owner, &playlist.figure_id)?;
    }
    Ok(())
}

async fn pull_remote(
    database: &LibraryDatabase,
    library_root: &Path,
    owner: &str,
    remote: &CloudLibrary,
) -> Result<()> {
    let local = database
        .managed_playlists(owner, true)?
        .into_iter()
        .map(|playlist| (playlist.figure_id.clone(), playlist))
        .collect::<HashMap<_, _>>();
    let remote_ids = remote
        .playlists
        .iter()
        .map(|playlist| playlist.figure_id.clone())
        .collect::<HashSet<_>>();

    for playlist in &remote.playlists {
        validate_figure_id(&playlist.figure_id)?;
        if local
            .get(&playlist.figure_id)
            .is_some_and(|playlist| playlist.dirty)
        {
            continue;
        }
        cache_remote_playlist(
            database,
            library_root,
            owner,
            playlist,
            local.get(&playlist.figure_id),
        )
        .await?;
    }

    for playlist in local.values() {
        if !playlist.dirty && !remote_ids.contains(&playlist.figure_id) {
            database.purge_managed_playlist(owner, &playlist.figure_id)?;
            let _ =
                fs::remove_dir_all(playlist_directory(library_root, owner, &playlist.figure_id));
        }
    }
    database.save_managed_library_state(owner, remote.version, remote.storage_limit_bytes)?;
    Ok(())
}

async fn cache_remote_playlist(
    database: &LibraryDatabase,
    library_root: &Path,
    owner: &str,
    playlist: &CloudPlaylist,
    existing: Option<&ManagedPlaylistRecord>,
) -> Result<()> {
    let destination = playlist_directory(library_root, owner, &playlist.figure_id);
    let staging = owner_directory(library_root, owner).join(format!(
        ".K{}-sync-{}",
        playlist.figure_id,
        nonce()
    ));
    fs::create_dir_all(&staging).context("Impossible de préparer le cache audio local.")?;
    let result = async {
        let mut tracks = Vec::with_capacity(playlist.tracks.len());
        for track in &playlist.tracks {
            let mut size = track.audio_size_bytes.unwrap_or(0);
            let mut hash = track.audio_sha256.clone().unwrap_or_default();
            if track.audio_available {
                let expected_hash = track
                    .audio_sha256
                    .as_deref()
                    .ok_or_else(|| anyhow!("FABA Cloud n'a pas fourni l'empreinte audio."))?;
                let target = staging.join(format!("{:02}.mp3", track.position));
                let previous = existing
                    .and_then(|playlist| playlist.tracks.get(track.position as usize))
                    .filter(|local_track| local_track.audio_sha256 == expected_hash)
                    .map(|_| track_path(library_root, owner, &playlist.figure_id, track.position));
                if previous.as_ref().is_some_and(|path| path.is_file()) {
                    fs::copy(previous.expect("checked previous path"), &target)
                        .context("Impossible de conserver une piste déjà en cache.")?;
                } else {
                    let bytes =
                        cloud::download_track_bytes(database, &playlist.figure_id, track.position)
                            .await?;
                    let downloaded_hash = hex::encode(Sha256::digest(&bytes));
                    if downloaded_hash != expected_hash {
                        bail!("La vérification d'intégrité d'une piste cloud a échoué.");
                    }
                    size = bytes.len() as u64;
                    hash = downloaded_hash;
                    fs::write(&target, bytes)
                        .context("Impossible d'enregistrer une piste dans le cache local.")?;
                }
            }
            tracks.push(ManagedTrackRecord {
                position: track.position,
                label: normalize_track_label(&track.label, track.position),
                audio_size_bytes: size,
                audio_sha256: hash,
            });
        }
        replace_directory(&staging, &destination)?;
        database.save_managed_playlist(
            owner,
            &ManagedPlaylistRecord {
                figure_id: playlist.figure_id.clone(),
                name: normalize_name(&playlist.name),
                updated_at: playlist.updated_at.clone(),
                dirty: false,
                needs_audio_upload: false,
                deleted: false,
                tracks,
            },
        )?;
        Result::<()>::Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn save_local_playlist(
    database: &LibraryDatabase,
    library_root: &Path,
    owner: &str,
    figure_id: &str,
    name: &str,
    paths: &[PathBuf],
) -> Result<()> {
    validate_figure_id(figure_id)?;
    validate_audio_paths(paths, 99)?;
    let destination = playlist_directory(library_root, owner, figure_id);
    let staging =
        owner_directory(library_root, owner).join(format!(".K{figure_id}-import-{}", nonce()));
    fs::create_dir_all(&staging).context("Impossible de préparer l'import local.")?;
    let result = (|| {
        let mut tracks = Vec::with_capacity(paths.len());
        for (position, source) in paths.iter().enumerate() {
            let target = staging.join(format!("{position:02}.mp3"));
            fs::copy(source, &target).with_context(|| {
                format!(
                    "Impossible de copier {} dans la bibliothèque.",
                    source.display()
                )
            })?;
            tracks.push(ManagedTrackRecord {
                position: position as u16,
                label: file_stem(source),
                audio_size_bytes: fs::metadata(&target)?.len(),
                audio_sha256: sha256_file(&target)?,
            });
        }
        replace_directory(&staging, &destination)?;
        database.save_managed_playlist(
            owner,
            &ManagedPlaylistRecord {
                figure_id: figure_id.to_owned(),
                name: normalize_name(name),
                updated_at: Utc::now().to_rfc3339(),
                dirty: true,
                needs_audio_upload: true,
                deleted: false,
                tracks,
            },
        )
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn validate_audio_paths(paths: &[PathBuf], maximum: usize) -> Result<()> {
    if paths.is_empty() || paths.len() > maximum {
        bail!("Sélectionnez entre 1 et {maximum} fichiers MP3.");
    }
    for path in paths {
        if !path.is_file() {
            bail!("Fichier audio introuvable : {}", path.display());
        }
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
        {
            bail!("Seuls les fichiers MP3 sont acceptés : {}", path.display());
        }
        if fs::metadata(path)?.len() > MAX_TRACK_BYTES {
            bail!("Une piste ne peut pas dépasser 200 Mo : {}", path.display());
        }
    }
    Ok(())
}

fn next_available_id(used: &HashSet<String>) -> Result<String> {
    (2000..=8999)
        .map(|value| value.to_string())
        .find(|value| !used.contains(value))
        .ok_or_else(|| anyhow!("Aucun identifiant personnalisé libre entre 2000 et 8999."))
}

fn validate_figure_id(value: &str) -> Result<()> {
    let valid = value.len() == 4
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(value.as_bytes()[0], b'2'..=b'8');
    if !valid {
        bail!("Utilisez un identifiant personnalisé entre 2000 et 8999.");
    }
    Ok(())
}

fn normalize_name(value: &str) -> String {
    value.trim().chars().take(100).collect()
}

fn normalize_track_label(value: &str, position: u16) -> String {
    let label = value.trim().chars().take(200).collect::<String>();
    if label.is_empty() {
        format!("Piste {}", position + 1)
    } else {
        label
    }
}

fn file_stem(path: &Path) -> String {
    normalize_name(
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Piste"),
    )
}

fn owner_directory(library_root: &Path, owner: &str) -> PathBuf {
    library_root.join(hex::encode(Sha256::digest(owner.as_bytes())))
}

fn playlist_directory(library_root: &Path, owner: &str, figure_id: &str) -> PathBuf {
    owner_directory(library_root, owner).join(format!("K{figure_id}"))
}

fn track_path(library_root: &Path, owner: &str, figure_id: &str, position: u16) -> PathBuf {
    playlist_directory(library_root, owner, figure_id).join(format!("{position:02}.mp3"))
}

fn replace_directory(staging: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("Chemin de bibliothèque locale invalide."))?;
    fs::create_dir_all(parent)?;
    if !destination.exists() {
        fs::rename(staging, destination)?;
        return Ok(());
    }
    let previous = parent.join(format!(
        ".{}-previous-{}",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("playlist"),
        nonce()
    ));
    fs::rename(destination, &previous)?;
    if let Err(error) = fs::rename(staging, destination) {
        let _ = fs::rename(&previous, destination);
        return Err(error).context("Impossible de finaliser la bibliothèque locale.");
    }
    fs::remove_dir_all(previous)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{scan_card, write_faba_plus_figure};
    use tempfile::tempdir;

    #[test]
    fn batch_import_allocates_ids_and_keeps_audio_offline() {
        let temp = tempdir().unwrap();
        let database = LibraryDatabase::new(temp.path().join("library.sqlite3"));
        database.initialize().unwrap();
        let first = temp.path().join("Une histoire.mp3");
        let second = temp.path().join("Une chanson.mp3");
        fs::write(&first, b"ID3-first").unwrap();
        fs::write(&second, b"ID3-second").unwrap();

        save_local_playlist(
            &database,
            &temp.path().join("audio"),
            "local",
            "2000",
            "Une histoire",
            &[first],
        )
        .unwrap();
        let used = database
            .managed_playlists("local", true)
            .unwrap()
            .into_iter()
            .map(|playlist| playlist.figure_id)
            .collect::<HashSet<_>>();
        assert_eq!(next_available_id(&used).unwrap(), "2001");

        save_local_playlist(
            &database,
            &temp.path().join("audio"),
            "local",
            "2001",
            "Une chanson",
            &[second],
        )
        .unwrap();
        let library = cached_library(&database, &temp.path().join("audio"), true, None).unwrap();
        assert_eq!(library.playlists.len(), 2);
        assert!(library
            .playlists
            .iter()
            .all(|playlist| playlist.pending_sync));
        assert!(library
            .playlists
            .iter()
            .flat_map(|playlist| &playlist.tracks)
            .all(|track| track.audio_available));
    }

    #[test]
    fn rejects_reserved_ids() {
        for invalid in ["0001", "1999", "9000", "abcd"] {
            assert!(validate_figure_id(invalid).is_err());
        }
        for valid in ["2000", "4567", "8999"] {
            assert!(validate_figure_id(valid).is_ok());
        }
    }

    #[test]
    fn library_write_overwrites_matching_ids_and_preserves_card_extras() {
        let temp = tempdir().unwrap();
        let database = LibraryDatabase::new(temp.path().join("library.sqlite3"));
        database.initialize().unwrap();
        let library_root = temp.path().join("library");
        let audio = temp.path().join("nouvelle-version.mp3");
        fs::write(&audio, b"new-audio").unwrap();
        save_local_playlist(
            &database,
            &library_root,
            "local",
            "2000",
            "Nouvelle version",
            &[audio],
        )
        .unwrap();

        let card = temp.path().join("card");
        let unrelated = card.join("PLAYER/K7777");
        let replaced = card.join("PLAYER/K2000");
        fs::create_dir_all(&unrelated).unwrap();
        fs::create_dir_all(&replaced).unwrap();
        fs::write(unrelated.join("CP00.faba"), b"untouched-extra").unwrap();
        fs::write(
            unrelated.join("info"),
            r#"{"totalTracks":1,"characterDir":"02190530777700"}"#,
        )
        .unwrap();
        fs::write(replaced.join("CP00.faba"), b"old-version").unwrap();
        fs::write(
            replaced.join("info"),
            r#"{"totalTracks":1,"characterDir":"02190530200000"}"#,
        )
        .unwrap();

        let editable_root = PathBuf::from(scan_card(&card).unwrap().root_path);
        for (playlist, paths) in all_playlist_audio_paths(&database, &library_root).unwrap() {
            write_faba_plus_figure(
                &editable_root,
                &playlist.figure_id,
                &paths,
                &temp.path().join("backups"),
            )
            .unwrap();
        }

        assert_eq!(
            fs::read(unrelated.join("CP00.faba")).unwrap(),
            b"untouched-extra"
        );
        assert_ne!(fs::read(replaced.join("00.faba")).unwrap(), b"old-version");
        let snapshot = scan_card(&card).unwrap();
        assert_eq!(
            snapshot
                .figures
                .iter()
                .map(|figure| figure.id.as_str())
                .collect::<Vec<_>>(),
            vec!["2000", "7777"]
        );
    }
}
