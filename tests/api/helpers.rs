use once_cell::sync::Lazy;
use secrecy::ExposeSecret;
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions};
use uuid::Uuid;
use zero_to_prod::{
    configuration::{DatabaseSettings, get_configuration},
    startup::{Application, get_connection_pool},
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

    // Modify configuration to ensure test isolation, no collisions
    let configuration = {
        let mut config = get_configuration().expect("Failed to read configuration.");
        // Use a different database for each test case
        config.database.database_name = Uuid::new_v4().to_string();
        // Request a random OS-assigned port
        config.application.port = 0;
        config
    };

    configure_database(&configuration.database).await;
    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application.");
    let address = format!("http://127.0.0.1:{}", application.port());
    let _ = tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        db_pool: get_connection_pool(&configuration.database).await,
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
