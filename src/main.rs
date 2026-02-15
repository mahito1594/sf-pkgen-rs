mod ansi;
mod cli;
mod error;
mod output;
mod sf_client;
mod tui;
mod wildcard;
mod xml;

use std::process;

use clap::Parser;

use cli::{Cli, Commands};
use error::AppError;
use output::{prompt_output_path, validate_output_path, write_output};
use sf_client::{RealSfClient, SfClient};
use xml::{PackageXmlInput, generate_package_xml};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate(args) => {
            if let Err(e) = run_generate(
                args.target_org.as_deref(),
                args.api_version.as_deref(),
                args.output_file.as_deref(),
            ) {
                let msg = e.to_string();
                if !msg.is_empty() {
                    eprintln!("{msg}");
                }
                process::exit(e.exit_code());
            }
        }
    }
}

fn run_generate(
    target_org: Option<&str>,
    api_version: Option<&str>,
    output_file: Option<&std::path::Path>,
) -> Result<(), AppError> {
    let sf_client = RealSfClient;

    // 1. Check sf CLI exists
    sf_client.check_sf_exists()?;

    // 2. Determine API version
    let api_version = match api_version {
        Some(v) => v.to_string(),
        None => {
            eprintln!("Fetching API version...");
            sf_client.get_org_info(target_org)?.api_version
        }
    };

    // 3. Fetch metadata types
    eprintln!("Fetching metadata types...");
    let mut metadata_types = sf_client.list_metadata_types(target_org, &api_version)?;

    if metadata_types.is_empty() {
        return Err(AppError::NoMetadataTypes);
    }

    // Sort metadata types alphabetically for consistent TUI display
    metadata_types.sort_by(|a, b| a.xml_name.cmp(&b.xml_name));

    // 4. TUI: select metadata types and components
    let selections = tui::run_tui(metadata_types, &sf_client, target_org, &api_version)?;

    // 5. Determine output path
    let output_path = match output_file {
        Some(p) => p.to_path_buf(),
        None => prompt_output_path()?,
    };

    // 6. Validate output path
    validate_output_path(&output_path)?;

    // 7. Generate package.xml
    let input = PackageXmlInput {
        types: selections,
        api_version,
    };
    let xml_content = generate_package_xml(&input);

    // 8. Write output
    write_output(&output_path, &xml_content)?;

    // 9. Done
    eprintln!("Written to {}.", output_path.display());

    Ok(())
}
