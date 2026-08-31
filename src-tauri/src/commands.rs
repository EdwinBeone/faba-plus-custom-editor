use crate::diagnostics::{DiagnosticLogger, DiagnosticReport};
use crate::domain::{
    delete_faba_plus_figure, ensure_editable, export_figure as export_figure_to, looks_like_card,
    require_figure, scan_card as scan_card_path, write_faba_plus_figure_with_trace, CardKind,
    CardSnapshot,
};
use crate::storage::{LibraryDatabase, RecentCard};
use serde::Serialize;
use std::path::{Path, PathBuf};
use sysinfo::Disks;
use tauri::{AppHandle, Manager, State};

#[derive(Debug)]
pub struct AppState {
    pub database: LibraryDatabase,
    pub diagnostics: DiagnosticLogger,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCard {
    id: String,
    label: String,
    mount_path: String,
    removable: bool,
    likely_faba: bool,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult {
    snapshot: CardSnapshot,
    backup_path: Option<String>,
    message: String,
}

#[tauri::command]
pub fn detect_cards(state: State<'_, AppState>) -> Vec<DetectedCard> {
    let disks = Disks::new_with_refreshed_list();
    let mut cards = disks
        .list()
        .iter()
        .filter_map(|disk| {
            let mount = disk.mount_point();
            let likely_faba = looks_like_card(mount);
            if !disk.is_removable() && !likely_faba {
                return None;
            }
            let mount_path = mount.to_string_lossy().into_owned();
            Some(DetectedCard {
                id: mount_path.clone(),
                label: disk.name().to_string_lossy().into_owned(),
                mount_path,
                removable: disk.is_removable(),
                likely_faba,
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
            })
        })
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| {
        right
            .likely_faba
            .cmp(&left.likely_faba)
            .then(left.label.cmp(&right.label))
    });
    state
        .diagnostics
        .info("cards.detect", format!("removable_or_faba={}", cards.len()));
    cards
}

#[tauri::command]
pub fn recent_cards(state: State<'_, AppState>) -> Result<Vec<RecentCard>, String> {
    state.database.recent_cards().map_err(display_error)
}

#[tauri::command]
pub fn scan_card(path: String, state: State<'_, AppState>) -> Result<CardSnapshot, String> {
    state
        .diagnostics
        .info("card.scan.start", format!("path={path}"));
    match load_snapshot(&path, &state.database) {
        Ok(snapshot) => {
            state.diagnostics.info(
                "card.scan.success",
                format!(
                    "path={} kind={:?} figures={} writable={}",
                    snapshot.root_path,
                    snapshot.kind,
                    snapshot.figures.len(),
                    snapshot.writable
                ),
            );
            Ok(snapshot)
        }
        Err(error) => {
            state
                .diagnostics
                .error("card.scan.error", format!("path={path} error={error}"));
            Err(error)
        }
    }
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticReport, String> {
    state.diagnostics.report().map_err(display_error)
}

#[tauri::command]
pub fn clear_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticReport, String> {
    state.diagnostics.clear().map_err(display_error)?;
    state
        .diagnostics
        .info("diagnostics.clear", "journal effacé par l'utilisateur");
    state.diagnostics.report().map_err(display_error)
}

#[tauri::command]
pub fn save_figure(
    app: AppHandle,
    root_path: String,
    figure_id: String,
    custom_name: String,
    audio_paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<MutationResult, String> {
    state.diagnostics.info(
        "figure.save.start",
        format!(
            "card={root_path} figure=K{figure_id} tracks={}",
            audio_paths.len()
        ),
    );
    let before = scan_card_path(Path::new(&root_path))
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    ensure_editable(&before)
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;

    let backup_root = backup_root(&app, &root_path)
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    let paths = audio_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    let labels = paths
        .iter()
        .map(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Piste")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let trace = |step: &str| {
        state.diagnostics.info(
            "figure.save.step",
            format!(
                "card={} figure=K{} step={step}",
                before.root_path, figure_id
            ),
        );
    };
    let backup = write_faba_plus_figure_with_trace(
        Path::new(&before.root_path),
        &figure_id,
        &paths,
        &backup_root,
        &trace,
    )
    .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;

    let mut snapshot = scan_card_path(Path::new(&before.root_path))
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    state
        .database
        .sync_snapshot(&snapshot)
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    let name = custom_name.trim();
    state
        .database
        .set_figure_name(
            &snapshot.root_path,
            &figure_id,
            (!name.is_empty()).then_some(name),
        )
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    state
        .database
        .set_track_labels(&snapshot.root_path, &figure_id, &labels)
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;
    state
        .database
        .decorate_snapshot(&mut snapshot)
        .map_err(|error| logged_error(&state.diagnostics, "figure.save.error", error))?;

    state.diagnostics.info(
        "figure.save.success",
        format!("card={} figure=K{figure_id}", snapshot.root_path),
    );
    Ok(MutationResult {
        snapshot,
        backup_path: backup.map(|path| path.to_string_lossy().into_owned()),
        message: if before.figures.iter().any(|figure| figure.id == figure_id) {
            "Figurine remplacée et ancienne version sauvegardée.".into()
        } else {
            "Figurine ajoutée à la carte.".into()
        },
    })
}

#[tauri::command]
pub fn rename_figure(
    root_path: String,
    figure_id: String,
    custom_name: String,
    state: State<'_, AppState>,
) -> Result<CardSnapshot, String> {
    state.diagnostics.info(
        "figure.rename.start",
        format!("card={root_path} figure=K{figure_id}"),
    );
    let snapshot = scan_card_path(Path::new(&root_path))
        .map_err(|error| logged_error(&state.diagnostics, "figure.rename.error", error))?;
    require_figure(&snapshot, &figure_id)
        .map_err(|error| logged_error(&state.diagnostics, "figure.rename.error", error))?;
    state
        .database
        .sync_snapshot(&snapshot)
        .map_err(|error| logged_error(&state.diagnostics, "figure.rename.error", error))?;
    let name = custom_name.trim();
    state
        .database
        .set_figure_name(
            &snapshot.root_path,
            &figure_id,
            (!name.is_empty()).then_some(name),
        )
        .map_err(|error| logged_error(&state.diagnostics, "figure.rename.error", error))?;
    let result = load_snapshot(&snapshot.root_path, &state.database)
        .map_err(|error| logged_error(&state.diagnostics, "figure.rename.error", error))?;
    state.diagnostics.info(
        "figure.rename.success",
        format!("card={} figure=K{figure_id}", snapshot.root_path),
    );
    Ok(result)
}

#[tauri::command]
pub fn delete_figure(
    app: AppHandle,
    root_path: String,
    figure_id: String,
    state: State<'_, AppState>,
) -> Result<MutationResult, String> {
    state.diagnostics.info(
        "figure.delete.start",
        format!("card={root_path} figure=K{figure_id}"),
    );
    let before = scan_card_path(Path::new(&root_path))
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    ensure_editable(&before)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    require_figure(&before, &figure_id)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    if before.kind != CardKind::FabaPlus {
        let error = "La suppression est réservée aux cartes FABA+.";
        state.diagnostics.error("figure.delete.error", error);
        return Err(error.into());
    }
    let backup_path = backup_root(&app, &root_path)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    let backup = delete_faba_plus_figure(Path::new(&before.root_path), &figure_id, &backup_path)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    state
        .database
        .remove_figure(&before.root_path, &figure_id)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    let snapshot = load_snapshot(&before.root_path, &state.database)
        .map_err(|error| logged_error(&state.diagnostics, "figure.delete.error", error))?;
    state.diagnostics.info(
        "figure.delete.success",
        format!("card={} figure=K{figure_id}", before.root_path),
    );
    Ok(MutationResult {
        snapshot,
        backup_path: Some(backup.to_string_lossy().into_owned()),
        message: "Figurine retirée ; une sauvegarde locale a été conservée.".into(),
    })
}

#[tauri::command]
pub fn export_figure(
    root_path: String,
    figure_id: String,
    destination_path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.diagnostics.info(
        "figure.export.start",
        format!("card={root_path} figure=K{figure_id} destination={destination_path}"),
    );
    let snapshot = scan_card_path(Path::new(&root_path))
        .map_err(|error| logged_error(&state.diagnostics, "figure.export.error", error))?;
    require_figure(&snapshot, &figure_id)
        .map_err(|error| logged_error(&state.diagnostics, "figure.export.error", error))?;
    let exported = export_figure_to(
        Path::new(&snapshot.root_path),
        &figure_id,
        Path::new(&destination_path),
    )
    .map(|path| path.to_string_lossy().into_owned())
    .map_err(|error| logged_error(&state.diagnostics, "figure.export.error", error))?;
    state.diagnostics.info(
        "figure.export.success",
        format!(
            "card={} figure=K{figure_id} path={exported}",
            snapshot.root_path
        ),
    );
    Ok(exported)
}

fn load_snapshot(path: &str, database: &LibraryDatabase) -> Result<CardSnapshot, String> {
    let mut snapshot = scan_card_path(Path::new(path)).map_err(display_error)?;
    database.sync_snapshot(&snapshot).map_err(display_error)?;
    database
        .decorate_snapshot(&mut snapshot)
        .map_err(display_error)?;
    Ok(snapshot)
}

fn backup_root(app: &AppHandle, card_root: &str) -> Result<PathBuf, String> {
    let card_name = Path::new(card_root)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("card")
        .replace(|character: char| !character.is_ascii_alphanumeric(), "-");
    app.path()
        .app_data_dir()
        .map(|path| path.join("backups").join(card_name))
        .map_err(display_error)
}

fn display_error(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

fn logged_error(
    diagnostics: &DiagnosticLogger,
    event: &str,
    error: impl std::fmt::Display,
) -> String {
    let message = format!("{error:#}");
    diagnostics.error(event, &message);
    message
}
