use once_cell::sync::Lazy;
use secrecy::ExposeSecret;
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions};
use std::net::TcpListener;
use uuid::Uuid;
use zero_to_prod::{
    configuration::{DatabaseSettings, get_configuration},
    email_client::EmailClient,
    startup::run,
    telemetry::{get_subscriber, init_subscriber},
};

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if std::env::var("TEST_LOG").is_ok() {
        // Logs go to terminal
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        // Logs go to the void (sink)/ deleted;
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to random port");
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);

    let mut configuration = get_configuration().expect("Failed to read configuration.");
    configuration.database.database_name = Uuid::new_v4().to_string();
    let connection_pool = configure_database(&configuration.database).await;

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

    let server =
        run(listener, connection_pool.clone(), email_client).expect("Failed to bind address");
    let _ = tokio::spawn(server);

    TestApp {
        address,
        db_pool: connection_pool,
    }
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let options_for_creating_db = PgConnectOptions::new()
        .host(&config.host)
        .username(&config.username)
        .password(config.password.expose_secret())
        .port(config.port)
        .database("postgres");
    // Connect to PostgreSQL
    let mut connection = PgConnection::connect_with(&options_for_creating_db)
        .await
        .expect("Failed to connect to Postgres.");
    // Create a new database
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await // Wait for Postgres to create database
        .expect("Failed to create database.");

    // Create a connection pool for the new database
    let connection_pool_options = config.with_db();
    let connection_pool = PgPool::connect_with(connection_pool_options)
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations") // Apply database schema to new database
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database.");

    connection_pool
}
