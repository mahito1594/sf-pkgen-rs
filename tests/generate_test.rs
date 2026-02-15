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
        _metadata_type: &str,
        _target_org: Option<&str>,
        _api_version: &str,
    ) -> Result<Vec<MetadataComponent>, AppError> {
        panic!("list_metadata should not be called in pre-TUI tests")
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
    };
    let args = make_args(Some("62.0"), Some("out.xml"));
    let err = sf_pkgen::run_generate(&client, &args).unwrap_err();
    assert!(matches!(err, AppError::NoMetadataTypes));
}
