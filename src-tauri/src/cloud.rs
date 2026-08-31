use crate::storage::{CloudSession, CloudStatus, LibraryDatabase};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fs, path::Path, time::Duration};

const PRODUCTION_ENDPOINT: &str = "https://faba.bo1.be";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudTrack {
    pub position: u16,
    pub label: String,
    #[serde(default)]
    pub audio_available: bool,
    #[serde(default)]
    pub audio_size_bytes: Option<u64>,
    #[serde(default)]
    pub audio_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPlaylist {
    pub figure_id: String,
    pub name: String,
    #[serde(default)]
    pub nfc_payload: String,
    #[serde(default)]
    pub track_count: u16,
    pub tracks: Vec<CloudTrack>,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLibrary {
    pub version: i64,
    pub playlists: Vec<CloudPlaylist>,
    #[serde(default)]
    pub storage_used_bytes: u64,
    #[serde(default)]
    pub storage_limit_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    token: String,
    expires_at: String,
    account: AccountResponse,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
    email: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
}

pub fn endpoint() -> String {
    std::env::var("FABA_CLOUD_URL")
        .unwrap_or_else(|_| PRODUCTION_ENDPOINT.into())
        .trim_end_matches('/')
        .to_owned()
}

pub fn status(database: &LibraryDatabase) -> Result<CloudStatus> {
    database.cloud_status(&endpoint())
}

pub async fn register(
    database: &LibraryDatabase,
    email: &str,
    password: &str,
    display_name: &str,
) -> Result<CloudStatus> {
    authenticate(
        database,
        "/api/v1/auth/register",
        serde_json::json!({
            "email": email,
            "password": password,
            "displayName": display_name,
            "clientName": desktop_client_name(),
        }),
    )
    .await
}

pub async fn login(database: &LibraryDatabase, email: &str, password: &str) -> Result<CloudStatus> {
    authenticate(
        database,
        "/api/v1/auth/login",
        serde_json::json!({
            "email": email,
            "password": password,
            "clientName": desktop_client_name(),
        }),
    )
    .await
}

pub async fn logout(database: &LibraryDatabase) -> Result<CloudStatus> {
    if let Some(session) = database.cloud_session()? {
        let _ = client()?
            .post(format!("{}/api/v1/auth/logout", session.endpoint))
            .bearer_auth(&session.token)
            .send()
            .await;
    }
    database.clear_cloud_session()?;
    status(database)
}

pub async fn library(database: &LibraryDatabase) -> Result<CloudLibrary> {
    let session = require_session(database)?;
    let response = client()?
        .get(format!("{}/api/v1/library", session.endpoint))
        .bearer_auth(&session.token)
        .send()
        .await
        .context("Impossible de joindre FABA Cloud.")?;
    decode(response).await
}

pub async fn save_remote_playlist(
    database: &LibraryDatabase,
    figure_id: &str,
    name: &str,
    labels: &[String],
    reset_audio: bool,
) -> Result<CloudLibrary> {
    if !is_custom_figure_id(figure_id) {
        bail!("L'identifiant doit être compris entre 2000 et 8999.");
    }
    if labels.is_empty() || labels.len() > 99 {
        bail!("Une playlist doit contenir entre 1 et 99 pistes.");
    }
    let session = require_session(database)?;
    let tracks = labels
        .iter()
        .enumerate()
        .map(|(position, label)| {
            serde_json::json!({
                "position": position,
                "label": label,
            })
        })
        .collect::<Vec<_>>();
    let response = client()?
        .put(format!(
            "{}/api/v1/library/playlists/{figure_id}",
            session.endpoint
        ))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({
            "figureId": figure_id,
            "name": name,
            "tracks": tracks,
            "resetAudio": reset_audio,
        }))
        .send()
        .await
        .context("Impossible d'enregistrer la playlist dans FABA Cloud.")?;
    decode(response).await
}

pub async fn delete_remote_playlist(database: &LibraryDatabase, figure_id: &str) -> Result<()> {
    if !is_custom_figure_id(figure_id) {
        bail!("L'identifiant doit être compris entre 2000 et 8999.");
    }
    let session = require_session(database)?;
    let response = client()?
        .delete(format!(
            "{}/api/v1/library/playlists/{figure_id}",
            session.endpoint
        ))
        .bearer_auth(&session.token)
        .send()
        .await
        .context("Impossible de supprimer la playlist de FABA Cloud.")?;
    let _: CloudLibrary = decode(response).await?;
    Ok(())
}

pub async fn download_track_bytes(
    database: &LibraryDatabase,
    figure_id: &str,
    position: u16,
) -> Result<Vec<u8>> {
    let session = require_session(database)?;
    let response = client()?
        .get(format!(
            "{}/api/v1/library/playlists/{figure_id}/tracks/{position}/audio",
            session.endpoint
        ))
        .bearer_auth(&session.token)
        .send()
        .await
        .context("Impossible de télécharger une piste depuis FABA Cloud.")?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .context("Piste cloud illisible pendant le téléchargement.")?;
    if !status.is_success() {
        return Err(error_from_bytes(status, &bytes));
    }
    Ok(bytes.to_vec())
}

async fn authenticate(
    database: &LibraryDatabase,
    route: &str,
    payload: serde_json::Value,
) -> Result<CloudStatus> {
    let endpoint = endpoint();
    let response = client()?
        .post(format!("{endpoint}{route}"))
        .json(&payload)
        .send()
        .await
        .context("Impossible de joindre FABA Cloud.")?;
    let response: SessionResponse = decode(response).await?;
    database.save_cloud_session(&CloudSession {
        endpoint,
        email: response.account.email,
        display_name: response.account.display_name,
        token: response.token,
        expires_at: response.expires_at,
        last_sync_at: None,
    })?;
    status(database)
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .context("Réponse FABA Cloud illisible.")?;
    if status.is_success() {
        return serde_json::from_slice(&bytes).context("Réponse FABA Cloud invalide.");
    }
    if status == StatusCode::UNAUTHORIZED {
        bail!("Session expirée. Reconnectez-vous à FABA Cloud.");
    }
    let message = serde_json::from_slice::<ErrorEnvelope>(&bytes)
        .map(|body| body.error.message)
        .unwrap_or_else(|_| format!("FABA Cloud a répondu avec l'erreur {status}."));
    Err(anyhow!(message))
}

fn require_session(database: &LibraryDatabase) -> Result<CloudSession> {
    database
        .cloud_session()?
        .ok_or_else(|| anyhow!("Connectez-vous d'abord à FABA Cloud."))
}

fn is_custom_figure_id(value: &str) -> bool {
    value.len() == 4
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(value.as_bytes()[0], b'2'..=b'8')
}

fn client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .user_agent(desktop_client_name())
        .build()
        .map_err(Into::into)
}

