mod eval;
mod sync_schema;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sqlai", version, about = "智能问数 CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Hello,
    SyncSchema(sync_schema::SyncArgs),
    Eval(eval::EvalArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hello => {
            println!("sqlai-cli ready");
            Ok(())
        }
        Cmd::SyncSchema(args) => sync_schema::run(args).await,
        Cmd::Eval(args) => eval::run(args).await,
    }
}
