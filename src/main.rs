use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;
use zero_to_prod::{
    configuration::get_configuration,
    email_client::EmailClient,
    startup::run,
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Subscriber receives all span and event data and decides how to process it for output
    let subscriber = get_subscriber("zero_to_prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_pool = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    } else {
        PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_lazy_with(configuration.database.with_db())
    };

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

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to run migrations");
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| configuration.application.port.to_string())
        .parse::<u16>()
        .expect("Failed to parse PORT");
    let address = format!("{}:{}", configuration.application.host, port);
    let listener = TcpListener::bind(address).expect("Failed to bind to random port");

    run(listener, connection_pool, email_client)?.await?;

    Ok(())
}
