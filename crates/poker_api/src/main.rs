use std::{net::SocketAddr, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    match poker_api::serve(addr).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("failed to run poker api on {addr}: {error}");
            ExitCode::FAILURE
        }
    }
}
