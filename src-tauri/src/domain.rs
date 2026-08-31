use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use id3::{frame::Frame, Encoding, Tag, TagLike, Version};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CardKind {
    FabaPlus,
    LegacyFaba,
    Empty,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub index: u16,
    pub file_name: String,
    pub path: String,
    pub label: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub id: String,
    pub folder_name: String,
    pub custom_name: Option<String>,
    pub path: String,
    pub nfc_payload: String,
    pub tracks: Vec<Track>,
    pub modified_at: Option<DateTime<Utc>>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSnapshot {
    pub root_path: String,
    pub kind: CardKind,
    pub writable: bool,
    pub figures: Vec<Figure>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabaPlusInfo {
    total_tracks: Option<u16>,
    character_dir: Option<String>,
}

pub fn scan_card(selected_path: &Path) -> Result<CardSnapshot> {
    if !selected_path.is_dir() {
        bail!("Le dossier sélectionné n'existe pas ou n'est pas accessible.");
    }

    let root = discover_content_root(selected_path);
    let mut figures = Vec::new();
    let mut saw_plus = false;
    let mut saw_legacy = false;

    let entries =
        fs::read_dir(&root).with_context(|| format!("Impossible de lire {}", root.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(folder_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(figure_id) = parse_figure_folder(folder_name) else {
            continue;
        };

        let mut tracks = Vec::new();
        for track_entry in fs::read_dir(&path).into_iter().flatten().flatten() {
            let track_path = track_entry.path();
            if !track_path.is_file() {
                continue;
            }
            let Some(file_name) = track_path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some(index) = parse_track_index(file_name) else {
                continue;
            };

            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".faba") {
                saw_plus = true;
            } else if lower.ends_with(".mki") || Path::new(file_name).extension().is_none() {
                saw_legacy = true;
            } else {
                continue;
            }

            let size_bytes = track_entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            tracks.push(Track {
                index,
                file_name: file_name.to_owned(),
                path: path_string(&track_path),
                label: format!("Piste {}", index + 1),
                size_bytes,
            });
        }
        tracks.sort_by_key(|track| track.index);

        let info_path = path.join("info");
        let mut warning = None;
        if info_path.is_file() {
            saw_plus = true;
            match read_plus_info(&info_path) {
                Ok(info) => {
                    if info.total_tracks != Some(tracks.len() as u16) {
                        warning =
                            Some("Le nombre de pistes ne correspond pas au fichier info.".into());
                    }
                    let expected = format!("02190530{figure_id}00");
                    if info.character_dir.as_deref() != Some(expected.as_str()) {
                        warning = Some("Le code NFC du fichier info semble incohérent.".into());
                    }
                }
                Err(_) => warning = Some("Le fichier info est illisible.".into()),
            }
        }

        let modified_at = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map(DateTime::<Utc>::from);

        figures.push(Figure {
            id: figure_id.clone(),
            folder_name: folder_name.to_owned(),
            custom_name: None,
            path: path_string(&path),
            nfc_payload: format!("02190530{figure_id}00"),
            tracks,
            modified_at,
            warning,
        });
    }
    figures.sort_by(|left, right| left.id.cmp(&right.id));

    let kind = if saw_plus {
        CardKind::FabaPlus
    } else if saw_legacy || root.file_name().and_then(|value| value.to_str()) == Some("MKI01") {
        CardKind::LegacyFaba
    } else if figures.is_empty() {
        CardKind::Empty
    } else {
        CardKind::Unknown
    };

    let mut warnings = Vec::new();
    if kind == CardKind::LegacyFaba {
        warnings.push(
            "Ancien format FABA détecté : consultation uniquement dans cette version.".into(),
        );
    }
    if kind == CardKind::Empty {
        warnings.push("Aucun contenu FABA+ détecté. Ce dossier peut être initialisé avec votre première figurine.".into());
    }
    if root != selected_path
        && is_named_directory(&root, "PLAYER")
        && has_figure_folders(selected_path)
    {
        warnings.push(
            "Des dossiers Kxxxx ont été détectés à la racine de la carte. Sur FABA+, ils doivent être placés dans le dossier PLAYER.".into(),
        );
    }

    Ok(CardSnapshot {
        root_path: path_string(&root),
        kind,
        writable: !fs::metadata(&root)?.permissions().readonly() && kind != CardKind::LegacyFaba,
        figures,
        warnings,
    })
}

pub fn discover_content_root(selected_path: &Path) -> PathBuf {
    if is_named_directory(selected_path, "PLAYER") {
        return selected_path.to_path_buf();
    }
    if let Some(player) = find_child_directory(selected_path, "PLAYER") {
        return player;
    }
    if has_figure_folders(selected_path) {
        return selected_path.to_path_buf();
    }
    let legacy = selected_path.join("MKI01");
    if legacy.is_dir() && has_figure_folders(&legacy) {
        return legacy;
    }
    selected_path.to_path_buf()
}

pub fn looks_like_card(selected_path: &Path) -> bool {
    is_named_directory(selected_path, "PLAYER")
        || find_child_directory(selected_path, "PLAYER").is_some()
        || has_figure_folders(selected_path)
        || has_figure_folders(&selected_path.join("MKI01"))
}

fn is_named_directory(path: &Path, expected_name: &str) -> bool {
    path.is_dir()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(expected_name))
}

fn find_child_directory(parent: &Path, expected_name: &str) -> Option<PathBuf> {
    fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| is_named_directory(path, expected_name))
}

fn has_figure_folders(path: &Path) -> bool {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry.path().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .and_then(parse_figure_folder)
                    .is_some()
        })
}

