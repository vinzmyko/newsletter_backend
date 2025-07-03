use std::fmt::{Debug, Display};

use tokio::task::JoinError;

use zero_to_prod::{
    configuration::get_configuration,
    issue_delivery_worker::run_worker_until_stopped,
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
    let application = Application::build(configuration.clone(), connection_pool).await?;
    let application_task = tokio::spawn(application.run_until_stopped());
    let worker_task = tokio::spawn(run_worker_until_stopped(configuration));

    // Coordinate shutdown
    tokio::select! {
        o = application_task => report_exit("API", o),
        o = worker_task => report_exit("Background worker", o),
    };

    Ok(())
}

// Error reporting, informs which component failed first, why it failed, and what the error was
fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{} has exited", task_name)
        }
        Ok(Err(e)) => {
            tracing::error!(
                error.name = %e,
                    "{} failed",
                    task_name
            )
        }
        Err(e) => {
            tracing::error!(
                error.name = %e,
                    "{}' task failed to complete",
                    task_name
            )
        }
    }
}
