use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub library_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub client_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub client_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub account: AccountResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub library_version: i64,
}

impl From<AuthUser> for AccountResponse {
    fn from(user: AuthUser) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            library_version: user.library_version,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPayload {
    pub position: u16,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackResponse {
    pub position: u16,
    pub label: String,
    pub audio_available: bool,
    pub audio_size_bytes: Option<u64>,
    pub audio_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistPayload {
    pub figure_id: String,
    pub name: String,
    pub tracks: Vec<TrackPayload>,
    #[serde(default)]
    pub reset_audio: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistResponse {
    pub figure_id: String,
    pub name: String,
    pub nfc_payload: String,
    pub track_count: u16,
    pub tracks: Vec<TrackResponse>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryResponse {
    pub version: i64,
    pub playlists: Vec<PlaylistResponse>,
    pub storage_used_bytes: u64,
    pub storage_limit_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub playlists: Vec<PlaylistPayload>,
    #[serde(default)]
    pub replace_missing: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioUploadResponse {
    pub figure_id: String,
    pub position: u16,
    pub audio_size_bytes: u64,
    pub audio_sha256: String,
}