#[cfg(test)]
pub fn write_faba_plus_figure(
    root: &Path,
    figure_id: &str,
    audio_paths: &[PathBuf],
    backup_root: &Path,
) -> Result<Option<PathBuf>> {
    write_faba_plus_figure_with_trace(root, figure_id, audio_paths, backup_root, &|_| {})
}

pub fn write_faba_plus_figure_with_trace(
    root: &Path,
    figure_id: &str,
    audio_paths: &[PathBuf],
    backup_root: &Path,
    trace: &dyn Fn(&str),
) -> Result<Option<PathBuf>> {
    trace("validation des paramètres");
    validate_custom_figure_id(figure_id)?;
    if audio_paths.is_empty() || audio_paths.len() > 99 {
        bail!("Choisissez entre 1 et 99 fichiers MP3.");
    }
    for audio in audio_paths {
        if !audio.is_file() {
            bail!("Fichier audio introuvable : {}", audio.display());
        }
        let is_mp3 = audio
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("mp3"));
        if !is_mp3 {
            bail!("Seuls les fichiers MP3 sont acceptés : {}", audio.display());
        }
    }
    if !root.is_dir() {
        bail!("Le support FABA+ n'est plus accessible.");
    }

    let folder_name = format!("K{figure_id}");
    let destination = root.join(&folder_name);
    let backup = if destination.exists() {
        trace("création de la sauvegarde locale avant remplacement");
        Some(backup_directory(&destination, backup_root, &folder_name)?)
    } else {
        None
    };

    let nonce = unique_nonce();
    let staging = root.join(format!(".faba-editor-{folder_name}-{nonce}"));
    trace("création du dossier temporaire sur la carte");
    fs::create_dir(&staging).context("Impossible de créer le dossier temporaire sur la carte.")?;

    let result = (|| -> Result<()> {
        trace(&format!(
            "copie et préparation ID3 de {} piste(s) MP3",
            audio_paths.len()
        ));
        for (index, source) in audio_paths.iter().enumerate() {
            let target = staging.join(format!("{index:02}.faba"));
            fs::copy(source, &target).with_context(|| {
                format!("Impossible de copier {} vers la carte", source.display())
            })?;
            remove_id3v1_tag(&target).with_context(|| {
                format!(
                    "Impossible de nettoyer les métadonnées de {}",
                    source.display()
                )
            })?;
            let mut tag = Tag::new();
            tag.add_frame(
                Frame::text("TIT2", format!("K{figure_id}CP{:02}", index + 1))
                    .set_encoding(Some(Encoding::UTF16)),
            );
            tag.write_to_path(&target, Version::Id3v23)
                .with_context(|| {
                    format!(
                        "Impossible de préparer les métadonnées FABA+ de {}",
                        source.display()
                    )
                })?;
        }
        let info = serde_json::json!({
            "totalTracks": audio_paths.len(),
            "characterDir": format!("02190530{figure_id}00")
        });
        trace("écriture et synchronisation du fichier info");
        // Keep the file handle in a dedicated scope so it is closed before the
        // staging directory is renamed. Windows refuses to rename a directory
        // while a file inside it is still open.
        {
            let mut info_file = fs::File::create(staging.join("info"))
                .context("Impossible de créer le fichier info sur la carte.")?;
            info_file
                .write_all(info.to_string().as_bytes())
                .context("Impossible d'écrire le fichier info sur la carte.")?;
            info_file
                .sync_all()
                .context("Impossible de synchroniser le fichier info sur la carte.")?;
        }

        let previous = root.join(format!(".{folder_name}-previous-{nonce}"));
        if destination.exists() {
            trace("mise à l'écart atomique de l'ancienne figurine");
            fs::rename(&destination, &previous)
                .context("Impossible de préparer le remplacement de la figurine.")?;
            trace("activation du nouveau dossier de figurine");
            if let Err(error) = fs::rename(&staging, &destination) {
                let _ = fs::rename(&previous, &destination);
                return Err(error)
                    .context("Le remplacement a échoué ; l'ancienne version a été restaurée.");
            }
            fs::remove_dir_all(previous)?;
        } else {
            trace("activation du nouveau dossier de figurine");
            fs::rename(&staging, &destination)
                .context("Impossible de finaliser l'écriture sur la carte.")?;
        }
        trace("écriture terminée avec succès");
        Ok(())
    })();

    if result.is_err() && staging.exists() {
        trace("nettoyage du dossier temporaire après erreur");
        let _ = fs::remove_dir_all(&staging);
    }
    result?;
    Ok(backup)
}

