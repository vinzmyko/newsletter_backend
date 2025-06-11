use crate::{
    configuration::{DatabaseSettings, Settings},
    email_client::EmailClient,
    routes::{health_check, subscribe},
};
use actix_web::{App, HttpServer, dev::Server, web, web::Data};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(
        configuration: Settings,
        connection_pool: PgPool,
    ) -> Result<Self, std::io::Error> {
        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender email address.");
        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            configuration
                .email_client
                .base_url
                .parse()
                .expect("Invalid base URL in configuration."),
            sender_email,
            configuration.email_client.authorisation_token,
            timeout,
        );

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
        )?;

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

pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
) -> Result<Server, std::io::Error> {
    let db_pool = Data::new(db_pool);
    let email_client = Data::new(email_client);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
