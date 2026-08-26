#[tokio::main]
async fn main() {
    if let Err(error) = minichain::cli::run().await {
        eprintln!("{error}");
        std::process::exit(error.exit_code);
    }
}
