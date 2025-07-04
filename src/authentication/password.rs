use anyhow::Context;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

use crate::{routes::ValidNewPassword, telemetry::spawn_blocking_with_tracing};

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials.")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

pub struct Credentials {
    pub username: String,
    pub password: Secret<String>,
}

#[tracing::instrument(name = "Validate credentials", skip(credentials, pool))]
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<uuid::Uuid, AuthError> {
    let mut authenticated_user_id = None;
    let mut phc_to_verify = Secret::new(
        "$argon2id$v=19$m=15000,t=2,p=1$\
            gZiV/M1gPc22E1AH/Jh1Hw$\
            CW0rkoo7oJBQ/iyh7uJ0L02aLefrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some((database_user_id, database_phc)) =
        get_stored_credentials(&credentials.username, pool).await?
    {
        authenticated_user_id = Some(database_user_id);
        phc_to_verify = database_phc;
    }
    spawn_blocking_with_tracing(move || verify_password_hash(phc_to_verify, credentials.password))
        .await
        .context("Failed to spawn blocking task.")??;

    authenticated_user_id
        .ok_or_else(|| anyhow::anyhow!("Unkonwn username."))
        .map_err(AuthError::InvalidCredentials)
}

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
pub async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(uuid::Uuid, Secret<String>)>, anyhow::Error> {
    let row: Option<_> = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await
    .context("Failed to perform a query to retrieve stored credentials.")?;

    let row = row.map(|row| (row.user_id, Secret::new(row.password_hash)));
    Ok(row)
}

#[tracing::instrument(name = "Verify password hash", skip(database_phc, password_candidate))]
fn verify_password_hash(
    database_phc: Secret<String>,
    password_candidate: Secret<String>,
) -> Result<(), AuthError> {
    let parsed_phc = PasswordHash::new(database_phc.expose_secret())
        .context("Failed to parse hash in PHC string format.")?;

    Argon2::default()
        // Hashes the input password with the same params as the phc in the database
        .verify_password(password_candidate.expose_secret().as_bytes(), &parsed_phc)
        .context("Invalid password.")
        .map_err(AuthError::InvalidCredentials)
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
pub async fn change_password(
    user_id: uuid::Uuid,
    password: ValidNewPassword,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(password))
        .await?
        .context("Failed to hash password")?;
    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $1
        WHERE user_id = $2
        "#,
        password_hash.expose_secret(),
        user_id
    )
    .execute(pool)
    .await
    .context("Failed to change user's password in the database.")?;
    Ok(())
}

fn compute_password_hash(password: ValidNewPassword) -> Result<Secret<String>, anyhow::Error> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15000, 2, 1, None).unwrap(),
    )
    .hash_password(password.as_bytes(), &salt)?
    .to_string();

    Ok(Secret::new(password_hash))
}
