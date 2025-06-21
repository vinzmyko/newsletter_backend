use actix_web::{
    HttpRequest, HttpResponse, ResponseError,
    http::{
        StatusCode, header,
        header::{HeaderMap, HeaderValue},
    },
    web,
};
use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

use crate::{
    domain::SubscriberEmail, email_client::EmailClient, telemetry::spawn_blocking_with_tracing,
};

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[derive(thiserror::Error, Debug)]
pub enum PublishError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("Authentication failed.")]
    AuthError(#[source] anyhow::Error),
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

struct Credentials {
    username: String,
    password: Secret<String>,
}

impl ResponseError for PublishError {
    fn error_response(&self) -> HttpResponse {
        match self {
            PublishError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
            PublishError::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}

#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(body, pool, email_client, request),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
) -> Result<HttpResponse, PublishError> {
    let credentials = basic_authentication(request.headers()).map_err(PublishError::AuthError)?;
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));
    let user_id = validate_credentials(credentials, &pool).await?;
    tracing::Span::current().record("user_id", tracing::field::display(&user_id));
    let subscribers = get_confirmed_subscribers(&pool).await?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await
                    // .with_context() is lazy, only takes a closure and only called in case of error
                    // .context() would allocate the string everytime we send an email out, in this case
                    // only when delivery fails
                    .with_context(|| {
                        format!("Failed to send newsletter issue to {}", subscriber.email)
                    })?;
            }
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscirber. \
                        Their stored contact details are invalid",
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    // Maps the retrieved rows type of the first argument
    let rows = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let confirmed_subscribers = rows
        .into_iter()
        .map(|r| match SubscriberEmail::parse(r.email) {
            Ok(email) => Ok(ConfirmedSubscriber { email }),
            Err(error) => Err(anyhow::anyhow!(error)),
        })
        .collect();

    Ok(confirmed_subscribers)
}

#[tracing::instrument(name = "Validate credentials", skip(credentials, pool))]
async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<uuid::Uuid, PublishError> {
    let mut authenticated_user_id = None;
    let mut phc_to_verify = Secret::new(
        "argon2id%v=19$m=15000,t=2,p=1$\
            gZiV/M1gPc22E1AH/Jh1Hw$\
            CW0rkoo7oJBQ/iyh7uJ0L02aLefrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some((database_user_id, database_phc)) =
        get_stored_credentials(&credentials.username, pool)
            .await
            .map_err(PublishError::UnexpectedError)?
    {
        authenticated_user_id = Some(database_user_id);
        phc_to_verify = database_phc;
    }
    spawn_blocking_with_tracing(move || verify_password_hash(phc_to_verify, credentials.password))
        .await
        .context("Failed to spawn blocking task.")
        .map_err(PublishError::UnexpectedError)??;

    authenticated_user_id
        .ok_or_else(|| PublishError::AuthError(anyhow::anyhow!("Unkonwn username.")))
}

#[tracing::instrument(name = "Verify password hash", skip(database_phc, password_candidate))]
fn verify_password_hash(
    database_phc: Secret<String>,
    password_candidate: Secret<String>,
) -> Result<(), PublishError> {
    let parsed_phc = PasswordHash::new(database_phc.expose_secret())
        .context("Failed to parse hash in PHC string format.")
        .map_err(PublishError::UnexpectedError)?;

    Argon2::default()
        // Hashes the input password with the same params as the phc in the database
        .verify_password(password_candidate.expose_secret().as_bytes(), &parsed_phc)
        .context("Invalid password.")
        .map_err(PublishError::AuthError)
}

#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
async fn get_stored_credentials(
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

fn basic_authentication(headers: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    let header_value = headers
        .get("Authorization") // HTTP headers are stored as u8
        .context("The 'Authorization' header was missing.")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF8 string.")?;
    // Hash segment
    let base64encoded_segment = header_value
        .strip_prefix("Basic ")
        .context("The authorization scheme was not 'Basic'.")?;
    let decoded_bytes = base64::decode_config(base64encoded_segment, base64::STANDARD)
        .context("Failed to base64-decode 'Basic' credentials.")?;
    let decoded_crentials = String::from_utf8(decoded_bytes)
        .context("The decoded credential string is not valid UTF8.")?;

    let mut credentials = decoded_crentials.splitn(2, ':');
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in 'Basic' auth."))?
        .to_string();
    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in 'Basic' auth."))?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password),
    })
}
