mod auth;
mod error;
mod library;
mod models;
mod storage;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
};
use error::ApiError;
use models::{
    AccountResponse, AudioUploadResponse, LibraryResponse, LoginRequest, PlaylistPayload,
    RegisterRequest, SessionResponse, SyncRequest,
};
use serde_json::json;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    session_days: i64,
    storage_dir: PathBuf,
    max_track_bytes: usize,
    max_account_bytes: u64,
    max_total_bytes: u64,
    auth_slots: Arc<Semaphore>,
    upload_slots: Arc<Semaphore>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        session_days: i64,
        storage_dir: PathBuf,
        max_track_bytes: usize,
        max_account_bytes: u64,
        max_total_bytes: u64,
    ) -> Self {
        Self {
            pool,
            session_days,
            storage_dir,
            max_track_bytes,
            max_account_bytes,
            max_total_bytes,
            auth_slots: Arc::new(Semaphore::new(4)),
            upload_slots: Arc::new(Semaphore::new(2)),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/me", get(me))
        .route("/api/v1/library", get(get_library))
        .route("/api/v1/library/sync", post(sync_library))
        .route(
            "/api/v1/library/playlists/{figure_id}",
            put(upsert_playlist).delete(delete_playlist),
        )
        .route(
            "/api/v1/library/playlists/{figure_id}/tracks/{position}/audio",
            get(download_audio).put(upload_audio),
        )
        .layer(middleware::map_response(add_security_headers))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
            AUTHORIZATION,
        )))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
                .on_failure(()),
        )
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "status": "ok", "service": "faba-cloud" })))
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    auth::register(&state, request).await.map(Json)
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    auth::login(&state, request).await.map(Json)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    auth::logout(&state, &headers).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountResponse>, ApiError> {
    auth::authenticate(&state, &headers)
        .await
        .map(AccountResponse::from)
        .map(Json)
}

async fn get_library(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<LibraryResponse>, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    library::get_library(&state, &user).await.map(Json)
}

async fn sync_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> Result<Json<LibraryResponse>, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    library::sync_playlists(&state, &user, request.playlists, request.replace_missing)
        .await
        .map(Json)
}

async fn upsert_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(figure_id): Path<String>,
    Json(playlist): Json<PlaylistPayload>,
) -> Result<Json<LibraryResponse>, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    library::upsert_playlist(&state, &user, &figure_id, playlist)
        .await
        .map(Json)
}

async fn delete_playlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(figure_id): Path<String>,
) -> Result<Json<LibraryResponse>, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    library::delete_playlist(&state, &user, &figure_id)
        .await
        .map(Json)
}

async fn upload_audio(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((figure_id, position)): Path<(String, u16)>,
    body: Body,
) -> Result<Json<AudioUploadResponse>, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    storage::upload_audio(&state, &user, &figure_id, position, body)
        .await
        .map(Json)
}

async fn download_audio(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((figure_id, position)): Path<(String, u16)>,
) -> Result<axum::response::Response, ApiError> {
    let user = auth::authenticate(&state, &headers).await?;
    storage::download_audio(&state, &user, &figure_id, position).await
}

async fn add_security_headers(mut response: axum::response::Response) -> axum::response::Response {
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("cache-control", "no-store".parse().unwrap());
    response
}
