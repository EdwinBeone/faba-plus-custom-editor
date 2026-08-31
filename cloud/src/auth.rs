use crate::{
    AppState,
    error::ApiError,
    models::{AuthUser, LoginRequest, RegisterRequest, SessionResponse},
};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::http::{HeaderMap, header::AUTHORIZATION};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand::random;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub async fn register(
    state: &AppState,
    request: RegisterRequest,
) -> Result<SessionResponse, ApiError> {
    let _password_slot = state
        .auth_slots
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    let email = normalize_email(&request.email)?;
    let display_name = validate_display_name(&request.display_name)?;
    validate_password(&request.password)?;
    let password = request.password;
    let password_hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?
    .map_err(|error| ApiError::Internal(anyhow::anyhow!(error)))?;

    let user_id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO users(id, email, display_name, password_hash) VALUES($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(&email)
    .bind(&display_name)
    .bind(password_hash)
    .execute(&state.pool)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .and_then(|value| value.code())
            .as_deref()
            == Some("23505")
        {
            return Err(ApiError::AccountExists);
        }
        return Err(error.into());
    }

    issue_session(
        state,
        AuthUser {
            id: user_id,
            email,
            display_name,
            library_version: 0,
        },
        request.client_name,
    )
    .await
}

pub async fn login(state: &AppState, request: LoginRequest) -> Result<SessionResponse, ApiError> {
    let _password_slot = state
        .auth_slots
        .acquire()
        .await
        .map_err(|error| ApiError::Internal(error.into()))?;
    let email = normalize_email(&request.email)?;
    let row = sqlx::query(
        "SELECT id, email, display_name, password_hash, library_version FROM users WHERE email=$1",
    )
    .bind(email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let password_hash: String = row.get("password_hash");
    let password = request.password;
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&password_hash).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(|error| ApiError::Internal(error.into()))?;
    if !valid {
        return Err(ApiError::Unauthorized);
    }
    issue_session(
        state,
        AuthUser {
            id: row.get("id"),
            email: row.get("email"),
            display_name: row.get("display_name"),
            library_version: row.get("library_version"),
        },
        request.client_name,
    )
    .await
}

pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, ApiError> {
    let token = bearer_token(headers)?;
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    let row = sqlx::query(
        "SELECT u.id, u.email, u.display_name, u.library_version, s.id AS session_id
         FROM sessions s
         JOIN users u ON u.id=s.user_id
         WHERE s.token_hash=$1 AND s.expires_at > NOW()",
    )
    .bind(token_hash)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let session_id: Uuid = row.get("session_id");
    let _ = sqlx::query("UPDATE sessions SET last_used_at=NOW() WHERE id=$1")
        .bind(session_id)
        .execute(&state.pool)
        .await;
    Ok(AuthUser {
        id: row.get("id"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        library_version: row.get("library_version"),
    })
}

pub async fn logout(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = bearer_token(headers)?;
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
        .bind(token_hash)
        .execute(&state.pool)
        .await?;
    Ok(())
}

async fn issue_session(
    state: &AppState,
    user: AuthUser,
    client_name: Option<String>,
) -> Result<SessionResponse, ApiError> {
    let random_bytes: [u8; 32] = random();
    let token = format!("fab_live_{}", URL_SAFE_NO_PAD.encode(random_bytes));
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    let expires_at = Utc::now() + Duration::days(state.session_days);
    let client_name = client_name
        .unwrap_or_else(|| "FABA+ Custom Editor".into())
        .trim()
        .chars()
        .take(100)
        .collect::<String>();
    sqlx::query(
        "INSERT INTO sessions(id, user_id, token_hash, client_name, expires_at)
         VALUES($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(token_hash)
    .bind(client_name)
    .bind(expires_at)
    .execute(&state.pool)
    .await?;
    Ok(SessionResponse {
        token,
        expires_at,
        account: user.into(),
    })
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.starts_with("fab_live_") && value.len() > 40)
        .ok_or(ApiError::Unauthorized)
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    let email = value.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        });
    valid
        .then_some(email)
        .ok_or_else(|| ApiError::Validation("Saisissez une adresse e-mail valide.".into()))
}

fn validate_display_name(value: &str) -> Result<String, ApiError> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(ApiError::Validation(
            "Le nom doit contenir entre 1 et 80 caractères.".into(),
        ));
    }
    Ok(name.to_owned())
}

fn validate_password(value: &str) -> Result<(), ApiError> {
    if value.chars().count() < 10 || value.chars().count() > 200 {
        return Err(ApiError::Validation(
            "Le mot de passe doit contenir au moins 10 caractères.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_account_fields() {
        assert_eq!(
            normalize_email(" Edwin@Example.COM ").unwrap(),
            "edwin@example.com"
        );
        assert!(normalize_email("invalid").is_err());
        assert!(validate_password("tropcourt").is_err());
        assert!(validate_password("phrase-secrete-solide").is_ok());
    }
}
