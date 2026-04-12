// This module implements package.xml inheritance for the --inherit option.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;

use crate::error::AppError;
use crate::wildcard;

/// Parsed content of a package.xml used for inheriting selections.
#[derive(Debug)]
pub(crate) struct InheritedPackage {
    /// Map from metadata type name to member list.
    /// If a type has a wildcard member, the list is exactly `["*"]`.
    pub(crate) types: HashMap<String, Vec<String>>,
    /// API version extracted from `<version>`, if present.
    pub(crate) version: Option<String>,
}

/// Parses a `package.xml` file and returns the inherited selections.
///
/// Returns `AppError::IoError` if the file cannot be read.
/// Returns `AppError::InheritParseError` if the XML is malformed.
pub(crate) fn parse_package_xml(path: &Path) -> Result<InheritedPackage, AppError> {
    let content = std::fs::read_to_string(path)?;
    parse_xml_content(&content, path)
}

fn parse_xml_content(content: &str, path: &Path) -> Result<InheritedPackage, AppError> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let path_str = path.to_string_lossy().into_owned();

    let mut types: HashMap<String, Vec<String>> = HashMap::new();
    let mut version: Option<String> = None;

    let mut in_types = false;
    let mut current_name: Option<String> = None;
    let mut current_members: Vec<String> = Vec::new();
    let mut current_tag: Option<String> = None;
    // Track open element depth to detect unclosed tags at EOF.
    let mut depth: i32 = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                let local_name = e.local_name();
                let tag = std::str::from_utf8(local_name.as_ref()).map_err(|err| {
                    AppError::InheritParseError {
                        path: path_str.clone(),
                        message: format!("invalid UTF-8 in element name: {err}"),
                    }
                })?;
                match tag {
                    "types" => {
                        in_types = true;
                        current_name = None;
                        current_members = Vec::new();
                    }
                    _ => {
                        current_tag = Some(tag.to_owned());
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let decoded = e.decode().map_err(|err| AppError::InheritParseError {
                    path: path_str.clone(),
                    message: err.to_string(),
                })?;
                let text = unescape(&decoded)
                    .map_err(|err| AppError::InheritParseError {
                        path: path_str.clone(),
                        message: err.to_string(),
                    })?
                    .into_owned();

                if in_types {
                    match current_tag.as_deref() {
                        Some("name") => current_name = Some(text),
                        Some("members") => current_members.push(text),
                        _ => {}
                    }
                } else if current_tag.as_deref() == Some("version") {
                    version = Some(text);
                }
            }
            Ok(Event::End(e)) => {
                depth -= 1;
                let local_name = e.local_name();
                let tag = std::str::from_utf8(local_name.as_ref()).map_err(|err| {
                    AppError::InheritParseError {
                        path: path_str.clone(),
                        message: format!("invalid UTF-8 in element name: {err}"),
                    }
                })?;
                if tag == "types" {
                    if let Some(name) = current_name.take() {
                        let members = normalize_members(std::mem::take(&mut current_members));
                        types
                            .entry(name)
                            .and_modify(|existing| {
                                existing.extend(members.iter().cloned());
                                *existing = normalize_members(std::mem::take(existing));
                            })
                            .or_insert(members);
                    }
                    current_members = Vec::new();
                    in_types = false;
                }
                current_tag = None;
            }
            Ok(Event::Empty(_)) => {
                // Self-closing elements do not change depth and carry no text.
            }
            Ok(Event::Eof) => {
                if depth != 0 {
                    return Err(AppError::InheritParseError {
                        path: path_str,
                        message: "unexpected end of file: document is not well-formed".to_string(),
                    });
                }
                break;
            }
            Err(e) => {
                return Err(AppError::InheritParseError {
                    path: path_str,
                    message: e.to_string(),
                });
            }
            _ => {}
        }
    }

    Ok(InheritedPackage { types, version })
}

/// Returns `true` if the member list represents a wildcard selection (`["*"]`).
pub(crate) fn is_wildcard_members(members: &[String]) -> bool {
    members.len() == 1 && members[0] == "*"
}

/// Normalises the member list for a metadata type.
/// If `*` is present among the members, it wins and all individual members are discarded.
fn normalize_members(members: Vec<String>) -> Vec<String> {
    if members.iter().any(|m| m == "*") {
        vec!["*".to_string()]
    } else {
        members
    }
}

