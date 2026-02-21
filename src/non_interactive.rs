use std::collections::BTreeMap;

use crate::error::AppError;
use crate::sf_client::{MetadataType, SfClient};
use crate::wildcard::supports_wildcard;

/// Resolves metadata selections in non-interactive mode.
///
/// Determines the target types (all or a subset via `--types`), validates them
/// against the available metadata types, and builds a selection map:
/// - Wildcard-supported types get `["*"]`
/// - Folder-based types get their full component list via `sf_client.list_metadata()`
pub(crate) fn resolve(
    sf_client: &dyn SfClient,
    metadata_types: &[MetadataType],
    all: bool,
    types: Option<&[String]>,
    target_org: Option<&str>,
    api_version: &str,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    // 1. Determine target type names
    let target_names: Vec<String> = if all {
        metadata_types.iter().map(|t| t.xml_name.clone()).collect()
    } else {
        normalize_types(types.unwrap_or(&[]))?
    };

    // 2. Validate against known metadata types
    let known: Vec<&str> = metadata_types.iter().map(|t| t.xml_name.as_str()).collect();
    for name in &target_names {
        if !known.contains(&name.as_str()) {
            return Err(AppError::ValidationError {
                message: format!("Unknown metadata type: {name}."),
            });
        }
    }

    // 3. Build selection map
    let mut selections = BTreeMap::new();
    for name in &target_names {
        if supports_wildcard(name) {
            selections.insert(name.clone(), vec!["*".to_string()]);
        } else {
            let components = sf_client.list_metadata(name, target_org, api_version)?;
            let names: Vec<String> = components.into_iter().map(|c| c.full_name).collect();
            if !names.is_empty() {
                selections.insert(name.clone(), names);
            }
        }
    }

    // 4. Check for empty selections
    if selections.is_empty() {
        return Err(AppError::NoComponentsSelected);
    }

    Ok(selections)
}

/// Normalizes `--types` values: trims whitespace, rejects empty entries, deduplicates.
fn normalize_types(types: &[String]) -> Result<Vec<String>, AppError> {
    let mut seen = Vec::new();
    for t in types {
        let trimmed = t.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError {
                message: "Metadata type list for --types must not contain empty entries."
                    .to_string(),
            });
        }
        if !seen.contains(&trimmed) {
            seen.push(trimmed);
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sf_client::{MetadataComponent, OrgInfo};

    struct MockNonInteractiveSfClient {
        components: BTreeMap<String, Vec<MetadataComponent>>,
        fail_list_metadata: bool,
    }

    impl SfClient for MockNonInteractiveSfClient {
        fn check_sf_exists(&self) -> Result<(), AppError> {
            Ok(())
        }

        fn get_org_info(&self, _target_org: Option<&str>) -> Result<OrgInfo, AppError> {
            Ok(OrgInfo {
                api_version: "62.0".to_string(),
            })
        }

        fn list_metadata_types(
            &self,
            _target_org: Option<&str>,
            _api_version: &str,
        ) -> Result<Vec<MetadataType>, AppError> {
            Ok(vec![])
        }

        fn list_metadata(
            &self,
            metadata_type: &str,
            _target_org: Option<&str>,
            _api_version: &str,
        ) -> Result<Vec<MetadataComponent>, AppError> {
            if self.fail_list_metadata {
                return Err(AppError::SfCliError {
                    message: "list_metadata failed".to_string(),
                });
            }
            Ok(self
                .components
                .get(metadata_type)
                .cloned()
                .unwrap_or_default())
        }
    }

    fn sample_types() -> Vec<MetadataType> {
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

    #[test]
    fn unknown_type_returns_validation_error() {
        let client = MockNonInteractiveSfClient {
            components: BTreeMap::new(),
            fail_list_metadata: false,
        };
        let types = vec!["NonExistent".to_string()];
        let err = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap_err();
        match err {
            AppError::ValidationError { message } => {
                assert!(message.contains("Unknown metadata type: NonExistent"));
            }
            other => panic!("Expected ValidationError, got: {other:?}"),
        }
    }

    #[test]
    fn empty_token_returns_validation_error() {
        let client = MockNonInteractiveSfClient {
            components: BTreeMap::new(),
            fail_list_metadata: false,
        };
        let types = vec!["ApexClass".to_string(), "".to_string()];
        let err = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap_err();
        match err {
            AppError::ValidationError { message } => {
                assert!(message.contains("empty entries"));
            }
            other => panic!("Expected ValidationError, got: {other:?}"),
        }
    }

    #[test]
    fn dedup_normalizes_duplicate_types() {
        let mut components = BTreeMap::new();
        components.insert(
            "Report".to_string(),
            vec![MetadataComponent {
                full_name: "SalesReport".to_string(),
            }],
        );
        let client = MockNonInteractiveSfClient {
            components,
            fail_list_metadata: false,
        };
        let types = vec![
            "ApexClass".to_string(),
            "Report".to_string(),
            "ApexClass".to_string(), // duplicate
        ];
        let result = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap();
        // ApexClass should appear once with wildcard, Report once with components
        assert_eq!(result.len(), 2);
        assert_eq!(result["ApexClass"], vec!["*"]);
        assert_eq!(result["Report"], vec!["SalesReport"]);
    }

    #[test]
    fn wildcard_type_gets_star() {
        let client = MockNonInteractiveSfClient {
            components: BTreeMap::new(),
            fail_list_metadata: false,
        };
        let types = vec!["ApexClass".to_string()];
        let result = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap();
        assert_eq!(result["ApexClass"], vec!["*"]);
    }

    #[test]
    fn folder_based_type_calls_list_metadata() {
        let mut components = BTreeMap::new();
        components.insert(
            "Report".to_string(),
            vec![
                MetadataComponent {
                    full_name: "SalesReport".to_string(),
                },
                MetadataComponent {
                    full_name: "MarketingReport".to_string(),
                },
            ],
        );
        let client = MockNonInteractiveSfClient {
            components,
            fail_list_metadata: false,
        };
        let types = vec!["Report".to_string()];
        let result = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap();
        assert_eq!(result["Report"].len(), 2);
        assert!(result["Report"].contains(&"SalesReport".to_string()));
        assert!(result["Report"].contains(&"MarketingReport".to_string()));
    }

    #[test]
    fn all_types_zero_components_returns_no_components_selected() {
        // All types are folder-based with no components
        let folder_types = vec![
            MetadataType {
                xml_name: "Report".to_string(),
            },
            MetadataType {
                xml_name: "Dashboard".to_string(),
            },
        ];
        let client = MockNonInteractiveSfClient {
            components: BTreeMap::new(), // no components for any type
            fail_list_metadata: false,
        };
        let err = resolve(&client, &folder_types, true, None, None, "62.0").unwrap_err();
        assert!(matches!(err, AppError::NoComponentsSelected));
    }

    #[test]
    fn list_metadata_failure_returns_error() {
        let client = MockNonInteractiveSfClient {
            components: BTreeMap::new(),
            fail_list_metadata: true,
        };
        let types = vec!["Report".to_string()]; // folder-based, will call list_metadata
        let err = resolve(&client, &sample_types(), false, Some(&types), None, "62.0").unwrap_err();
        assert!(matches!(err, AppError::SfCliError { .. }));
    }
}
