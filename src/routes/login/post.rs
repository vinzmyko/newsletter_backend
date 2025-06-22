use actix_web::{HttpResponse, http::header::LOCATION, web};
use secrecy::Secret;
use sqlx::PgPool;

use crate::authentication::{Credentials, validate_credentials};

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}

#[tracing::instrument(
    skip(form, pool),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };
    tracing::Span::current().record("username", tracing::field::display(&credentials.username));
    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", tracing::field::display(&user_id));
            return HttpResponse::SeeOther()
                .insert_header((LOCATION, "/"))
                .finish();
        }
        Err(_) => {
            todo!()
        }
    }
}
