mod cli;
mod error;
mod sf_client;

mod ansi;
mod inherit;
mod output;
mod signal;
mod tui;
mod wildcard;
mod xml;

use std::collections::{HashMap, HashSet};

use clap::Parser;
use cli::{Cli, GenerateArgs};
use error::AppError;
use output::{prompt_output_path, validate_output_path, write_output};
use sf_client::{MetadataType, RealSfClient, SfClient};
use xml::{PackageXmlInput, generate_package_xml};

fn resolve_initial_selections(
    sf_client: &dyn SfClient,
    args: &GenerateArgs,
    api_version: &str,
    metadata_types: &[MetadataType],
    inherited: Option<&inherit::InheritedPackage>,
) -> Result<HashMap<String, HashSet<String>>, AppError> {
    let Some(pkg) = inherited else {
        return Ok(HashMap::new());
    };

    let org_type_set: HashSet<String> = metadata_types.iter().map(|t| t.xml_name.clone()).collect();

    // For types with individual members (not wildcard), fetch org components
    // so that member-level validation and warnings can run before the TUI.
    let mut org_components: HashMap<String, Vec<String>> = HashMap::new();
    let mut skipped_member_types: HashSet<String> = HashSet::new();
    let has_individual_members = pkg
        .types
        .values()
        .any(|members| !inherit::is_wildcard_members(members));
    if has_individual_members {
        eprintln!("Fetching components for inherited types...");
    }
    for (type_name, members) in &pkg.types {
        // Wildcard selections and types absent from the org need no component fetch.
        if inherit::is_wildcard_members(members) || !org_type_set.contains(type_name) {
            continue;
        }
        if let Ok(components) =
            sf_client.list_metadata(type_name, args.target_org.as_deref(), api_version)
        {
            org_components.insert(
                type_name.clone(),
                components.into_iter().map(|c| c.full_name).collect(),
            );
        } else {
            skipped_member_types.insert(type_name.clone());
        }
        signal::check_interrupted()?;
    }

    let (selections, warnings) = inherit::resolve_inherited_selections(
        pkg,
        &org_type_set,
        &org_components,
        &skipped_member_types,
    );
    for warning in &warnings {
        eprintln!("Warning: {warning}");
    }
    Ok(selections)
}