pub fn delete_faba_plus_figure(
    root: &Path,
    figure_id: &str,
    backup_root: &Path,
) -> Result<PathBuf> {
    validate_figure_id(figure_id)?;
    let folder_name = format!("K{figure_id}");
    let destination = root.join(&folder_name);
    if !destination.is_dir() {
        bail!("La figurine n'existe plus sur la carte.");
    }
    let backup = backup_directory(&destination, backup_root, &folder_name)?;
    let tombstone = root.join(format!(".{folder_name}-deleted-{}", unique_nonce()));
    fs::rename(&destination, &tombstone)?;
    fs::remove_dir_all(&tombstone)?;
    Ok(backup)
}

pub fn export_figure(root: &Path, figure_id: &str, destination: &Path) -> Result<PathBuf> {
    validate_figure_id(figure_id)?;
    if !destination.is_dir() {
        bail!("Le dossier d'export n'existe pas.");
    }
    let folder_name = format!("K{figure_id}");
    let source = root.join(&folder_name);
    if !source.is_dir() {
        bail!("La figurine n'existe plus sur la carte.");
    }
    let target = next_available_path(destination, &folder_name);
    copy_directory(&source, &target)?;
    Ok(target)
}

fn validate_figure_id(figure_id: &str) -> Result<()> {
    if figure_id.len() != 4
        || !figure_id.bytes().all(|byte| byte.is_ascii_digit())
        || figure_id == "0000"
    {
        bail!("L'identifiant doit contenir 4 chiffres entre 0001 et 9999.");
    }
    Ok(())
}

