use clap::Parser;
use sf_pkgen::cli::{Cli, Commands};
use sf_pkgen::sf_client::RealSfClient;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate(args) => {
            let sf_client = RealSfClient;
            if let Err(e) = sf_pkgen::run_generate(&sf_client, &args) {
                let msg = e.to_string();
                if !msg.is_empty() {
                    eprintln!("{msg}");
                }
                std::process::exit(e.exit_code());
            }
        }
    }
}
