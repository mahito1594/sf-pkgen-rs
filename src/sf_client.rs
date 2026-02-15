use std::process::Command;

use serde::Deserialize;

use crate::ansi::strip_ansi_escapes;
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Domain types (public)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OrgInfo {
    pub api_version: String,
}

#[derive(Debug, Clone)]
pub struct MetadataType {
    pub xml_name: String,
}

#[derive(Debug, Clone)]
pub struct MetadataComponent {
    pub full_name: String,
}

// ---------------------------------------------------------------------------
// Internal serde structs (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SfResponse {
    status: i32,
    result: Option<serde_json::Value>,
    message: Option<String>,
    name: Option<String>,
    stack: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrgDisplayResult {
    #[serde(rename = "apiVersion")]
    api_version: String,
}

#[derive(Debug, Deserialize)]
struct ListMetadataTypesResult {
    #[serde(rename = "metadataObjects")]
    metadata_objects: Vec<MetadataTypeRaw>,
}

#[derive(Debug, Deserialize)]
struct MetadataTypeRaw {
    #[serde(rename = "xmlName")]
    xml_name: String,
}

#[derive(Debug, Deserialize)]
struct MetadataComponentRaw {
    #[serde(rename = "fullName")]
    full_name: String,
}

// ---------------------------------------------------------------------------
// Helper functions (private)
// ---------------------------------------------------------------------------

fn build_error_message(response: &SfResponse) -> String {
    if let Some(msg) = &response.message
        && !msg.trim().is_empty()
    {
        return msg.clone();
    }
    // Fallback: name + stack
    let name = response.name.as_deref().unwrap_or("UnknownError");
    let stack = response.stack.as_deref().unwrap_or("");
    if stack.is_empty() {
        name.to_string()
    } else {
        format!("{name}\n{stack}")
    }
}

fn parse_sf_response(stdout: &str, stderr: &str) -> Result<serde_json::Value, AppError> {
    let cleaned = strip_ansi_escapes(stdout);
    let response: SfResponse =
        serde_json::from_str(&cleaned).map_err(|_| AppError::JsonParseError {
            stderr: stderr.to_string(),
        })?;

    if response.status != 0 {
        return Err(AppError::SfCliError {
            message: build_error_message(&response),
        });
    }

    response.result.ok_or_else(|| AppError::JsonParseError {
        stderr: stderr.to_string(),
    })
}

fn run_sf_command(args: &[&str]) -> Result<serde_json::Value, AppError> {
    let output = Command::new("sf").args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::SfCliNotFound
        } else {
            AppError::IoError(e)
        }
    })?;

    // SIGINT check after child process completes
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(2) {
            return Err(AppError::Cancelled);
        }
    }
    crate::signal::check_interrupted()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    parse_sf_response(&stdout, &stderr)
}

// ---------------------------------------------------------------------------
// SfClient trait (public)
// ---------------------------------------------------------------------------

pub trait SfClient {
    fn check_sf_exists(&self) -> Result<(), AppError>;
    fn get_org_info(&self, target_org: Option<&str>) -> Result<OrgInfo, AppError>;
    fn list_metadata_types(
        &self,
        target_org: Option<&str>,
        api_version: &str,
    ) -> Result<Vec<MetadataType>, AppError>;
    fn list_metadata(
        &self,
        metadata_type: &str,
        target_org: Option<&str>,
        api_version: &str,
    ) -> Result<Vec<MetadataComponent>, AppError>;
}

// ---------------------------------------------------------------------------
// RealSfClient implementation
// ---------------------------------------------------------------------------

pub struct RealSfClient;