fn validate_custom_figure_id(figure_id: &str) -> Result<()> {
    validate_figure_id(figure_id)?;
    if matches!(figure_id.as_bytes()[0], b'0' | b'1' | b'9') {
        bail!(
            "Les identifiants 0xxx, 1xxx et 9xxx sont réservés par FABA+. Choisissez un identifiant entre 2000 et 8999."
        );
    }
    Ok(())
}

fn parse_figure_folder(value: &str) -> Option<String> {
    (value.len() == 5
        && value.starts_with('K')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_digit))
    .then(|| value[1..].to_owned())
}

fn parse_track_index(value: &str) -> Option<u16> {
    let stem = Path::new(value).file_stem()?.to_str()?;
    let digits = if let Some(rest) = stem.strip_prefix("CP").or_else(|| stem.strip_prefix("cp")) {
        rest
    } else if stem.bytes().all(|byte| byte.is_ascii_digit()) {
        stem
    } else {
        return None;
    };
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn remove_id3v1_tag(path: &Path) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let length = file.metadata()?.len();
    if length < 128 {
        return Ok(());
    }
    file.seek(SeekFrom::End(-128))?;
    let mut marker = [0_u8; 3];
    file.read_exact(&mut marker)?;
    if marker == *b"TAG" {
        file.set_len(length - 128)?;
    }
    Ok(())
}

fn read_plus_info(path: &Path) -> Result<FabaPlusInfo> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(Into::into)
}

fn backup_directory(source: &Path, backup_root: &Path, folder_name: &str) -> Result<PathBuf> {
    fs::create_dir_all(backup_root)?;
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let target = next_available_path(backup_root, &format!("{folder_name}-{timestamp}"));
    copy_directory(source, &target)?;
    Ok(target)
}

