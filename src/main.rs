use zero_to_prod::{
    configuration::get_configuration,
    startup::{Application, get_connection_pool},
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Subscriber receives all span and event data and decides how to process it for output
    let subscriber = get_subscriber("zero_to_prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration");
    let connection_pool = get_connection_pool(&configuration.database).await;

    let application = Application::build(configuration, connection_pool).await?;
    application.run_until_stopped().await?;

    Ok(())
}
