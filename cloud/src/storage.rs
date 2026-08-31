use crate::{
    AppState,
    error::ApiError,
    library::validate_figure_id,
    models::{AudioUploadResponse, AuthUser},
};
use axum::{
    body::{Body, to_bytes},
    http::{Response, header},
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub async fn upload_audio(
    state: &AppState,
    user: &AuthUser,
    figure_id: &str,
    position: u16,
    body: Body,
) -> Result<AudioUploadResponse, ApiError> {
    let _upload_slot = state
        .upload_slots
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    validate_figure_id(figure_id)?;
    if position > 98 {
        return Err(ApiError::Validation(
            "La position de piste doit être comprise entre 0 et 98.".into(),
        ));
    }
    let bytes = to_bytes(body, state.max_track_bytes).await.map_err(|_| {
        ApiError::PayloadTooLarge(format!(
            "Une piste ne peut pas dépasser {} Mo.",
            state.max_track_bytes / 1024 / 1024
        ))
    })?;
    if bytes.is_empty() || !looks_like_mp3(&bytes) {
        return Err(ApiError::Validation(
            "Le fichier doit être un MP3 valide et non vide.".into(),
        ));
    }

    let mut transaction = state.pool.begin().await?;
    // Serialize quota checks so simultaneous uploads cannot overrun the global limit.
    sqlx::query("SELECT pg_advisory_xact_lock(7002008999)")
        .execute(&mut *transaction)
        .await?;
    let track = sqlx::query(
        "SELECT audio_size_bytes FROM playlist_tracks
         WHERE user_id=$1 AND figure_id=$2 AND position=$3 FOR UPDATE",
    )
    .bind(user.id)
    .bind(figure_id)
    .bind(position as i16)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::NotFound("Cette piste n'existe pas dans la bibliothèque.".into()))?;
    let previous_size = track
        .get::<Option<i64>, _>("audio_size_bytes")
        .unwrap_or(0)
        .max(0) as u64;
    let used = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(audio_size_bytes), 0)::bigint
         FROM playlist_tracks WHERE user_id=$1",
    )
    .bind(user.id)
    .fetch_one(&mut *transaction)
    .await?
    .max(0) as u64;
    let new_size = bytes.len() as u64;
    if used.saturating_sub(previous_size).saturating_add(new_size) > state.max_account_bytes {
        return Err(ApiError::PayloadTooLarge(format!(
            "Le quota cloud de {} Go est atteint.",
            state.max_account_bytes / 1024 / 1024 / 1024
        )));
    }
    let total_used = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(audio_size_bytes), 0)::bigint FROM playlist_tracks",
    )
    .fetch_one(&mut *transaction)
    .await?
    .max(0) as u64;
    if total_used
        .saturating_sub(previous_size)
        .saturating_add(new_size)
        > state.max_total_bytes
    {
        return Err(ApiError::PayloadTooLarge(
            "Le stockage FABA Cloud est actuellement complet.".into(),
        ));
    }

    let sha256 = hex::encode(Sha256::digest(&bytes));
    let destination = audio_path(state, user.id, figure_id, position);
    let parent = destination
        .parent()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("invalid storage path")))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(internal_io)?;
    let temporary = parent.join(format!(".upload-{}.tmp", Uuid::new_v4()));
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(internal_io)?;
    file.write_all(&bytes).await.map_err(internal_io)?;
    file.sync_all().await.map_err(internal_io)?;
    drop(file);
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(internal_io(error));
    }

    sqlx::query(
        "UPDATE playlist_tracks SET audio_size_bytes=$4, audio_sha256=$5, audio_updated_at=NOW()
         WHERE user_id=$1 AND figure_id=$2 AND position=$3",
    )
    .bind(user.id)
    .bind(figure_id)
    .bind(position as i16)
    .bind(new_size as i64)
    .bind(&sha256)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE users SET library_version=library_version+1, updated_at=NOW() WHERE id=$1")
        .bind(user.id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok(AudioUploadResponse {
        figure_id: figure_id.to_owned(),
        position,
        audio_size_bytes: new_size,
        audio_sha256: sha256,
    })
}

pub async fn download_audio(
    state: &AppState,
    user: &AuthUser,
    figure_id: &str,
    position: u16,
) -> Result<axum::response::Response, ApiError> {
    validate_figure_id(figure_id)?;
    let size = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT audio_size_bytes FROM playlist_tracks
         WHERE user_id=$1 AND figure_id=$2 AND position=$3",
    )
    .bind(user.id)
    .bind(figure_id)
    .bind(position as i16)
    .fetch_optional(&state.pool)
    .await?
    .flatten()
    .ok_or_else(|| {
        ApiError::NotFound("Aucun fichier audio n'est disponible pour cette piste.".into())
    })?;
    let file = tokio::fs::File::open(audio_path(state, user.id, figure_id, position))
        .await
        .map_err(|error| {
            tracing::error!(%error, %figure_id, %position, "audio file missing from storage");
            ApiError::Internal(anyhow::anyhow!("audio file missing"))
        })?;
    Response::builder()
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, size)
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"K{figure_id}-CP{:02}.mp3\"",
                position + 1
            ),
        )
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|error| ApiError::Internal(error.into()))
}

pub async fn remove_playlist(state: &AppState, user_id: Uuid, figure_id: &str) {
    let _ = tokio::fs::remove_dir_all(playlist_dir(state, user_id, figure_id)).await;
}

pub async fn prune_playlist(state: &AppState, user_id: Uuid, figure_id: &str, track_count: usize) {
    let directory = playlist_dir(state, user_id, figure_id);
    let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let should_remove = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".mp3"))
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|position| position >= track_count);
        if should_remove {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

fn playlist_dir(state: &AppState, user_id: Uuid, figure_id: &str) -> PathBuf {
    state.storage_dir.join(user_id.to_string()).join(figure_id)
}

fn audio_path(state: &AppState, user_id: Uuid, figure_id: &str, position: u16) -> PathBuf {
    playlist_dir(state, user_id, figure_id).join(format!("{position:02}.mp3"))
}

fn looks_like_mp3(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"ID3") {
        return true;
    }
    bytes
        .windows(2)
        .take(64 * 1024)
        .any(|pair| pair[0] == 0xff && pair[1] & 0xe0 == 0xe0)
}

fn internal_io(error: std::io::Error) -> ApiError {
    ApiError::Internal(anyhow::Error::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_mp3_headers() {
        assert!(looks_like_mp3(b"ID3\x04\x00\x00"));
        assert!(looks_like_mp3(b"\xff\xfb\x90\x64"));
        assert!(!looks_like_mp3(b"RIFF----WAVE"));
    }
}
