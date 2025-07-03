use std::net::TcpListener;

use actix_session::{SessionMiddleware, storage::RedisSessionStore};
use actix_web::{App, HttpServer, cookie::Key, dev::Server, web, web::Data};
use actix_web_flash_messages::{FlashMessagesFramework, storage::CookieMessageStore};
use actix_web_lab::middleware::from_fn;
use secrecy::{ExposeSecret, Secret};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing_actix_web::TracingLogger;

use crate::{
    authentication::reject_anonymous_users,
    configuration::{DatabaseSettings, Settings},
    email_client::EmailClient,
    routes::{
        admin_dashboard, change_password, change_password_form, confirm, health_check, home,
        log_out, login, login_form, publish_newsletter, send_newsletter_form, subscribe,
    },
};

pub struct Application {
    port: u16,
    server: Server,
}

#[derive(Clone, Debug)]
pub struct HmacSecret(pub Secret<String>);

impl Application {
    pub async fn build(
        configuration: Settings,
        connection_pool: PgPool,
    ) -> Result<Self, anyhow::Error> {
        let email_client = configuration.email_client.client();

        let requested_port = if configuration.application.port == 0 {
            0
        } else {
            std::env::var("PORT")
                .unwrap_or_else(|_| configuration.application.port.to_string())
                .parse::<u16>()
                .expect("Failed to parse PORT")
        };

        let address = format!("{}:{}", configuration.application.host, requested_port);
        let listener = TcpListener::bind(&address)?;
        let designated_port = listener.local_addr().unwrap().port();
        let server = run(
            listener,
            connection_pool,
            email_client,
            configuration.application.base_url,
            configuration.application.hmac_secret,
            configuration.redis_uri,
        )
        .await?;

        Ok(Self {
            port: designated_port,
            server,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// Application state can only access a single unique specific type, thus make a new one
pub struct ApplicationBaseUrl(pub String);

pub async fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    } else {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_lazy_with(configuration.with_db())
    }
}

pub async fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    hmac_secret: Secret<String>,
    redis_uri: Secret<String>,
) -> Result<Server, anyhow::Error> {
    let db_pool = Data::new(db_pool);
    let email_client = Data::new(email_client);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let secret_key = Key::from(hmac_secret.expose_secret().as_bytes());
    // Storage backend - where flash messages are stored in cookies, how they are secured, and what
    // format they use.
    let message_store = CookieMessageStore::builder(secret_key.clone()).build();
    // Orchestrates the storage backend defined, provided api for sending and receiving messages,
    // and handles the lifecycle of the flash messages.
    let message_framework = FlashMessagesFramework::builder(message_store).build();
    let redis_store = RedisSessionStore::new(redis_uri.expose_secret()).await?;
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(message_framework.clone())
            .wrap(SessionMiddleware::new(
                redis_store.clone(),
                secret_key.clone(),
            ))
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .route("/", web::get().to(home))
            .route("/login", web::get().to(login_form))
            .route("/login", web::post().to(login))
            .service(
                // web::scope() needs a .service() for mounting
                web::scope("/admin") // Can only wrap a scope not a service
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/dashboard", web::get().to(admin_dashboard))
                    .route("/password", web::get().to(change_password_form))
                    .route("/password", web::post().to(change_password))
                    .route("/newsletter", web::get().to(send_newsletter_form))
                    .route("/newsletter", web::post().to(publish_newsletter))
                    .route("/logout", web::post().to(log_out)),
            )
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
            .app_data(Data::new(hmac_secret.clone()))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
