// Folder-based metadata types that do NOT support wildcard (*) retrieval.
// Based on: https://github.com/forcedotcom/source-deploy-retrieve/blob/v6.3.1/src/registry/metadataRegistry.json
// Types with a "folderType" property in metadataRegistry.json are listed here.
const FOLDER_BASED_TYPES: &[&str] = &["Dashboard", "Document", "EmailTemplate", "Report"];

/// Returns `true` if the metadata type supports wildcard (`*`) member selection.
///
/// Types not in the folder-based list are assumed to support wildcard by default.
pub fn supports_wildcard(xml_name: &str) -> bool {
    !FOLDER_BASED_TYPES.contains(&xml_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_based_types_do_not_support_wildcard() {
        assert!(!supports_wildcard("Dashboard"));
        assert!(!supports_wildcard("Document"));
        assert!(!supports_wildcard("EmailTemplate"));
        assert!(!supports_wildcard("Report"));
    }

    #[test]
    fn common_types_support_wildcard() {
        assert!(supports_wildcard("ApexClass"));
        assert!(supports_wildcard("ApexTrigger"));
        assert!(supports_wildcard("CustomObject"));
        assert!(supports_wildcard("LightningComponentBundle"));
    }

    #[test]
    fn unknown_type_supports_wildcard_by_default() {
        assert!(supports_wildcard("SomeNewMetadataType"));
    }

    #[test]
    fn case_sensitive_matching() {
        // "dashboard" (lowercase) is not in the list
        assert!(supports_wildcard("dashboard"));
        assert!(supports_wildcard("DASHBOARD"));
    }

    #[test]
    fn folder_based_list_is_exactly_four_types() {
        assert_eq!(
            FOLDER_BASED_TYPES,
            &["Dashboard", "Document", "EmailTemplate", "Report"]
        );
    }
}