fn next_available_path(parent: &Path, base_name: &str) -> PathBuf {
    let first = parent.join(base_name);
    if !first.exists() {
        return first;
    }
    for suffix in 2..10_000 {
        let candidate = parent.join(format!("{base_name}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base_name}-{}", unique_nonce()))
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn unique_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn ensure_editable(snapshot: &CardSnapshot) -> Result<()> {
    match snapshot.kind {
        CardKind::LegacyFaba => {
            bail!("Cette carte utilise l'ancien format FABA, disponible en lecture seule.")
        }
        CardKind::Unknown => bail!("Le format de cette carte n'est pas reconnu."),
        _ if !snapshot.writable => bail!("La carte est en lecture seule."),
        _ => Ok(()),
    }
}

pub fn require_figure<'a>(snapshot: &'a CardSnapshot, figure_id: &str) -> Result<&'a Figure> {
    snapshot
        .figures
        .iter()
        .find(|figure| figure.id == figure_id)
        .ok_or_else(|| anyhow!("Figurine introuvable sur la carte."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fake_mp3(path: &Path, contents: &[u8]) {
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn writes_scans_and_backs_up_a_plus_figure() {
        let card = tempdir().unwrap();
        let source = tempdir().unwrap();
        let backups = tempdir().unwrap();
        fake_mp3(&source.path().join("01.mp3"), b"first");
        fake_mp3(&source.path().join("02.mp3"), b"second");

        let first_backup = write_faba_plus_figure(
            card.path(),
            "3742",
            &[source.path().join("01.mp3"), source.path().join("02.mp3")],
            backups.path(),
        )
        .unwrap();
        assert!(first_backup.is_none());

        let snapshot = scan_card(card.path()).unwrap();
        assert_eq!(snapshot.kind, CardKind::FabaPlus);
        assert_eq!(snapshot.figures.len(), 1);
        assert_eq!(snapshot.figures[0].tracks.len(), 2);
        assert_eq!(
            Tag::read_from_path(card.path().join("K3742/00.faba"))
                .unwrap()
                .title(),
            Some("K3742CP01")
        );
        let first_track = fs::read(card.path().join("K3742/00.faba")).unwrap();
        assert!(first_track
            .windows(3)
            .any(|bytes| bytes == [0x01, 0xff, 0xfe] || bytes == [0x01, 0xfe, 0xff]));
        assert!(first_track
            .windows(b"first".len())
            .any(|bytes| bytes == b"first"));
        assert_eq!(
            fs::read_to_string(card.path().join("K3742/info")).unwrap(),
            r#"{"characterDir":"02190530374200","totalTracks":2}"#
        );

        fake_mp3(&source.path().join("03.mp3"), b"replacement");
        let replacement_backup = write_faba_plus_figure(
            card.path(),
            "3742",
            &[source.path().join("03.mp3")],
            backups.path(),
        )
        .unwrap();
        assert!(replacement_backup.unwrap().join("00.faba").is_file());
        assert_eq!(
            Tag::read_from_path(card.path().join("K3742/00.faba"))
                .unwrap()
                .title(),
            Some("K3742CP01")
        );
    }

    #[test]
    fn discovers_legacy_content_and_keeps_it_read_only() {
        let card = tempdir().unwrap();
        fs::create_dir_all(card.path().join("MKI01/K0010")).unwrap();
        fs::write(card.path().join("MKI01/K0010/CP01.MKI"), b"ciphered").unwrap();

        let snapshot = scan_card(card.path()).unwrap();
        assert_eq!(snapshot.kind, CardKind::LegacyFaba);
        assert!(!snapshot.writable);
        assert_eq!(snapshot.root_path, path_string(&card.path().join("MKI01")));
    }

    #[test]
    fn discovers_player_and_writes_figures_inside_it() {
        let card = tempdir().unwrap();
        let source = tempdir().unwrap();
        let backups = tempdir().unwrap();
        fs::create_dir_all(card.path().join("PLAYER/KTEST")).unwrap();
        fs::create_dir_all(card.path().join("K0001")).unwrap();
        fs::write(card.path().join("K0001/CP00.faba"), b"misplaced").unwrap();
        fake_mp3(&source.path().join("01.mp3"), b"inside-player");

        assert!(looks_like_card(card.path()));
        let snapshot = scan_card(card.path()).unwrap();
        assert_eq!(snapshot.root_path, path_string(&card.path().join("PLAYER")));
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("racine de la carte")));

        write_faba_plus_figure(
            Path::new(&snapshot.root_path),
            "3742",
            &[source.path().join("01.mp3")],
            backups.path(),
        )
        .unwrap();

        assert_eq!(
            Tag::read_from_path(card.path().join("PLAYER/K3742/00.faba"))
                .unwrap()
                .title(),
            Some("K3742CP01")
        );
        assert!(!card.path().join("K3742").exists());
    }

    #[test]
    fn rejects_invalid_ids_and_non_mp3_inputs() {
        let card = tempdir().unwrap();
        let source = tempdir().unwrap();
        let backups = tempdir().unwrap();
        fs::write(source.path().join("track.wav"), b"wav").unwrap();
        fake_mp3(&source.path().join("track.mp3"), b"mp3");

        assert!(validate_custom_figure_id("2000").is_ok());
        assert!(validate_custom_figure_id("8999").is_ok());

        assert!(write_faba_plus_figure(
            card.path(),
            "../1",
            &[source.path().join("track.wav")],
            backups.path(),
        )
        .is_err());
        assert!(write_faba_plus_figure(
            card.path(),
            "3101",
            &[source.path().join("track.wav")],
            backups.path(),
        )
        .is_err());
        for reserved in ["0001", "1234", "9001"] {
            let error = write_faba_plus_figure(
                card.path(),
                reserved,
                &[source.path().join("track.mp3")],
                backups.path(),
            )
            .unwrap_err();
            assert!(format!("{error:#}").contains("réservés par FABA+"));
        }
    }
}