pub async fn upload_track(
    session: &CloudSession,
    figure_id: &str,
    position: u16,
    path: &Path,
) -> Result<()> {
    let bytes = fs::read(path)
        .with_context(|| format!("Impossible de lire la piste {}.", path.display()))?;
    let response = client()?
        .put(format!(
            "{}/api/v1/library/playlists/{figure_id}/tracks/{position}/audio",
            session.endpoint
        ))
        .bearer_auth(&session.token)
        .header(reqwest::header::CONTENT_TYPE, "audio/mpeg")
        .body(bytes)
        .send()
        .await
        .context("Impossible d'envoyer une piste vers FABA Cloud.")?;
    let _: serde_json::Value = decode(response).await?;
    Ok(())
}

fn error_from_bytes(status: StatusCode, bytes: &[u8]) -> anyhow::Error {
    if status == StatusCode::UNAUTHORIZED {
        return anyhow!("Session expirée. Reconnectez-vous à FABA Cloud.");
    }
    let message = serde_json::from_slice::<ErrorEnvelope>(bytes)
        .map(|body| body.error.message)
        .unwrap_or_else(|_| format!("FABA Cloud a répondu avec l'erreur {status}."));
    anyhow!(message)
}

fn desktop_client_name() -> String {
    format!(
        "FABA+ Custom Editor/{} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_custom_ids_are_cloud_eligible() {
        for valid in ["2000", "3101", "8999"] {
            assert!(is_custom_figure_id(valid));
        }
        for invalid in ["0001", "1234", "9001", "oops"] {
            assert!(!is_custom_figure_id(invalid));
        }
    }
}
