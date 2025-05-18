use zero_to_prod::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    run()?.await
}
