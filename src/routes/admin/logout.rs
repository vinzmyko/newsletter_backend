use actix_web::HttpResponse;
use actix_web_flash_messages::FlashMessage;

use crate::{
    session_state::TypedSession,
    utils::{e500, see_other},
};

pub async fn log_out(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    // Making sure the user session is removed from the redis memory
    if session.get_user_id().map_err(e500)?.is_none() {
        Ok(see_other("/login"))
    } else {
        session.log_out();
        FlashMessage::info("You have successfully logged out.").send();
        Ok(see_other("/login"))
    }
}
