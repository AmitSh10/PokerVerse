use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let config = match poker_api::ApiConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let addr = config.addr();

    match poker_api::serve(addr).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("failed to run poker api on {addr}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("poker_api=info,tower_http=info"));
    let _ = fmt().with_env_filter(filter).try_init();
}
