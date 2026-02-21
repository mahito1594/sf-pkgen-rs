use std::collections::BTreeMap;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

pub(crate) struct PackageXmlInput {
    /// Map of metadata type name to selected members (sorted by BTreeMap key order).
    pub(crate) types: BTreeMap<String, Vec<String>>,
    pub(crate) api_version: String,
}

/// Generates a `package.xml` string from the given input.
///
/// Format rules (per specification):
/// - XML declaration: `<?xml version="1.0" encoding="UTF-8"?>`
/// - `<types>` sorted by `<name>` alphabetically (case-sensitive, guaranteed by BTreeMap)
/// - Within `<types>`: `<members>` first, then `<name>`
/// - `<members>` sorted alphabetically; `*` always comes first
/// - Indent: 4 spaces, newline: LF, trailing newline present
pub(crate) fn generate_package_xml(input: &PackageXmlInput) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut writer = Writer::new_with_indent(&mut buf, b' ', 4);

    // XML declaration
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect("writing XML declaration");

    // <Package xmlns="...">
    let mut package = BytesStart::new("Package");
    package.push_attribute(("xmlns", "http://soap.sforce.com/2006/04/metadata"));
    writer
        .write_event(Event::Start(package))
        .expect("writing Package start");

    // <types> entries (BTreeMap guarantees alphabetical order by key)
    for (type_name, members) in &input.types {
        let mut sorted_members = members.clone();
        sort_members(&mut sorted_members);

        writer
            .write_event(Event::Start(BytesStart::new("types")))
            .expect("writing types start");

        for member in &sorted_members {
            writer
                .write_event(Event::Start(BytesStart::new("members")))
                .expect("writing members start");
            writer
                .write_event(Event::Text(BytesText::new(member)))
                .expect("writing members text");
            writer
                .write_event(Event::End(BytesEnd::new("members")))
                .expect("writing members end");
        }

        writer
            .write_event(Event::Start(BytesStart::new("name")))
            .expect("writing name start");
        writer
            .write_event(Event::Text(BytesText::new(type_name)))
            .expect("writing name text");
        writer
            .write_event(Event::End(BytesEnd::new("name")))
            .expect("writing name end");

        writer
            .write_event(Event::End(BytesEnd::new("types")))
            .expect("writing types end");
    }

    // <version>
    writer
        .write_event(Event::Start(BytesStart::new("version")))
        .expect("writing version start");
    writer
        .write_event(Event::Text(BytesText::new(&input.api_version)))
        .expect("writing version text");
    writer
        .write_event(Event::End(BytesEnd::new("version")))
        .expect("writing version end");

    // </Package>
    writer
        .write_event(Event::End(BytesEnd::new("Package")))
        .expect("writing Package end");

    let mut xml = String::from_utf8(buf).expect("UTF-8 XML output");
    // Ensure trailing newline
    if !xml.ends_with('\n') {
        xml.push('\n');
    }
    xml
}

