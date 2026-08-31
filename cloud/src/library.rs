use crate::{
    AppState,
    error::ApiError,
    models::{AuthUser, LibraryResponse, PlaylistPayload, PlaylistResponse, TrackResponse},
};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Transaction};
use std::collections::HashMap;

pub async fn get_library(state: &AppState, user: &AuthUser) -> Result<LibraryResponse, ApiError> {
    let playlist_rows = sqlx::query(
        "SELECT figure_id, name, nfc_payload, track_count, updated_at
         FROM playlists WHERE user_id=$1 ORDER BY figure_id",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let track_rows = sqlx::query(
        "SELECT figure_id, position, label, audio_size_bytes, audio_sha256
         FROM playlist_tracks WHERE user_id=$1 ORDER BY figure_id, position",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let mut tracks = HashMap::<String, Vec<TrackResponse>>::new();
    for row in track_rows {
        tracks
            .entry(row.get::<String, _>("figure_id").trim().to_owned())
            .or_default()
            .push(TrackResponse {
                position: row.get::<i16, _>("position") as u16,
                label: row.get("label"),
                audio_available: row.get::<Option<i64>, _>("audio_size_bytes").is_some(),
                audio_size_bytes: row
                    .get::<Option<i64>, _>("audio_size_bytes")
                    .map(|value| value.max(0) as u64),
                audio_sha256: row.get("audio_sha256"),
            });
    }
    let playlists = playlist_rows
        .into_iter()
        .map(|row| {
            let figure_id = row.get::<String, _>("figure_id").trim().to_owned();
            PlaylistResponse {
                tracks: tracks.remove(&figure_id).unwrap_or_default(),
                figure_id,
                name: row.get("name"),
                nfc_payload: row.get::<String, _>("nfc_payload").trim().to_owned(),
                track_count: row.get::<i16, _>("track_count") as u16,
                updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
            }
        })
        .collect();
    let version = sqlx::query_scalar::<_, i64>("SELECT library_version FROM users WHERE id=$1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    let storage_used_bytes = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(audio_size_bytes), 0)::bigint
         FROM playlist_tracks WHERE user_id=$1",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?
    .max(0) as u64;
    Ok(LibraryResponse {
        version,
        playlists,
        storage_used_bytes,
        storage_limit_bytes: state.max_account_bytes,
    })
}

pub async fn upsert_playlist(
    state: &AppState,
    user: &AuthUser,
    figure_id: &str,
    mut playlist: PlaylistPayload,
) -> Result<LibraryResponse, ApiError> {
    if playlist.figure_id != figure_id {
        return Err(ApiError::Validation(
            "L'identifiant de la playlist ne correspond pas à l'URL.".into(),
        ));
    }
    validate_playlist(&mut playlist)?;
    let mut transaction = state.pool.begin().await?;
    upsert_in_transaction(&mut transaction, user, &playlist).await?;
    bump_version(&mut transaction, user).await?;
    transaction.commit().await?;
    if playlist.reset_audio {
        crate::storage::remove_playlist(state, user.id, &playlist.figure_id).await;
    } else {
        crate::storage::prune_playlist(state, user.id, &playlist.figure_id, playlist.tracks.len())
            .await;
    }
    get_library(state, user).await
}

pub async fn sync_playlists(
    state: &AppState,
    user: &AuthUser,
    mut playlists: Vec<PlaylistPayload>,
    replace_missing: bool,
) -> Result<LibraryResponse, ApiError> {
    if playlists.len() > 500 {
        return Err(ApiError::Validation(
            "Une synchronisation est limitée à 500 playlists.".into(),
        ));
    }
    for playlist in &mut playlists {
        validate_playlist(playlist)?;
    }
    let mut transaction = state.pool.begin().await?;
    let mut removed_figure_ids = Vec::new();
    if replace_missing {
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT figure_id::text FROM playlists WHERE user_id=$1",
        )
        .bind(user.id)
        .fetch_all(&mut *transaction)
        .await?;
        let figure_ids = playlists
            .iter()
            .map(|playlist| playlist.figure_id.clone())
            .collect::<Vec<_>>();
        removed_figure_ids = existing
            .into_iter()
            .map(|value| value.trim().to_owned())
            .filter(|value| !figure_ids.contains(value))
            .collect();
        if figure_ids.is_empty() {
            sqlx::query("DELETE FROM playlists WHERE user_id=$1")
                .bind(user.id)
                .execute(&mut *transaction)
                .await?;
        } else {
            sqlx::query(
                "DELETE FROM playlists
                 WHERE user_id=$1 AND NOT (figure_id::text = ANY($2::text[]))",
            )
            .bind(user.id)
            .bind(figure_ids)
            .execute(&mut *transaction)
            .await?;
        }
    }
    for playlist in &playlists {
        upsert_in_transaction(&mut transaction, user, playlist).await?;
    }
    if replace_missing || !playlists.is_empty() {
        bump_version(&mut transaction, user).await?;
    }
    transaction.commit().await?;
    for figure_id in removed_figure_ids {
        crate::storage::remove_playlist(state, user.id, &figure_id).await;
    }
    for playlist in &playlists {
        crate::storage::prune_playlist(state, user.id, &playlist.figure_id, playlist.tracks.len())
            .await;
    }
    get_library(state, user).await
}

pub async fn delete_playlist(
    state: &AppState,
    user: &AuthUser,
    figure_id: &str,
) -> Result<LibraryResponse, ApiError> {
    validate_figure_id(figure_id)?;
    let mut transaction = state.pool.begin().await?;
    let result = sqlx::query("DELETE FROM playlists WHERE user_id=$1 AND figure_id=$2")
        .bind(user.id)
        .bind(figure_id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() > 0 {
        bump_version(&mut transaction, user).await?;
    }
    transaction.commit().await?;
    if result.rows_affected() > 0 {
        crate::storage::remove_playlist(state, user.id, figure_id).await;
    }
    get_library(state, user).await
}

async fn upsert_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
    playlist: &PlaylistPayload,
) -> Result<(), ApiError> {
    let nfc_payload = format!("02190530{}00", playlist.figure_id);
    sqlx::query(
        "INSERT INTO playlists(user_id, figure_id, name, nfc_payload, track_count, updated_at)
         VALUES($1, $2, $3, $4, $5, NOW())
         ON CONFLICT(user_id, figure_id) DO UPDATE SET
           name=EXCLUDED.name,
           nfc_payload=EXCLUDED.nfc_payload,
           track_count=EXCLUDED.track_count,
           updated_at=NOW()",
    )
    .bind(user.id)
    .bind(&playlist.figure_id)
    .bind(&playlist.name)
    .bind(nfc_payload)
    .bind(playlist.tracks.len() as i16)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "DELETE FROM playlist_tracks
         WHERE user_id=$1 AND figure_id=$2 AND position >= $3",
    )
    .bind(user.id)
    .bind(&playlist.figure_id)
    .bind(playlist.tracks.len() as i16)
    .execute(&mut **transaction)
    .await?;
    for track in &playlist.tracks {
        sqlx::query(
            "INSERT INTO playlist_tracks(user_id, figure_id, position, label)
             VALUES($1, $2, $3, $4)
             ON CONFLICT(user_id, figure_id, position) DO UPDATE SET label=EXCLUDED.label",
        )
        .bind(user.id)
        .bind(&playlist.figure_id)
        .bind(track.position as i16)
        .bind(&track.label)
        .execute(&mut **transaction)
        .await?;
    }
    if playlist.reset_audio {
        sqlx::query(
            "UPDATE playlist_tracks
             SET audio_size_bytes=NULL, audio_sha256=NULL, audio_updated_at=NULL
             WHERE user_id=$1 AND figure_id=$2",
        )
        .bind(user.id)
        .bind(&playlist.figure_id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn bump_version(
    transaction: &mut Transaction<'_, Postgres>,
    user: &AuthUser,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE users SET library_version=library_version+1, updated_at=NOW() WHERE id=$1")
        .bind(user.id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn validate_playlist(playlist: &mut PlaylistPayload) -> Result<(), ApiError> {
    validate_figure_id(&playlist.figure_id)?;
    playlist.name = playlist.name.trim().chars().take(100).collect();
    if playlist.name.is_empty() {
        playlist.name = format!("Figurine K{}", playlist.figure_id);
    }
    if playlist.tracks.is_empty() || playlist.tracks.len() > 99 {
        return Err(ApiError::Validation(
            "Une playlist doit contenir entre 1 et 99 pistes.".into(),
        ));
    }
    playlist.tracks.sort_by_key(|track| track.position);
    for (index, track) in playlist.tracks.iter_mut().enumerate() {
        if track.position as usize != index {
            return Err(ApiError::Validation(
                "Les positions des pistes doivent être continues à partir de zéro.".into(),
            ));
        }
        track.label = track.label.trim().chars().take(200).collect();
        if track.label.is_empty() {
            track.label = format!("Piste {}", index + 1);
        }
    }
    Ok(())
}

pub(crate) fn validate_figure_id(value: &str) -> Result<(), ApiError> {
    let valid = value.len() == 4
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(value.as_bytes()[0], b'2'..=b'8');
    valid.then_some(()).ok_or_else(|| {
        ApiError::Validation("Utilisez un identifiant personnalisé entre 2000 et 8999.".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackPayload;

    #[test]
    fn rejects_reserved_ids_and_normalizes_labels() {
        for reserved in ["0001", "1234", "9001", "abcd"] {
            assert!(validate_figure_id(reserved).is_err());
        }
        for valid in ["2000", "3101", "8999"] {
            assert!(validate_figure_id(valid).is_ok());
        }
        let mut playlist = PlaylistPayload {
            figure_id: "3101".into(),
            name: "  Histoires du soir  ".into(),
            tracks: vec![TrackPayload {
                position: 0,
                label: "  Introduction  ".into(),
            }],
            reset_audio: false,
        };
        validate_playlist(&mut playlist).unwrap();
        assert_eq!(playlist.name, "Histoires du soir");
        assert_eq!(playlist.tracks[0].label, "Introduction");
    }
}
