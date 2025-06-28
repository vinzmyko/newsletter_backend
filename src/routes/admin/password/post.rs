use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

use crate::{
    authentication::{AuthError, Credentials, validate_credentials},
    routes::admin::dashboard::get_username,
    session_state::TypedSession,
    utils::{e500, see_other},
};

#[derive(Debug)]
pub struct ValidNewPassword(String);

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: Secret<String>,
    new_password: Secret<String>,
    new_password_check: Secret<String>,
}

impl ValidNewPassword {
    pub fn parse(s: &str) -> Result<ValidNewPassword, String> {
        if s.len() < 12 || s.len() > 128 {
            return Err(format!(
                "Password must be between 12 and 128 characters, got {}",
                s.len()
            ));
        }
        Ok(ValidNewPassword(s.to_string()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub async fn change_password(
    form: web::Form<FormData>,
    session: TypedSession,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = session.get_user_id().map_err(e500)?;
    if user_id.is_none() {
        return Ok(see_other("/login"));
    };
    let user_id = user_id.unwrap();
    let new_password = match ValidNewPassword::parse(form.new_password.expose_secret()) {
        Ok(password) => password,
        Err(error) => {
            FlashMessage::error(&error).send();
            return Ok(see_other("/admin/password"));
        }
    };
    let new_password_check = match ValidNewPassword::parse(form.new_password_check.expose_secret())
    {
        Ok(password) => password,
        Err(e) => {
            FlashMessage::error(&e).send();
            return Ok(see_other("/admin/password"));
        }
    };

    if new_password.0 != new_password_check.0 {
        FlashMessage::error(
            "You entered two different new passwords - the field values must match.",
        )
        .send();
        return Ok(see_other("/admin/password"));
    }

    // Get the username from the session
    let username = get_username(user_id, &pool).await.map_err(e500)?;

    // User must prove they know the current password/reauthenticate
    let credentials = Credentials {
        username,
        password: form.0.current_password,
    };
    if let Err(e) = validate_credentials(credentials, &pool).await {
        return match e {
            AuthError::InvalidCredentials(_) => {
                FlashMessage::error("The current password is incorrect.").send();
                Ok(see_other("/admin/password"))
            }
            AuthError::UnexpectedError(_) => Err(e500(e)),
        };
    }
    todo!()
}

#[cfg(test)]
mod tests {
    use crate::routes::admin::password::post::ValidNewPassword;
    use claim::{assert_err, assert_ok};

    #[test]
    fn new_password_less_than_12_is_rejected() {
        let password_less_then_12 = "a".repeat(6);
        assert_err!(ValidNewPassword::parse(password_less_then_12.as_ref()));
    }

    #[test]
    fn new_password_greater_than_128_is_rejected() {
        let password_greater_than_128 = "a".repeat(129);
        assert_err!(ValidNewPassword::parse(password_greater_than_128.as_ref()));
    }

    #[test]
    fn new_password_between_12_and_128_is_accepted() {
        let valid_password = "a".repeat(14);
        assert_ok!(ValidNewPassword::parse(valid_password.as_ref()));
    }
}