impl SfClient for RealSfClient {
    fn check_sf_exists(&self) -> Result<(), AppError> {
        Command::new("sf").arg("--version").output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::SfCliNotFound
            } else {
                AppError::IoError(e)
            }
        })?;
        Ok(())
    }

    fn get_org_info(&self, target_org: Option<&str>) -> Result<OrgInfo, AppError> {
        let mut args = vec!["org", "display", "--json"];
        if let Some(org) = target_org {
            args.push("-o");
            args.push(org);
        }

        let result = run_sf_command(&args).map_err(|e| match e {
            AppError::SfCliError { message } => AppError::ApiVersionError { message },
            other => other,
        })?;

        let parsed: OrgDisplayResult =
            serde_json::from_value(result).map_err(|_| AppError::ApiVersionError {
                message: "Failed to parse org display result".to_string(),
            })?;

        Ok(OrgInfo {
            api_version: parsed.api_version,
        })
    }

    fn list_metadata_types(
        &self,
        target_org: Option<&str>,
        api_version: &str,
    ) -> Result<Vec<MetadataType>, AppError> {
        let mut args = vec![
            "org",
            "list",
            "metadata-types",
            "--api-version",
            api_version,
            "--json",
        ];
        if let Some(org) = target_org {
            args.push("-o");
            args.push(org);
        }

        let result = run_sf_command(&args)?;

        let parsed: ListMetadataTypesResult =
            serde_json::from_value(result).map_err(|_| AppError::SfCliError {
                message: "Failed to parse metadata types result".to_string(),
            })?;

        Ok(parsed
            .metadata_objects
            .into_iter()
            .map(|raw| MetadataType {
                xml_name: raw.xml_name,
            })
            .collect())
    }

    fn list_metadata(
        &self,
        metadata_type: &str,
        target_org: Option<&str>,
        api_version: &str,
    ) -> Result<Vec<MetadataComponent>, AppError> {
        let mut args = vec![
            "org",
            "list",
            "metadata",
            "-m",
            metadata_type,
            "--api-version",
            api_version,
            "--json",
        ];
        if let Some(org) = target_org {
            args.push("-o");
            args.push(org);
        }

        let result = run_sf_command(&args)?;

        let components: Vec<MetadataComponentRaw> =
            serde_json::from_value(result).map_err(|_| AppError::SfCliError {
                message: format!("Failed to parse metadata components for {metadata_type}"),
            })?;

        Ok(components
            .into_iter()
            .map(|raw| MetadataComponent {
                full_name: raw.full_name,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_sf_response: success cases --

    #[test]
    fn parse_org_display_success() {
        let stdout = r#"{"status":0,"result":{"apiVersion":"62.0","id":"00D000000000000"}}"#;
        let result = parse_sf_response(stdout, "").unwrap();
        let parsed: OrgDisplayResult = serde_json::from_value(result).unwrap();
        assert_eq!(parsed.api_version, "62.0");
    }

    #[test]
    fn parse_metadata_types_success() {
        let stdout = r#"{
            "status": 0,
            "result": {
                "metadataObjects": [
                    {"xmlName": "ApexClass", "inFolder": false, "directoryName": "classes"},
                    {"xmlName": "Report", "inFolder": true, "directoryName": "reports"}
                ]
            }
        }"#;
        let result = parse_sf_response(stdout, "").unwrap();
        let parsed: ListMetadataTypesResult = serde_json::from_value(result).unwrap();
        assert_eq!(parsed.metadata_objects.len(), 2);
        assert_eq!(parsed.metadata_objects[0].xml_name, "ApexClass");
        assert_eq!(parsed.metadata_objects[1].xml_name, "Report");
    }

    #[test]
    fn parse_metadata_components_success() {
        let stdout = r#"{
            "status": 0,
            "result": [
                {"fullName": "AccountController"},
                {"fullName": "ContactService"}
            ]
        }"#;
        let result = parse_sf_response(stdout, "").unwrap();
        let components: Vec<MetadataComponentRaw> = serde_json::from_value(result).unwrap();
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].full_name, "AccountController");
        assert_eq!(components[1].full_name, "ContactService");
    }

    #[test]
    fn parse_empty_components_array() {
        let stdout = r#"{"status": 0, "result": []}"#;
        let result = parse_sf_response(stdout, "").unwrap();
        let components: Vec<MetadataComponentRaw> = serde_json::from_value(result).unwrap();
        assert!(components.is_empty());
    }

    // -- parse_sf_response: error cases --

    #[test]
    fn parse_error_response_with_message() {
        let stdout = r#"{
            "status": 1,
            "name": "AuthError",
            "message": "No authorization found for org.",
            "stack": "AuthError: at something..."
        }"#;
        let err = parse_sf_response(stdout, "").unwrap_err();
        match err {
            AppError::SfCliError { message } => {
                assert_eq!(message, "No authorization found for org.");
            }
            other => panic!("Expected SfCliError, got: {other:?}"),
        }
    }

    #[test]
    fn parse_error_response_with_empty_message() {
        let stdout = r#"{
            "status": 1,
            "name": "SomeError",
            "message": "",
            "stack": "SomeError: at line 42\n  at module.js:10"
        }"#;
        let err = parse_sf_response(stdout, "").unwrap_err();
        match err {
            AppError::SfCliError { message } => {
                assert!(message.starts_with("SomeError\n"));
                assert!(message.contains("at line 42"));
            }
            other => panic!("Expected SfCliError, got: {other:?}"),
        }
    }

    #[test]
    fn parse_error_response_without_message() {
        let stdout = r#"{
            "status": 1,
            "name": "UnexpectedError",
            "stack": "Error stack trace"
        }"#;
        let err = parse_sf_response(stdout, "").unwrap_err();
        match err {
            AppError::SfCliError { message } => {
                assert_eq!(message, "UnexpectedError\nError stack trace");
            }
            other => panic!("Expected SfCliError, got: {other:?}"),
        }
    }

    // -- parse_sf_response: JSON parse failures --

    #[test]
    fn parse_invalid_json() {
        let err = parse_sf_response("not json at all", "some stderr").unwrap_err();
        match err {
            AppError::JsonParseError { stderr } => {
                assert_eq!(stderr, "some stderr");
            }
            other => panic!("Expected JsonParseError, got: {other:?}"),
        }
    }

    #[test]
    fn parse_empty_stdout() {
        let err = parse_sf_response("", "stderr output").unwrap_err();
        match err {
            AppError::JsonParseError { stderr } => {
                assert_eq!(stderr, "stderr output");
            }
            other => panic!("Expected JsonParseError, got: {other:?}"),
        }
    }

    #[test]
    fn parse_json_missing_status() {
        let stdout = r#"{"result": {"apiVersion": "62.0"}}"#;
        // serde will fail because `status` is required (i32 has no default)
        let err = parse_sf_response(stdout, "stderr").unwrap_err();
        assert!(matches!(err, AppError::JsonParseError { .. }));
    }

    // -- ANSI escape handling --

    #[test]
    fn parse_json_with_ansi_escapes() {
        let stdout = "\x1b[31m{\"status\":0,\"result\":{\"apiVersion\":\"62.0\"}}\x1b[0m";
        let result = parse_sf_response(stdout, "").unwrap();
        let parsed: OrgDisplayResult = serde_json::from_value(result).unwrap();
        assert_eq!(parsed.api_version, "62.0");
    }

    // -- ApiVersionError wrapping --

    #[test]
    fn get_org_info_wraps_sf_cli_error_as_api_version_error() {
        // Simulate the error wrapping that get_org_info does
        let original = AppError::SfCliError {
            message: "No default org found".to_string(),
        };
        let wrapped = match original {
            AppError::SfCliError { message } => AppError::ApiVersionError { message },
            other => other,
        };
        match wrapped {
            AppError::ApiVersionError { message } => {
                assert_eq!(message, "No default org found");
            }
            other => panic!("Expected ApiVersionError, got: {other:?}"),
        }
    }

    // -- build_error_message --

    #[test]
    fn build_error_message_prefers_message() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: Some("Auth failed".to_string()),
            name: Some("AuthError".to_string()),
            stack: Some("stack trace".to_string()),
        };
        assert_eq!(build_error_message(&response), "Auth failed");
    }

    #[test]
    fn build_error_message_falls_back_to_name_and_stack() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: None,
            name: Some("FatalError".to_string()),
            stack: Some("at line 1".to_string()),
        };
        assert_eq!(build_error_message(&response), "FatalError\nat line 1");
    }

    #[test]
    fn build_error_message_name_only_when_stack_empty() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: None,
            name: Some("FatalError".to_string()),
            stack: Some(String::new()),
        };
        assert_eq!(build_error_message(&response), "FatalError");
    }

    #[test]
    fn build_error_message_all_none() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: None,
            name: None,
            stack: None,
        };
        assert_eq!(build_error_message(&response), "UnknownError");
    }

    // -- status=0 with no result --

    #[test]
    fn parse_status_0_with_no_result() {
        let stdout = r#"{"status": 0}"#;
        let err = parse_sf_response(stdout, "stderr").unwrap_err();
        assert!(matches!(err, AppError::JsonParseError { .. }));
    }

    // -- error response with empty message falls back --

    #[test]
    fn build_error_message_empty_message_falls_back() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: Some(String::new()),
            name: Some("EmptyMsgError".to_string()),
            stack: Some("trace here".to_string()),
        };
        assert_eq!(build_error_message(&response), "EmptyMsgError\ntrace here");
    }

    #[test]
    fn build_error_message_whitespace_only_message_falls_back() {
        let response = SfResponse {
            status: 1,
            result: None,
            message: Some("   ".to_string()),
            name: Some("WhitespaceMsg".to_string()),
            stack: Some("stack info".to_string()),
        };
        assert_eq!(build_error_message(&response), "WhitespaceMsg\nstack info");
    }
}