fn run_generate(sf_client: &dyn SfClient, args: &GenerateArgs) -> Result<(), AppError> {
    // 1. Check sf CLI exists
    sf_client.check_sf_exists()?;
    signal::check_interrupted()?;

    // 2. Parse --inherit early so we can fail fast on invalid files and
    //    use the inherited version when determining the API version.
    let inherited = match &args.inherit {
        Some(path) => Some(inherit::parse_package_xml(path)?),
        None => None,
    };

    // 3. Determine API version.
    //    Priority: --api-version > inherited <version> > org info
    let api_version = match &args.api_version {
        Some(v) => v.clone(),
        None => {
            let inherited_version = inherited.as_ref().and_then(|p| p.version.as_ref());
            match inherited_version {
                Some(v) => v.clone(),
                None => {
                    eprintln!("Fetching API version...");
                    sf_client
                        .get_org_info(args.target_org.as_deref())?
                        .api_version
                }
            }
        }
    };
    signal::check_interrupted()?;

    // 4. Fetch metadata types
    eprintln!("Fetching metadata types...");
    let mut metadata_types =
        sf_client.list_metadata_types(args.target_org.as_deref(), &api_version)?;

    if metadata_types.is_empty() {
        return Err(AppError::NoMetadataTypes);
    }
    signal::check_interrupted()?;

    // Sort metadata types alphabetically for consistent TUI display
    metadata_types.sort_by(|a, b| a.xml_name.cmp(&b.xml_name));

    // 5. Resolve initial selections from --inherit against org types and components.
    let initial_selections = resolve_initial_selections(
        sf_client,
        args,
        &api_version,
        &metadata_types,
        inherited.as_ref(),
    )?;

    // 6. Select metadata types and components
    let selections = tui::run_tui(
        metadata_types,
        sf_client,
        args.target_org.as_deref(),
        &api_version,
        initial_selections,
    )?;

    // 7. Determine output path
    let output_path = match &args.output_file {
        Some(p) => p.clone(),
        None => prompt_output_path()?,
    };
    signal::check_interrupted()?;

    // 8. Validate output path
    validate_output_path(&output_path)?;

    // 9. Generate package.xml
    let input = PackageXmlInput {
        types: selections,
        api_version,
    };
    let xml_content = generate_package_xml(&input);

    // 10. Write output
    write_output(&output_path, &xml_content)?;

    // 11. Done
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
        /// If Some, list_metadata returns components from this map; if None, returns SfCliError
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
                None => Err(AppError::SfCliError {
                    message: "command failed".to_string(),
                }),
            }
        }
    }

    fn make_args(api_version: Option<&str>, output_file: Option<&str>) -> GenerateArgs {
        GenerateArgs {
            target_org: None,
            api_version: api_version.map(String::from),
            output_file: output_file.map(PathBuf::from),
            inherit: None,
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
        let err = crate::run_generate(&client, &args).unwrap_err();
        assert!(
            matches!(err, AppError::NoMetadataTypes),
            "expected NoMetadataTypes but got: {err}"
        );
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

    // -- API version priority: --api-version > inherited > org fallback --

    /// Creates GenerateArgs with --inherit pointing to a temp file that contains the given XML.
    fn make_args_with_inherit(
        xml_content: &str,
        api_version: Option<&str>,
    ) -> (GenerateArgs, tempfile::NamedTempFile) {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(xml_content.as_bytes()).expect("write temp");
        let args = GenerateArgs {
            target_org: None,
            api_version: api_version.map(String::from),
            output_file: Some(PathBuf::from("out.xml")),
            inherit: Some(file.path().to_path_buf()),
        };
        (args, file)
    }

    fn parse_inherited_from_temp(
        file: &tempfile::NamedTempFile,
    ) -> crate::inherit::InheritedPackage {
        crate::inherit::parse_package_xml(file.path()).expect("parse inherited package")
    }

    #[test]
    fn inherit_without_api_version_uses_inherited_version() {
        // Given: --inherit points to a package.xml with version "60.0"; no --api-version
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <types><members>*</members><name>ApexClass</name></types>\n\
                   <version>60.0</version>\n\
                   </Package>\n";
        let (args, _file) = make_args_with_inherit(xml, None);

        // Client: get_org_info would fail (so if 60.0 is used, get_org_info is NOT called)
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: None, // get_org_info fails → confirms inherited version was used
            metadata_types: Some(vec![]),
            components: None,
        };

        // When: run_generate is called
        let err = crate::run_generate(&client, &args).unwrap_err();

        // Then: the error is NoMetadataTypes (empty list), NOT ApiVersionError.
        // If get_org_info had been called, we'd get ApiVersionError instead.
        assert!(
            matches!(err, AppError::NoMetadataTypes),
            "Expected NoMetadataTypes (inherited version used); got: {err}"
        );
    }

    #[test]
    fn explicit_api_version_takes_priority_over_inherited_version() {
        // Given: --api-version "62.0" is specified AND --inherit has version "60.0"
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <version>60.0</version>\n\
                   </Package>\n";
        let (args, _file) = make_args_with_inherit(xml, Some("62.0"));

        // Client: get_org_info would fail (confirms get_org_info is NOT called)
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: None,
            metadata_types: Some(vec![]),
            components: None,
        };

        // When: run_generate is called
        let err = crate::run_generate(&client, &args).unwrap_err();

        // Then: NoMetadataTypes (62.0 was used, not 60.0; get_org_info was not called)
        assert!(
            matches!(err, AppError::NoMetadataTypes),
            "Expected NoMetadataTypes (explicit api_version used); got: {err}"
        );
    }

    #[test]
    fn inherit_with_no_version_falls_back_to_org_info() {
        // Given: --inherit points to a package.xml with NO <version>; no --api-version
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   </Package>\n";
        let (args, _file) = make_args_with_inherit(xml, None);

        // Client: get_org_info returns "62.0"
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()), // fallback version from org
            metadata_types: Some(vec![]),
            components: None,
        };

        // When: run_generate is called
        let err = crate::run_generate(&client, &args).unwrap_err();

        // Then: NoMetadataTypes (62.0 from org info was used as fallback)
        assert!(
            matches!(err, AppError::NoMetadataTypes),
            "Expected NoMetadataTypes (org fallback version used); got: {err}"
        );
    }

    #[test]
    fn inherit_not_specified_behavior_unchanged() {
        // Given: no --inherit option; existing behavior should be preserved
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: None, // get_org_info fails
            metadata_types: Some(vec![]),
            components: None,
        };
        // No --inherit, no --api-version: should call get_org_info and get ApiVersionError
        let args = make_args(None, Some("out.xml"));

        // When: run_generate is called
        let err = crate::run_generate(&client, &args).unwrap_err();

        // Then: ApiVersionError (exactly as before --inherit was added)
        assert!(
            matches!(err, AppError::ApiVersionError { .. }),
            "Expected ApiVersionError for missing org info without --inherit; got: {err}"
        );
    }

    #[test]
    fn inherit_parse_failure_returns_error_before_api_version_check() {
        // Given: --inherit points to a non-existent file
        let args = GenerateArgs {
            target_org: None,
            api_version: None,
            output_file: Some(PathBuf::from("out.xml")),
            inherit: Some(PathBuf::from("/nonexistent/missing.xml")),
        };
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![]),
            components: None,
        };

        // When: run_generate is called
        let err = crate::run_generate(&client, &args).unwrap_err();

        // Then: an error is returned (IoError for missing file), not ApiVersionError
        assert!(
            matches!(err, AppError::IoError(_)),
            "Expected IoError for missing --inherit file; got: {err}"
        );
    }

    #[test]
    fn inherit_individual_members_triggers_list_metadata_and_validates() {
        // Given: --inherit points to a package.xml with ApexClass and individual members.
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <types><members>AccountController</members><name>ApexClass</name></types>\n\
                   <version>62.0</version>\n\
                   </Package>\n";
        let (args, file) = make_args_with_inherit(xml, Some("62.0"));

        let mut components: HashMap<String, Vec<MetadataComponent>> = HashMap::new();
        components.insert(
            "ApexClass".to_string(),
            vec![MetadataComponent {
                full_name: "AccountController".to_string(),
            }],
        );
        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![MetadataType {
                xml_name: "ApexClass".to_string(),
            }]),
            components: Some(components),
        };
        let metadata_types = vec![MetadataType {
            xml_name: "ApexClass".to_string(),
        }];
        let inherited = parse_inherited_from_temp(&file);

        let selections = crate::resolve_initial_selections(
            &client,
            &args,
            "62.0",
            &metadata_types,
            Some(&inherited),
        )
        .unwrap();

        assert!(
            selections
                .get("ApexClass")
                .is_some_and(|members| members.contains("AccountController")),
            "Expected inherited member to be preselected after validation"
        );
    }

    #[test]
    fn inherit_folder_based_type_wildcard_is_skipped_with_warning() {
        // Given: --inherit points to a package.xml with Report (a folder-based type) using "*".
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <types><members>*</members><name>Report</name></types>\n\
                   <version>62.0</version>\n\
                   </Package>\n";
        let (args, file) = make_args_with_inherit(xml, Some("62.0"));

        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![MetadataType {
                xml_name: "Report".to_string(),
            }]),
            // Report has wildcard — no list_metadata call expected (skipped in loop).
            // Using Some(empty map) to avoid panic if called unexpectedly.
            components: Some(HashMap::new()),
        };
        let metadata_types = vec![MetadataType {
            xml_name: "Report".to_string(),
        }];
        let inherited = parse_inherited_from_temp(&file);

        let selections = crate::resolve_initial_selections(
            &client,
            &args,
            "62.0",
            &metadata_types,
            Some(&inherited),
        )
        .unwrap();

        assert!(
            !selections.contains_key("Report"),
            "Expected folder-based wildcard selection to be skipped"
        );
    }

    #[test]
    fn inherit_individual_members_are_skipped_when_component_fetch_fails() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <types><members>AccountController</members><name>ApexClass</name></types>\n\
                   <version>62.0</version>\n\
                   </Package>\n";
        let (args, file) = make_args_with_inherit(xml, Some("62.0"));

        let client = MockSfClient {
            check_sf_ok: true,
            api_version: Some("62.0".to_string()),
            metadata_types: Some(vec![MetadataType {
                xml_name: "ApexClass".to_string(),
            }]),
            components: None,
        };
        let metadata_types = vec![MetadataType {
            xml_name: "ApexClass".to_string(),
        }];
        let inherited = parse_inherited_from_temp(&file);

        let selections = crate::resolve_initial_selections(
            &client,
            &args,
            "62.0",
            &metadata_types,
            Some(&inherited),
        )
        .unwrap();

        assert!(
            !selections.contains_key("ApexClass"),
            "Expected unresolved inherited members to be skipped"
        );
    }
}
