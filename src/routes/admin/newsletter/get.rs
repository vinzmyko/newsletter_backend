use std::fmt::Write;

use actix_web::{HttpResponse, http::header::ContentType, web};
use actix_web_flash_messages::IncomingFlashMessages;

use crate::authentication::UserId;

pub async fn send_newsletter_form(
    flash_messages: IncomingFlashMessages,
    _user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
        <html lang="en">
            <head>
                <meta http-equiv="content-type" content="text/html; charset=utf-8">
                <title>Publish Newsletter Issue</title>
            </head>
            <body>
                {msg_html}
                <form action="/admin/newsletter" method="post">
                    <label>Newsletter Title:
                        <br>
                        <input
                            type="text"
                            size="100"
                            placeholder="Enter the issue title"
                            name="title"
                        >
                    </label>
                    <br>
                    <label>Plain Text Content:
                        <br>
                        <textarea
                            name="text_content"
                            placeholder="Enter newsletter body"
                            rows="10"
                            cols="135"
                            wrap="soft"
                        ></textarea>
                    </label>
                    <label>HTML Content:
                        <br>
                        <textarea
                            name="html_content"
                            placeholder="Enter newsletter body"
                            rows="10"
                            cols="135"
                            wrap="soft"
                        ></textarea>
                    </label>
                    <button type="submit">Publish</button>
                </form>
                <br>
                <p><a href="/admin/dashboard">&lt;- Back</a></p>
            </body>
        </html>"#,
        )))
}