/// Resolves the inherited package selections against the types and components
/// available in the current org.
///
/// Rules:
/// - Types not present in `org_types` are skipped with a warning.
/// - For wildcard types (`["*"]`), the selection is accepted only when the type
///   supports wildcards; folder-based types (Dashboard, Document, etc.) are skipped
///   with a warning.
/// - For individual members, those present in `org_components[type]` are kept;
///   absent members are skipped with a warning.
/// - Types in `skipped_member_types` are excluded with a warning because their
///   components could not be fetched from the org.
/// - A type whose every individual member is skipped is excluded from the result.
///
/// Returns `(selections, warnings)`.
pub(crate) fn resolve_inherited_selections(
    inherited: &InheritedPackage,
    org_types: &HashSet<String>,
    org_components: &HashMap<String, Vec<String>>,
    skipped_member_types: &HashSet<String>,
) -> (HashMap<String, HashSet<String>>, Vec<String>) {
    let mut selections: HashMap<String, HashSet<String>> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for (type_name, members) in &inherited.types {
        if !org_types.contains(type_name) {
            warnings.push(format!(
                "Skipping '{type_name}': metadata type not found in the org."
            ));
            continue;
        }

        // Wildcard selection — only valid for types that support it.
        // Folder-based types (Dashboard, Document, etc.) do not support wildcard.
        if is_wildcard_members(members) {
            if !wildcard::supports_wildcard(type_name) {
                warnings.push(format!(
                    "Skipping wildcard '*' for '{type_name}': folder-based types do not support wildcard selection."
                ));
                continue;
            }
            let mut set = HashSet::new();
            set.insert("*".to_string());
            selections.insert(type_name.clone(), set);
            continue;
        }

        if skipped_member_types.contains(type_name) {
            warnings.push(format!(
                "Skipping inherited members for '{type_name}': failed to fetch components from the org."
            ));
            continue;
        }

        // Individual members — validate against known org components.
        let org_member_list = org_components.get(type_name);
        let mut selected: HashSet<String> = HashSet::new();

        for member in members {
            let exists = org_member_list.is_some_and(|list| list.contains(member));

            if exists {
                selected.insert(member.clone());
            } else {
                warnings.push(format!(
                    "Skipping '{member}' in '{type_name}': component not found in the org."
                ));
            }
        }

        if !selected.is_empty() {
            selections.insert(type_name.clone(), selected);
        }
    }

    (selections, warnings)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::io::Write;
    use std::path::Path;

    use tempfile::NamedTempFile;

    use crate::error::AppError;

    use super::*;

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn write_temp_xml(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("failed to write temp file");
        file
    }

    /// Builds a valid package.xml with the given types and version.
    ///
    /// Each entry in `types` is `(type_name, members)`.
    fn make_package_xml(types: &[(&str, &[&str])], version: &str) -> String {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n",
        );
        for (type_name, members) in types {
            xml.push_str("    <types>\n");
            for member in *members {
                xml.push_str(&format!("        <members>{member}</members>\n"));
            }
            xml.push_str(&format!("        <name>{type_name}</name>\n"));
            xml.push_str("    </types>\n");
        }
        xml.push_str(&format!("    <version>{version}</version>\n"));
        xml.push_str("</Package>\n");
        xml
    }

    // -------------------------------------------------------------------------
    // parse_package_xml: normal cases
    // -------------------------------------------------------------------------

    #[test]
    fn parse_package_xml_extracts_type_name_and_members() {
        // Given: a valid package.xml with one type and individual members
        let xml = make_package_xml(
            &[("ApexClass", &["AccountController", "ContactService"])],
            "62.0",
        );
        let file = write_temp_xml(&xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: the type and members are extracted correctly
        let members = result
            .types
            .get("ApexClass")
            .expect("ApexClass should be present");
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"AccountController".to_string()));
        assert!(members.contains(&"ContactService".to_string()));
    }

    #[test]
    fn parse_package_xml_extracts_version() {
        // Given: a package.xml with a specific version
        let xml = make_package_xml(&[("ApexClass", &["*"])], "61.0");
        let file = write_temp_xml(&xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: the version is extracted
        assert_eq!(result.version, Some("61.0".to_string()));
    }

    #[test]
    fn parse_package_xml_wildcard_member_identified() {
        // Given: a package.xml with a wildcard member
        let xml = make_package_xml(&[("ApexClass", &["*"])], "62.0");
        let file = write_temp_xml(&xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: the type's members list contains only "*"
        let members = result
            .types
            .get("ApexClass")
            .expect("ApexClass should be present");
        assert_eq!(members, &vec!["*".to_string()]);
    }

    #[test]
    fn parse_package_xml_wildcard_with_individual_members_wildcard_wins() {
        // Given: a package.xml where a type has both "*" and individual members
        let xml = make_package_xml(
            &[("ApexClass", &["AccountController", "*", "ContactService"])],
            "62.0",
        );
        let file = write_temp_xml(&xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: only "*" remains; individual members are discarded
        let members = result
            .types
            .get("ApexClass")
            .expect("ApexClass should be present");
        assert_eq!(members, &vec!["*".to_string()]);
    }

    #[test]
    fn parse_package_xml_multiple_types() {
        // Given: a package.xml with multiple metadata types
        let xml = make_package_xml(
            &[
                ("ApexClass", &["*"]),
                ("CustomObject", &["Account", "Contact"]),
                ("Report", &["FolderA/MyReport"]),
            ],
            "62.0",
        );
        let file = write_temp_xml(&xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: all types are present with correct members
        assert!(result.types.contains_key("ApexClass"));
        assert!(result.types.contains_key("CustomObject"));
        assert!(result.types.contains_key("Report"));
        assert_eq!(result.types.len(), 3);
    }

    #[test]
    fn parse_package_xml_merges_repeated_types_blocks() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   <types>\n\
                       <members>AccountController</members>\n\
                       <name>ApexClass</name>\n\
                   </types>\n\
                   <types>\n\
                       <members>ContactService</members>\n\
                       <name>ApexClass</name>\n\
                   </types>\n\
                   <version>62.0</version>\n\
                   </Package>\n";
        let file = write_temp_xml(xml);

        let result = parse_package_xml(file.path()).unwrap();

        assert_eq!(
            result.types.get("ApexClass"),
            Some(&vec![
                "AccountController".to_string(),
                "ContactService".to_string()
            ])
        );
    }

    #[test]
    fn parse_package_xml_no_version_returns_none() {
        // Given: a package.xml with no <version> tag
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                   <Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">\n\
                   </Package>\n";
        let file = write_temp_xml(xml);

        // When: parsing the file
        let result = parse_package_xml(file.path()).unwrap();

        // Then: version is None
        assert_eq!(result.version, None);
    }

    // -------------------------------------------------------------------------
    // parse_package_xml: error cases
    // -------------------------------------------------------------------------

    #[test]
    fn parse_package_xml_file_not_found_returns_io_error() {
        // Given: a path that does not exist
        let path = Path::new("/nonexistent/path/package.xml");

        // When: parsing a non-existent file
        let err = parse_package_xml(path).unwrap_err();

        // Then: an IoError is returned
        assert!(
            matches!(err, AppError::IoError(_)),
            "Expected IoError, got: {err:?}"
        );
    }

    #[test]
    fn parse_package_xml_invalid_xml_returns_inherit_parse_error() {
        // Given: a file with malformed XML
        let file = write_temp_xml("this is not xml at all <<<");

        // When: parsing the file
        let err = parse_package_xml(file.path()).unwrap_err();

        // Then: an InheritParseError is returned (not IoError)
        assert!(
            matches!(err, AppError::InheritParseError { .. }),
            "Expected InheritParseError, got: {err:?}"
        );
    }

    #[test]
    fn parse_package_xml_unclosed_tag_returns_inherit_parse_error() {
        // Given: a file with structurally invalid XML (unclosed tag)
        let file = write_temp_xml(
            "<?xml version=\"1.0\"?>\n\
             <Package>\n\
             <types><name>ApexClass</name>\n",
        );

        // When: parsing the file
        let err = parse_package_xml(file.path()).unwrap_err();

        // Then: an InheritParseError is returned
        assert!(
            matches!(err, AppError::InheritParseError { .. }),
            "Expected InheritParseError, got: {err:?}"
        );
    }

    #[test]
    fn parse_package_xml_error_message_contains_path() {
        // Given: a file with malformed XML
        let file = write_temp_xml("not valid xml <<<");
        let path_str = file.path().to_string_lossy().to_string();

        // When: parsing the file
        let err = parse_package_xml(file.path()).unwrap_err();

        // Then: the error message references the file path
        assert!(
            err.to_string().contains(&path_str),
            "Error message should contain the file path. Got: {err}"
        );
    }

    // -------------------------------------------------------------------------
    // resolve_inherited_selections: normal cases
    // -------------------------------------------------------------------------

    #[test]
    fn resolve_empty_inherited_returns_empty_result() {
        // Given: an InheritedPackage with no types
        let inherited = InheritedPackage {
            types: HashMap::new(),
            version: None,
        };
        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: empty selections and no warnings
        assert!(selections.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_wildcard_type_added_with_wildcard_selection() {
        // Given: an InheritedPackage with a wildcard type
        let mut types = HashMap::new();
        types.insert("ApexClass".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: ApexClass is selected with wildcard, no warnings
        let selected = selections
            .get("ApexClass")
            .expect("ApexClass should be in selections");
        assert_eq!(selected, &HashSet::from(["*".to_string()]));
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_individual_members_present_in_org_are_selected() {
        // Given: an InheritedPackage with individual members, all present in org
        let mut types = HashMap::new();
        types.insert(
            "ApexClass".to_string(),
            vec![
                "AccountController".to_string(),
                "ContactService".to_string(),
            ],
        );
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let mut org_components: HashMap<String, Vec<String>> = HashMap::new();
        org_components.insert(
            "ApexClass".to_string(),
            vec![
                "AccountController".to_string(),
                "ContactService".to_string(),
                "OtherClass".to_string(),
            ],
        );

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: both members are selected, no warnings
        let selected = selections
            .get("ApexClass")
            .expect("ApexClass should be in selections");
        assert!(selected.contains("AccountController"));
        assert!(selected.contains("ContactService"));
        assert!(
            !selected.contains("OtherClass"),
            "OtherClass was not in inherited"
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_type_not_in_org_is_skipped_with_warning() {
        // Given: an InheritedPackage with a type that doesn't exist in the org
        let mut types = HashMap::new();
        types.insert("ObsoleteType".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: type is not in selections, a warning is emitted
        assert!(!selections.contains_key("ObsoleteType"));
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("ObsoleteType"),
            "Warning should mention the skipped type. Got: {}",
            warnings[0]
        );
    }

    #[test]
    fn resolve_individual_member_not_in_org_is_skipped_with_warning() {
        // Given: an InheritedPackage where one member doesn't exist in org
        let mut types = HashMap::new();
        types.insert(
            "ApexClass".to_string(),
            vec!["ExistingClass".to_string(), "DeletedClass".to_string()],
        );
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let mut org_components: HashMap<String, Vec<String>> = HashMap::new();
        org_components.insert("ApexClass".to_string(), vec!["ExistingClass".to_string()]);

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: only ExistingClass is selected; DeletedClass emits a warning
        let selected = selections
            .get("ApexClass")
            .expect("ApexClass should be in selections");
        assert!(selected.contains("ExistingClass"));
        assert!(!selected.contains("DeletedClass"));

        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("DeletedClass"),
            "Warning should mention the skipped member. Got: {}",
            warnings[0]
        );
    }

    #[test]
    fn resolve_all_members_missing_from_org_type_excluded_from_selections() {
        // Given: an InheritedPackage where all members of a type are absent from org
        let mut types = HashMap::new();
        types.insert(
            "ApexClass".to_string(),
            vec!["GoneClass1".to_string(), "GoneClass2".to_string()],
        );
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let mut org_components: HashMap<String, Vec<String>> = HashMap::new();
        org_components.insert("ApexClass".to_string(), vec![]); // no components in org

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: ApexClass is not in selections (no valid members), warnings for each missing member
        // The type should not appear in selections if no members were resolved
        assert!(!selections.contains_key("ApexClass") || selections["ApexClass"].is_empty());
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn resolve_warnings_are_in_english() {
        // Given: a type that doesn't exist in the org
        let mut types = HashMap::new();
        types.insert("MissingType".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::new();
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (_, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: warning message is in English (contains ASCII-only words typical for English)
        assert!(!warnings.is_empty());
        // Basic check: warning contains ASCII text, which is a proxy for English
        assert!(
            warnings[0].is_ascii(),
            "Warning message should be in English (ASCII). Got: {}",
            warnings[0]
        );
    }

    #[test]
    fn resolve_multiple_types_mix_of_found_and_missing() {
        // Given: two types — one exists in org, one does not
        let mut types = HashMap::new();
        types.insert("ApexClass".to_string(), vec!["*".to_string()]);
        types.insert("ObsoleteType".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: ApexClass is selected, ObsoleteType is skipped with warning
        assert!(selections.contains_key("ApexClass"));
        assert!(!selections.contains_key("ObsoleteType"));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn resolve_wildcard_selection_contains_only_wildcard() {
        // Given: a wildcard type in inherited package
        let mut types = HashMap::new();
        types.insert("ApexTrigger".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexTrigger".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, _) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: the selection for ApexTrigger is exactly {"*"} — no other members
        let selected = selections
            .get("ApexTrigger")
            .expect("ApexTrigger should be selected");
        assert_eq!(selected.len(), 1);
        assert!(selected.contains("*"));
    }

    // -------------------------------------------------------------------------
    // resolve_inherited_selections: wildcard exclusion rule
    // -------------------------------------------------------------------------

    #[test]
    fn resolve_does_not_mix_wildcard_and_individual_in_output() {
        // Given: inherited types already normalized (no * + individual mixing in input)
        // The output should never have both "*" and individual members for the same type
        let mut types = HashMap::new();
        // After parse_package_xml normalization, this would only have ["*"]
        // But testing defensively with resolved output
        types.insert("ApexClass".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, _) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: the selection does not contain both "*" and individual members
        if let Some(selected) = selections.get("ApexClass") {
            if selected.contains("*") {
                // If wildcard is present, no individual members should be present
                assert_eq!(
                    selected.len(),
                    1,
                    "Wildcard selection must contain only '*', got: {selected:?}"
                );
            }
        }
    }

    // -------------------------------------------------------------------------
    // resolve_inherited_selections: folder-based type wildcard guard
    // -------------------------------------------------------------------------

    #[test]
    fn resolve_folder_based_type_with_wildcard_is_skipped_with_warning() {
        // Given: an inherited package with a folder-based type (Report) using wildcard
        let mut types = HashMap::new();
        types.insert("Report".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["Report".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: Report is NOT in selections (folder-based types do not support wildcard)
        assert!(!selections.contains_key("Report"));
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("Report"),
            "Warning should mention the skipped type. Got: {}",
            warnings[0]
        );
    }

    #[test]
    fn resolve_non_folder_based_type_with_wildcard_is_accepted() {
        // Given: an inherited package with a non-folder-based type (ApexClass) using wildcard
        let mut types = HashMap::new();
        types.insert("ApexClass".to_string(), vec!["*".to_string()]);
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();

        // When: resolving selections
        let (selections, warnings) =
            resolve_inherited_selections(&inherited, &org_types, &org_components, &HashSet::new());

        // Then: ApexClass is selected with wildcard and no warnings
        let selected = selections
            .get("ApexClass")
            .expect("ApexClass should be in selections");
        assert_eq!(selected, &HashSet::from(["*".to_string()]));
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_skips_member_types_when_component_fetch_failed() {
        let mut types = HashMap::new();
        types.insert(
            "ApexClass".to_string(),
            vec!["AccountController".to_string()],
        );
        let inherited = InheritedPackage {
            types,
            version: None,
        };

        let org_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);
        let org_components: HashMap<String, Vec<String>> = HashMap::new();
        let skipped_member_types: HashSet<String> = HashSet::from(["ApexClass".to_string()]);

        let (selections, warnings) = resolve_inherited_selections(
            &inherited,
            &org_types,
            &org_components,
            &skipped_member_types,
        );

        assert!(!selections.contains_key("ApexClass"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ApexClass"));
        assert!(warnings[0].contains("failed to fetch components"));
    }

    // -------------------------------------------------------------------------
    // is_wildcard_members
    // -------------------------------------------------------------------------

    #[test]
    fn is_wildcard_members_returns_true_for_single_wildcard() {
        assert!(is_wildcard_members(&["*".to_string()]));
    }

    #[test]
    fn is_wildcard_members_returns_false_for_individual_members() {
        assert!(!is_wildcard_members(&["AccountController".to_string()]));
    }

    #[test]
    fn is_wildcard_members_returns_false_for_empty() {
        assert!(!is_wildcard_members(&[]));
    }

    #[test]
    fn is_wildcard_members_returns_false_for_multiple_members_including_wildcard() {
        // normalize_members would collapse this to ["*"], but is_wildcard_members
        // checks the slice as-is
        assert!(!is_wildcard_members(&[
            "*".to_string(),
            "AccountController".to_string()
        ]));
    }
}
