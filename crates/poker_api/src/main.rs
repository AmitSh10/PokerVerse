use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
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
