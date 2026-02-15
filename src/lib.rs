pub mod cli;
pub mod error;
pub mod sf_client;

mod ansi;
mod output;
mod tui;
mod wildcard;
mod xml;

use cli::GenerateArgs;
use error::AppError;
use output::{prompt_output_path, validate_output_path, write_output};
use sf_client::SfClient;
use xml::{PackageXmlInput, generate_package_xml};

pub fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError> {
    // 1. Check sf CLI exists
    sf_client.check_sf_exists()?;

    // 2. Determine API version
    let api_version = match &args.api_version {
        Some(v) => v.clone(),
        None => {
            eprintln!("Fetching API version...");
            sf_client
                .get_org_info(args.target_org.as_deref())?
                .api_version
        }
    };

    // 3. Fetch metadata types
    eprintln!("Fetching metadata types...");
    let mut metadata_types =
        sf_client.list_metadata_types(args.target_org.as_deref(), &api_version)?;

    if metadata_types.is_empty() {
        return Err(AppError::NoMetadataTypes);
    }

    // Sort metadata types alphabetically for consistent TUI display
    metadata_types.sort_by(|a, b| a.xml_name.cmp(&b.xml_name));

    // 4. TUI: select metadata types and components
    let selections = tui::run_tui(
        metadata_types,
        sf_client,
        args.target_org.as_deref(),
        &api_version,
    )?;

    // 5. Determine output path
    let output_path = match &args.output_file {
        Some(p) => p.clone(),
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
