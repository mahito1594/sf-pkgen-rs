mod cli;
mod error;
mod sf_client;

mod ansi;
mod output;
mod signal;
mod tui;
mod wildcard;
mod xml;

use clap::Parser;
use cli::{Cli, GenerateArgs};
use error::AppError;
use output::{prompt_output_path, validate_output_path, write_output};
use sf_client::{RealSfClient, SfClient};
use xml::{PackageXmlInput, generate_package_xml};

fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError> {
    // 1. Check sf CLI exists
    sf_client.check_sf_exists()?;
    signal::check_interrupted()?;

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
    signal::check_interrupted()?;

    // 3. Fetch metadata types
    eprintln!("Fetching metadata types...");
    let mut metadata_types =
        sf_client.list_metadata_types(args.target_org.as_deref(), &api_version)?;

    if metadata_types.is_empty() {
        return Err(AppError::NoMetadataTypes);
    }
    signal::check_interrupted()?;

    // Sort metadata types alphabetically for consistent TUI display
    metadata_types.sort_by(|a, b| a.xml_name.cmp(&b.xml_name));

    // 4. Select metadata types and components
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
    signal::check_interrupted()?;

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

fn main() {
    signal::install_handler_once();
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Generate(args) => {
            let sf_client = RealSfClient;
            if let Err(e) = run_generate(&sf_client, &args) {
                let msg = e.to_string();
                if !msg.is_empty() {
                    eprintln!("{msg}");
                }
                std::process::exit(e.exit_code());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::cli::GenerateArgs;
    use crate::error::AppError;
    use crate::sf_client::{MetadataComponent, MetadataType, OrgInfo, SfClient};

    /// Configurable mock for SfClient. Each field controls the behavior of the
    /// corresponding trait method. Uses simple flags/values instead of stored
    /// `Result<T, AppError>` to avoid needing Clone on AppError.
    struct MockSfClient {
        /// If false, check_sf_exists returns SfCliNotFound
        check_sf_ok: bool,
        /// If Some, get_org_info returns Ok with this version; if None, returns ApiVersionError
        api_version: Option<String>,
        /// If Some, list_metadata_types returns Ok with these types; if None, returns SfCliError
        metadata_types: Option<Vec<MetadataType>>,
        /// If Some, list_metadata returns components from this map; if None, panics
        components: Option<HashMap<String, Vec<MetadataComponent>>>,
    }

    impl SfClient for MockSfClient {
        fn check_sf_exists(&self) -> Result<(), AppError> {
            if self.check_sf_ok {
                Ok(())
            } else {
                Err(AppError::SfCliNotFound)
            }
        }

        fn get_org_info(&self, _target_org: Option<&str>) -> Result<OrgInfo, AppError> {
            match &self.api_version {
                Some(v) => Ok(OrgInfo {
                    api_version: v.clone(),
                }),
                None => Err(AppError::ApiVersionError {
                    message: "No default org".to_string(),
                }),
            }
        }

        fn list_metadata_types(
            &self,
            _target_org: Option<&str>,
            _api_version: &str,
        ) -> Result<Vec<MetadataType>, AppError> {
            match &self.metadata_types {
                Some(types) => Ok(types.clone()),
                None => Err(AppError::SfCliError {
                    message: "command failed".to_string(),
                }),
            }
        }

        fn list_metadata(
            &self,
            metadata_type: &str,
            _target_org: Option<&str>,
            _api_version: &str,
        ) -> Result<Vec<MetadataComponent>, AppError> {
            match &self.components {
                Some(map) => Ok(map.get(metadata_type).cloned().unwrap_or_default()),
                None => panic!("list_metadata should not be called in pre-TUI tests"),
            }
        }
    }

    fn make_args(api_version: Option<&str>, output_file: Option<&str>) -> GenerateArgs {
        GenerateArgs {
            target_org: None,
            api_version: api_version.map(String::from),
            output_file: output_file.map(PathBuf::from),
        }
    }

    #[test]
    fn check_sf_exists_failure_returns_sf_cli_not_found() {
        let client = MockSfClient {
            check_sf_ok: false,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![]),
            components: None,
        };
        let args = make_args(Some("62.0"), Some("out.xml"));
        let err = crate::run_generate(&client, &args).unwrap_err();
        assert!(matches!(err, AppError::SfCliNotFound));
    }

    #[test]
    fn get_org_info_failure_returns_api_version_error() {
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: None, // get_org_info will fail
            metadata_types: Some(vec![]),
            components: None,
        };
        // api_version not specified, so get_org_info will be called
        let args = make_args(None, Some("out.xml"));
        let err = crate::run_generate(&client, &args).unwrap_err();
        assert!(matches!(err, AppError::ApiVersionError { .. }));
    }

    #[test]
    fn api_version_specified_skips_get_org_info() {
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: None, // get_org_info would fail if called
            metadata_types: Some(vec![]),
            components: None,
        };
        // api_version specified, so get_org_info should be skipped.
        // metadata_types is empty so run_generate returns NoMetadataTypes
        // before reaching run_tui (which would require /dev/tty).
        let args = make_args(Some("62.0"), Some("out.xml"));
        let result = crate::run_generate(&client, &args);
        match result {
            Err(AppError::ApiVersionError { .. }) => {
                panic!("get_org_info should not have been called when api_version is specified")
            }
            _ => {} // NoMetadataTypes or any non-ApiVersionError is fine
        }
    }

    #[test]
    fn list_metadata_types_failure_returns_sf_cli_error() {
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: None, // list_metadata_types will fail
            components: None,
        };
        let args = make_args(Some("62.0"), Some("out.xml"));
        let err = crate::run_generate(&client, &args).unwrap_err();
        assert!(matches!(err, AppError::SfCliError { .. }));
    }

    #[test]
    fn empty_metadata_types_returns_no_metadata_types() {
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![]),
            components: None,
        };
        let args = make_args(Some("62.0"), Some("out.xml"));
        let err = crate::run_generate(&client, &args).unwrap_err();
        assert!(matches!(err, AppError::NoMetadataTypes));
    }
}
