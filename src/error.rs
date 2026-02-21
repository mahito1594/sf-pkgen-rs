#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(
        "sf CLI not found. Visit https://developer.salesforce.com/tools/salesforcecli to install it."
    )]
    SfCliNotFound,

    #[error("{message}")]
    SfCliError { message: String },

    #[error(
        "{stderr}\nThere may be an issue with sf CLI or its plugins. Run 'sf plugins --core' and verify that @salesforce/plugin-org is included."
    )]
    JsonParseError { stderr: String },

    #[error("{message}\nPlease specify the API version explicitly with the --api-version option.")]
    ApiVersionError { message: String },

    #[error("No metadata types were found.")]
    NoMetadataTypes,

    #[error("No metadata components selected.")]
    NoComponentsSelected,

    #[error("{message}")]
    OutputPathError { message: String },

    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{message}")]
    ValidationError { message: String },

    #[error("")]
    Cancelled,
}

impl AppError {
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            AppError::Cancelled => 130,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_cancelled_returns_130() {
        assert_eq!(AppError::Cancelled.exit_code(), 130);
    }

    #[test]
    fn exit_code_non_cancelled_returns_1() {
        let cases: Vec<AppError> = vec![
            AppError::SfCliNotFound,
            AppError::SfCliError {
                message: "error".to_string(),
            },
            AppError::JsonParseError {
                stderr: "parse error".to_string(),
            },
            AppError::ApiVersionError {
                message: "bad version".to_string(),
            },
            AppError::NoMetadataTypes,
            AppError::NoComponentsSelected,
            AppError::OutputPathError {
                message: "path error".to_string(),
            },
            AppError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "test")),
            AppError::ValidationError {
                message: "invalid".to_string(),
            },
        ];
        for error in &cases {
            assert_eq!(error.exit_code(), 1, "Failed for: {error:?}");
        }
    }

    #[test]
    fn display_sf_cli_not_found() {
        let error = AppError::SfCliNotFound;
        assert_eq!(
            error.to_string(),
            "sf CLI not found. Visit https://developer.salesforce.com/tools/salesforcecli to install it."
        );
    }

    #[test]
    fn display_sf_cli_error() {
        let error = AppError::SfCliError {
            message: "something went wrong".to_string(),
        };
        assert_eq!(error.to_string(), "something went wrong");
    }

    #[test]
    fn display_json_parse_error() {
        let error = AppError::JsonParseError {
            stderr: "invalid json".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "invalid json\nThere may be an issue with sf CLI or its plugins. Run 'sf plugins --core' and verify that @salesforce/plugin-org is included."
        );
    }

    #[test]
    fn display_api_version_error() {
        let error = AppError::ApiVersionError {
            message: "unknown version".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "unknown version\nPlease specify the API version explicitly with the --api-version option."
        );
    }

    #[test]
    fn display_no_metadata_types() {
        let error = AppError::NoMetadataTypes;
        assert_eq!(error.to_string(), "No metadata types were found.");
    }

    #[test]
    fn display_no_components_selected() {
        let error = AppError::NoComponentsSelected;
        assert_eq!(error.to_string(), "No metadata components selected.");
    }

    #[test]
    fn display_output_path_error() {
        let error = AppError::OutputPathError {
            message: "manifest/package.xml は既に存在します。".to_string(),
        };
        assert_eq!(error.to_string(), "manifest/package.xml は既に存在します。");
    }

    #[test]
    fn display_validation_error() {
        let error = AppError::ValidationError {
            message: "--all requires --non-interactive".to_string(),
        };
        assert_eq!(error.to_string(), "--all requires --non-interactive");
    }

    #[test]
    fn display_cancelled_is_empty() {
        let error = AppError::Cancelled;
        assert_eq!(error.to_string(), "");
    }

    #[test]
    fn from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let app_error: AppError = io_error.into();
        assert!(matches!(app_error, AppError::IoError(_)));
        assert_eq!(app_error.to_string(), "access denied");
    }
}
