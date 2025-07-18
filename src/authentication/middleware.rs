use std::ops::Deref;

use actix_web::{
    FromRequest, HttpMessage,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    error::InternalError,
};
use actix_web_lab::middleware::Next;
use uuid::Uuid;

use crate::{
    session_state::TypedSession,
    utils::{e500, see_other},
};

// Just like application state you can only have one variable per type
#[derive(Copy, Clone, Debug)]
pub struct UserId(Uuid);

// Allows for println!(), dbg!(), and format!()
impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// Allows you to treat UserId as a Uuid, meaning no UserId.0 but *UserId. You can keep the inner
// Uuid type private with this
impl Deref for UserId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn reject_anonymous_users(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let session = {
        let (http_request, payload) = req.parts_mut();
        TypedSession::from_request(http_request, payload).await
    }?;

    match session.get_user_id().map_err(e500)? {
        Some(user_id) => {
            req.extensions_mut().insert(UserId(user_id));
            next.call(req).await
        }
        None => {
            let response = see_other("/login");
            let e = anyhow::anyhow!("The user has not logged in");
            Err(InternalError::from_response(e, response).into())
        }
    }
}
