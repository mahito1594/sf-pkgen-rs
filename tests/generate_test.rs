use std::collections::HashMap;
use std::path::PathBuf;

use sf_pkgen::cli::GenerateArgs;
use sf_pkgen::error::AppError;
use sf_pkgen::sf_client::{MetadataComponent, MetadataType, OrgInfo, SfClient};

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
        non_interactive: false,
        all: false,
        types: None,
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
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
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
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::ApiVersionError { .. }));
}

#[test]
fn api_version_specified_skips_get_org_info() {
    let client = MockSfClient {
        check_sf_ok: true,
        api_version: None, // get_org_info would fail if called
        metadata_types: Some(vec![MetadataType {
            xml_name: "ApexClass".to_string(),
        }]),
        components: None,
    };
    // api_version specified, so get_org_info should be skipped.
    // The function will proceed past org_info to TUI, which will fail
    // because there's no /dev/tty in test environment. That's fine —
    // we just verify it doesn't return ApiVersionError.
    let args = make_args(Some("62.0"), Some("out.xml"));
    let result = sf_pkgen::run_generate(&client, &args);
    match result {
        Err(AppError::ApiVersionError { .. }) => {
            panic!("get_org_info should not have been called when api_version is specified")
        }
        _ => {} // Any other result (including IoError from TUI) is fine
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
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
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
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::NoMetadataTypes));
}

// ---------------------------------------------------------------------------
// Non-interactive mode integration tests
// ---------------------------------------------------------------------------

fn sample_metadata_types() -> Vec<MetadataType> {
    vec![
        MetadataType {
            xml_name: "ApexClass".to_string(),
        },
        MetadataType {
            xml_name: "Report".to_string(),
        },
        MetadataType {
            xml_name: "CustomObject".to_string(),
        },
    ]
}

fn make_non_interactive_client() -> MockSfClient {
    let mut components = HashMap::new();
    components.insert(
        "Report".to_string(),
        vec![MetadataComponent {
            full_name: "SalesReport".to_string(),
        }],
    );
    MockSfClient {
        check_sf_ok: true,
        api_version: Some("62.0".to_string()),
        metadata_types: Some(sample_metadata_types()),
        components: Some(components),
    }
}

#[test]
fn non_interactive_all_generates_xml() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("package.xml");
    let client = make_non_interactive_client();
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: Some(output_path.clone()),
        non_interactive: true,
        all: true,
        types: None,
    };
    sf_pkgen::run_generate(&client, &args).unwrap();
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("<Package xmlns="));
    assert!(content.contains("<name>ApexClass</name>"));
    assert!(content.contains("<members>*</members>"));
    // Report is folder-based, should have SalesReport
    assert!(content.contains("<name>Report</name>"));
    assert!(content.contains("<members>SalesReport</members>"));
}

#[test]
fn non_interactive_types_generates_xml() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("package.xml");
    let client = make_non_interactive_client();
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: Some(output_path.clone()),
        non_interactive: true,
        all: false,
        types: Some(vec!["ApexClass".to_string(), "Report".to_string()]),
    };
    sf_pkgen::run_generate(&client, &args).unwrap();
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("<name>ApexClass</name>"));
    assert!(content.contains("<name>Report</name>"));
    // CustomObject should not be included
    assert!(!content.contains("<name>CustomObject</name>"));
}

#[test]
fn non_interactive_unknown_type_fails() {
    let client = make_non_interactive_client();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("package.xml");
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: Some(output_path),
        non_interactive: true,
        all: false,
        types: Some(vec!["NonExistentType".to_string()]),
    };
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    match err {
        AppError::ValidationError { message } => {
            assert!(message.contains("Unknown metadata type"));
        }
        other => panic!("Expected ValidationError, got: {other:?}"),
    }
}

#[test]
fn all_without_non_interactive_fails() {
    let client = make_non_interactive_client();
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: Some(PathBuf::from("out.xml")),
        non_interactive: false,
        all: true,
        types: None,
    };
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::ValidationError { .. }));
}

#[test]
fn non_interactive_without_all_or_types_fails() {
    let client = make_non_interactive_client();
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: Some(PathBuf::from("out.xml")),
        non_interactive: true,
        all: false,
        types: None,
    };
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::ValidationError { .. }));
}

#[test]
fn non_interactive_without_output_file_fails() {
    let client = make_non_interactive_client();
    let args = GenerateArgs {
        target_org: None,
        api_version: Some("62.0".to_string()),
        output_file: None,
        non_interactive: true,
        all: true,
        types: None,
    };
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::ValidationError { .. }));
}
