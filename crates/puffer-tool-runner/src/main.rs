use anyhow::Result;
use clap::Parser;
use puffer_tool_runner::{
    load_or_generate_token, print_handshake, ToolRunnerOptions, ToolRunnerService,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(bin_name = "puffer-tool-runner")]
struct Cli {
    /// Bind address for the gRPC server.
    #[arg(long = "bind", default_value = "127.0.0.1:0")]
    bind: String,
    /// Bearer token expected from clients. When omitted, one is generated.
    #[arg(long = "token")]
    token: Option<String>,
    /// Optional file that receives the runner handshake JSON.
    #[arg(long = "handshake-file")]
    handshake_file: Option<PathBuf>,
    /// Print the handshake JSON to stdout after binding.
    #[arg(long = "print-handshake", default_value_t = false)]
    print_handshake: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let token = load_or_generate_token(cli.token.as_deref());
    let service = ToolRunnerService::new(token.clone());
    let listener = tokio::net::TcpListener::bind(&cli.bind).await?;
    let addr = listener.local_addr()?;
    print_handshake(&ToolRunnerOptions {
        endpoint: format!("http://{addr}"),
        token,
        handshake_file: cli.handshake_file,
        print_stdout: cli.print_handshake,
    })?;
    tonic::transport::Server::builder()
        .add_service(service.into_service())
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
        .await?;
    Ok(())
}