/// Sorts members alphabetically with `*` always first.
fn sort_members(members: &mut Vec<String>) {
    members.sort();
    // If `*` is present, move it to the front
    if let Some(pos) = members.iter().position(|m| m == "*") {
        members.remove(pos);
        members.insert(0, "*".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(types: Vec<(&str, Vec<&str>)>, api_version: &str) -> PackageXmlInput {
        let mut map = BTreeMap::new();
        for (name, members) in types {
            map.insert(
                name.to_string(),
                members.into_iter().map(|s| s.to_string()).collect(),
            );
        }
        PackageXmlInput {
            types: map,
            api_version: api_version.to_string(),
        }
    }

    #[test]
    fn generates_xml_declaration() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    }

    #[test]
    fn generates_package_element_with_namespace() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        assert!(xml.contains("<Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">"));
    }

    #[test]
    fn wildcard_member_only() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        assert!(xml.contains("<members>*</members>"));
        assert!(xml.contains("<name>ApexClass</name>"));
    }

    #[test]
    fn individual_members_sorted() {
        let input = make_input(
            vec![("ApexClass", vec!["ContactService", "AccountController"])],
            "62.0",
        );
        let xml = generate_package_xml(&input);
        let ac_pos = xml.find("<members>AccountController</members>").unwrap();
        let cs_pos = xml.find("<members>ContactService</members>").unwrap();
        assert!(
            ac_pos < cs_pos,
            "AccountController should come before ContactService"
        );
    }

    #[test]
    fn wildcard_always_first_among_members() {
        // Even though `*` comes after `A` in some orderings, it should be first
        let input = make_input(vec![("ApexClass", vec!["AccountController", "*"])], "62.0");
        let xml = generate_package_xml(&input);
        let star_pos = xml.find("<members>*</members>").unwrap();
        let ac_pos = xml.find("<members>AccountController</members>").unwrap();
        assert!(star_pos < ac_pos, "* should come before AccountController");
    }

    #[test]
    fn types_sorted_by_name() {
        let input = make_input(
            vec![("CustomObject", vec!["Account"]), ("ApexClass", vec!["*"])],
            "62.0",
        );
        let xml = generate_package_xml(&input);
        let apex_pos = xml.find("<name>ApexClass</name>").unwrap();
        let custom_pos = xml.find("<name>CustomObject</name>").unwrap();
        assert!(
            apex_pos < custom_pos,
            "ApexClass should come before CustomObject"
        );
    }

    #[test]
    fn version_element_present() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        assert!(xml.contains("<version>62.0</version>"));
    }

    #[test]
    fn trailing_newline() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        assert!(xml.ends_with('\n'));
    }

    #[test]
    fn indent_is_four_spaces() {
        let input = make_input(vec![("ApexClass", vec!["*"])], "62.0");
        let xml = generate_package_xml(&input);
        // <types> should be indented by 4 spaces
        assert!(xml.contains("\n    <types>"));
        // <members> should be indented by 8 spaces
        assert!(xml.contains("\n        <members>"));
    }

    #[test]
    fn members_then_name_order_within_types() {
        let input = make_input(vec![("ApexClass", vec!["Foo"])], "62.0");
        let xml = generate_package_xml(&input);
        let members_pos = xml.find("<members>Foo</members>").unwrap();
        let name_pos = xml.find("<name>ApexClass</name>").unwrap();
        assert!(
            members_pos < name_pos,
            "<members> should come before <name>"
        );
    }

    #[test]
    fn multiple_types_with_mixed_members() {
        let input = make_input(
            vec![
                ("ApexClass", vec!["*"]),
                ("Report", vec!["SalesReport", "MarketingReport"]),
            ],
            "61.0",
        );
        let xml = generate_package_xml(&input);
        // ApexClass has wildcard
        assert!(xml.contains("<members>*</members>"));
        // Report has individual members sorted
        let mr_pos = xml.find("<members>MarketingReport</members>").unwrap();
        let sr_pos = xml.find("<members>SalesReport</members>").unwrap();
        assert!(mr_pos < sr_pos);
        assert!(xml.contains("<version>61.0</version>"));
    }

    #[test]
    fn full_xml_snapshot() {
        let input = make_input(
            vec![
                ("ApexClass", vec!["*"]),
                ("CustomObject", vec!["Account", "Contact"]),
            ],
            "62.0",
        );
        let xml = generate_package_xml(&input);
        let expected = "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<Package xmlns=\"http://soap.sforce.com/2006/04/metadata\">
    <types>
        <members>*</members>
        <name>ApexClass</name>
    </types>
    <types>
        <members>Account</members>
        <members>Contact</members>
        <name>CustomObject</name>
    </types>
    <version>62.0</version>
</Package>
";
        assert_eq!(xml, expected);
    }

    // -- sort_members --

    #[test]
    fn sort_members_with_wildcard() {
        let mut members = vec!["Zebra".to_string(), "*".to_string(), "Alpha".to_string()];
        sort_members(&mut members);
        assert_eq!(members, vec!["*", "Alpha", "Zebra"]);
    }

    #[test]
    fn sort_members_without_wildcard() {
        let mut members = vec!["Zebra".to_string(), "Alpha".to_string()];
        sort_members(&mut members);
        assert_eq!(members, vec!["Alpha", "Zebra"]);
    }

    #[test]
    fn sort_members_empty() {
        let mut members: Vec<String> = vec![];
        sort_members(&mut members);
        assert!(members.is_empty());
    }
}
