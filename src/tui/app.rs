use std::collections::{BTreeMap, HashMap, HashSet};

use crate::sf_client::MetadataType;
use crate::tui::fuzzy::fuzzy_filter;
use crate::wildcard::supports_wildcard;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusPane {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub(crate) enum ComponentLoadState {
    Loading,
    Loaded(Vec<String>),
    Error(String),
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub(crate) struct AppState {
    // Left pane
    pub(crate) metadata_types: Vec<MetadataType>,
    pub(crate) filtered_indices: Vec<usize>,
    pub(crate) left_cursor: usize,
    pub(crate) search_query: String,
    pub(crate) is_searching: bool,

    // Right pane
    pub(crate) component_cache: HashMap<String, ComponentLoadState>,
    pub(crate) right_cursor: usize,
    pub(crate) selections: HashMap<String, HashSet<String>>,

    // Common
    pub(crate) focus: FocusPane,
    pub(crate) should_quit: bool,
    pub(crate) cancelled: bool,
}

impl AppState {
    pub(crate) fn new(metadata_types: Vec<MetadataType>) -> Self {
        let filtered_indices: Vec<usize> = (0..metadata_types.len()).collect();
        Self {
            metadata_types,
            filtered_indices,
            left_cursor: 0,
            search_query: String::new(),
            is_searching: false,
            component_cache: HashMap::new(),
            right_cursor: 0,
            selections: HashMap::new(),
            focus: FocusPane::Left,
            should_quit: false,
            cancelled: false,
        }
    }

    /// Returns the metadata type at the current left cursor position.
    pub(crate) fn highlighted_type(&self) -> Option<&MetadataType> {
        self.filtered_indices
            .get(self.left_cursor)
            .map(|&i| &self.metadata_types[i])
    }

    /// Returns the list of components for the currently highlighted type (if loaded).
    fn highlighted_components(&self) -> Option<&Vec<String>> {
        let ht = self.highlighted_type()?;
        match self.component_cache.get(&ht.xml_name)? {
            ComponentLoadState::Loaded(components) => Some(components),
            _ => None,
        }
    }

    // -- Cursor movement --

    pub(crate) fn move_cursor_up(&mut self) {
        match self.focus {
            FocusPane::Left => {
                if !self.filtered_indices.is_empty() {
                    if self.left_cursor == 0 {
                        self.left_cursor = self.filtered_indices.len() - 1;
                    } else {
                        self.left_cursor -= 1;
                    }
                    self.right_cursor = 0;
                }
            }
            FocusPane::Right => {
                if let Some(components) = self.highlighted_components()
                    && !components.is_empty()
                {
                    if self.right_cursor == 0 {
                        self.right_cursor = components.len() - 1;
                    } else {
                        self.right_cursor -= 1;
                    }
                }
            }
        }
    }

    pub(crate) fn move_cursor_down(&mut self) {
        match self.focus {
            FocusPane::Left => {
                if !self.filtered_indices.is_empty() {
                    self.left_cursor = (self.left_cursor + 1) % self.filtered_indices.len();
                    self.right_cursor = 0;
                }
            }
            FocusPane::Right => {
                if let Some(components) = self.highlighted_components()
                    && !components.is_empty()
                {
                    self.right_cursor = (self.right_cursor + 1) % components.len();
                }
            }
        }
    }

    // -- Focus --

    pub(crate) fn switch_focus(&mut self) {
        self.focus = match self.focus {
            FocusPane::Left => FocusPane::Right,
            FocusPane::Right => FocusPane::Left,
        };
    }

    // -- Selection --

    /// Toggles the selection of the component at the current right cursor position.
    /// Implements wildcard exclusion: selecting `*` clears individual selections,
    /// and selecting an individual component clears `*`.
    pub(crate) fn toggle_selection(&mut self) {
        let type_name = match self.highlighted_type() {
            Some(t) => t.xml_name.clone(),
            None => return,
        };

        let component_name = match self.highlighted_components() {
            Some(components) => match components.get(self.right_cursor) {
                Some(name) => name.clone(),
                None => return,
            },
            None => return,
        };

        let selected = self.selections.entry(type_name).or_default();

        if selected.contains(&component_name) {
            // Deselect
            selected.remove(&component_name);
        } else if component_name == "*" {
            // Selecting wildcard: clear all individual selections
            selected.clear();
            selected.insert("*".to_string());
        } else {
            // Selecting individual: remove wildcard if present
            selected.remove("*");
            selected.insert(component_name);
        }
    }

    // -- Search --

    pub(crate) fn start_search(&mut self) {
        self.is_searching = true;
        self.search_query.clear();
        self.apply_fuzzy_filter();
    }

    pub(crate) fn update_search(&mut self, ch: char) {
        self.search_query.push(ch);
        self.apply_fuzzy_filter();
    }

    pub(crate) fn backspace_search(&mut self) {
        self.search_query.pop();
        self.apply_fuzzy_filter();
    }

    pub(crate) fn end_search(&mut self) {
        self.is_searching = false;
    }

    pub(crate) fn apply_fuzzy_filter(&mut self) {
        let type_names: Vec<String> = self
            .metadata_types
            .iter()
            .map(|t| t.xml_name.clone())
            .collect();

        let results = fuzzy_filter(&self.search_query, &type_names);
        self.filtered_indices = results.into_iter().map(|(i, _)| i).collect();
        self.left_cursor = 0;
        self.right_cursor = 0;
    }

    // -- Confirm / Cancel --

    /// Returns the selection result if at least one component is selected.
    pub(crate) fn confirm(&self) -> Option<BTreeMap<String, Vec<String>>> {
        let mut result = BTreeMap::new();
        for (type_name, members) in &self.selections {
            if !members.is_empty() {
                let mut sorted: Vec<String> = members.iter().cloned().collect();
                sorted.sort();
                result.insert(type_name.clone(), sorted);
            }
        }
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.should_quit = true;
        self.cancelled = true;
    }

    // -- Component loading --

    /// Sets the component load result for a metadata type.
    pub(crate) fn set_components(&mut self, type_name: &str, result: Result<Vec<String>, String>) {
        match result {
            Ok(components) => {
                self.component_cache.insert(
                    type_name.to_string(),
                    ComponentLoadState::Loaded(components),
                );
            }
            Err(msg) => {
                self.component_cache
                    .insert(type_name.to_string(), ComponentLoadState::Error(msg));
            }
        }
    }

    /// Checks if components need to be loaded for the highlighted type.
    /// If so, sets state to Loading and returns the type name.
    pub(crate) fn request_components_if_needed(&mut self) -> Option<String> {
        let type_name = self.highlighted_type()?.xml_name.clone();
        if self.component_cache.contains_key(&type_name) {
            return None;
        }
        self.component_cache
            .insert(type_name.clone(), ComponentLoadState::Loading);
        Some(type_name)
    }

    /// Builds the component list for a type, prepending `*` if wildcard is supported.
    pub(crate) fn build_component_list(
        type_name: &str,
        mut components: Vec<String>,
    ) -> Vec<String> {
        if supports_wildcard(type_name) {
            components.insert(0, "*".to_string());
        }
        components
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_types() -> Vec<MetadataType> {
        vec![
            MetadataType {
                xml_name: "ApexClass".to_string(),
            },
            MetadataType {
                xml_name: "CustomObject".to_string(),
            },
            MetadataType {
                xml_name: "Report".to_string(),
            },
        ]
    }

    fn app_with_loaded_components() -> AppState {
        let mut app = AppState::new(sample_types());
        // Load components for ApexClass (wildcard supported)
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec![
                    "AccountController".to_string(),
                    "ContactService".to_string(),
                ],
            )),
        );
        // Load components for Report (folder-based, no wildcard)
        app.set_components(
            "Report",
            Ok(AppState::build_component_list(
                "Report",
                vec!["SalesReport".to_string(), "MarketingReport".to_string()],
            )),
        );
        app
    }

    // -- new --

    #[test]
    fn new_initializes_correctly() {
        let app = AppState::new(sample_types());
        assert_eq!(app.metadata_types.len(), 3);
        assert_eq!(app.filtered_indices, vec![0, 1, 2]);
        assert_eq!(app.left_cursor, 0);
        assert_eq!(app.right_cursor, 0);
        assert_eq!(app.focus, FocusPane::Left);
        assert!(!app.should_quit);
        assert!(!app.cancelled);
        assert!(!app.is_searching);
    }

    // -- highlighted_type --

    #[test]
    fn highlighted_type_returns_first() {
        let app = AppState::new(sample_types());
        assert_eq!(app.highlighted_type().unwrap().xml_name, "ApexClass");
    }

    #[test]
    fn highlighted_type_empty_types() {
        let app = AppState::new(vec![]);
        assert!(app.highlighted_type().is_none());
    }

    // -- cursor movement (left pane) --

    #[test]
    fn move_cursor_down_left_pane() {
        let mut app = AppState::new(sample_types());
        app.move_cursor_down();
        assert_eq!(app.left_cursor, 1);
        assert_eq!(app.highlighted_type().unwrap().xml_name, "CustomObject");
    }

    #[test]
    fn move_cursor_down_wraps_left() {
        let mut app = AppState::new(sample_types());
        app.left_cursor = 2;
        app.move_cursor_down();
        assert_eq!(app.left_cursor, 0);
    }

    #[test]
    fn move_cursor_up_left_pane() {
        let mut app = AppState::new(sample_types());
        app.left_cursor = 1;
        app.move_cursor_up();
        assert_eq!(app.left_cursor, 0);
    }

    #[test]
    fn move_cursor_up_wraps_left() {
        let mut app = AppState::new(sample_types());
        app.move_cursor_up();
        assert_eq!(app.left_cursor, 2);
    }

    #[test]
    fn move_cursor_resets_right_cursor() {
        let mut app = app_with_loaded_components();
        app.right_cursor = 2;
        app.move_cursor_down();
        assert_eq!(app.right_cursor, 0);
    }

    // -- cursor movement (right pane) --

    #[test]
    fn move_cursor_down_right_pane() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.move_cursor_down();
        assert_eq!(app.right_cursor, 1);
    }

    #[test]
    fn move_cursor_down_wraps_right() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        // ApexClass has: ["*", "AccountController", "ContactService"] = 3 items
        app.right_cursor = 2;
        app.move_cursor_down();
        assert_eq!(app.right_cursor, 0);
    }

    #[test]
    fn move_cursor_up_right_pane() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.right_cursor = 1;
        app.move_cursor_up();
        assert_eq!(app.right_cursor, 0);
    }

    #[test]
    fn move_cursor_up_wraps_right() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.move_cursor_up();
        assert_eq!(app.right_cursor, 2);
    }

    // -- focus --

    #[test]
    fn switch_focus_toggles() {
        let mut app = AppState::new(sample_types());
        assert_eq!(app.focus, FocusPane::Left);
        app.switch_focus();
        assert_eq!(app.focus, FocusPane::Right);
        app.switch_focus();
        assert_eq!(app.focus, FocusPane::Left);
    }

    // -- selection --

    #[test]
    fn toggle_selection_individual() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        // Move to "AccountController" (index 1)
        app.right_cursor = 1;
        app.toggle_selection();
        let selected = app.selections.get("ApexClass").unwrap();
        assert!(selected.contains("AccountController"));
        assert!(!selected.contains("*"));
    }

    #[test]
    fn toggle_selection_wildcard_clears_individual() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        // First select individual
        app.right_cursor = 1;
        app.toggle_selection();
        assert!(app.selections["ApexClass"].contains("AccountController"));

        // Now select wildcard
        app.right_cursor = 0;
        app.toggle_selection();
        let selected = &app.selections["ApexClass"];
        assert!(selected.contains("*"));
        assert!(!selected.contains("AccountController"));
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn toggle_selection_individual_clears_wildcard() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        // First select wildcard
        app.right_cursor = 0;
        app.toggle_selection();
        assert!(app.selections["ApexClass"].contains("*"));

        // Now select individual
        app.right_cursor = 1;
        app.toggle_selection();
        let selected = &app.selections["ApexClass"];
        assert!(!selected.contains("*"));
        assert!(selected.contains("AccountController"));
    }

    #[test]
    fn toggle_selection_deselect() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.right_cursor = 1;
        app.toggle_selection();
        assert!(app.selections["ApexClass"].contains("AccountController"));

        // Toggle again to deselect
        app.toggle_selection();
        assert!(!app.selections["ApexClass"].contains("AccountController"));
    }

    #[test]
    fn toggle_selection_no_wildcard_for_folder_type() {
        let mut app = app_with_loaded_components();
        // Move to Report (index 2)
        app.left_cursor = 2;
        app.focus = FocusPane::Right;
        // Report components: ["SalesReport", "MarketingReport"] (no *)
        app.right_cursor = 0;
        app.toggle_selection();
        let selected = &app.selections["Report"];
        assert!(selected.contains("SalesReport"));
    }

    // -- search --

    #[test]
    fn search_filters_types() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        assert!(app.is_searching);

        app.update_search('A');
        app.update_search('p');
        app.update_search('e');
        app.update_search('x');

        // Should filter to ApexClass only
        assert!(!app.filtered_indices.is_empty());
        let filtered_names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.metadata_types[i].xml_name.as_str())
            .collect();
        assert!(filtered_names.contains(&"ApexClass"));
    }

    #[test]
    fn search_backspace_widens_filter() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        app.update_search('A');
        app.update_search('p');
        app.update_search('e');
        app.update_search('x');
        let narrow_count = app.filtered_indices.len();

        app.backspace_search();
        assert!(app.filtered_indices.len() >= narrow_count);
    }

    #[test]
    fn end_search_exits_search_mode() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        app.end_search();
        assert!(!app.is_searching);
    }

    #[test]
    fn search_resets_cursors() {
        let mut app = AppState::new(sample_types());
        app.left_cursor = 2;
        app.right_cursor = 1;
        app.start_search();
        app.update_search('x');
        assert_eq!(app.left_cursor, 0);
        assert_eq!(app.right_cursor, 0);
    }

    // -- confirm --

    #[test]
    fn confirm_with_selections() {
        let mut app = app_with_loaded_components();
        app.right_cursor = 0;
        app.toggle_selection(); // Select * for ApexClass

        let result = app.confirm().unwrap();
        assert_eq!(result["ApexClass"], vec!["*"]);
    }

    #[test]
    fn confirm_without_selections_returns_none() {
        let app = AppState::new(sample_types());
        assert!(app.confirm().is_none());
    }

    #[test]
    fn confirm_sorts_members() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        // Select ContactService (index 2) then AccountController (index 1)
        app.right_cursor = 2;
        app.toggle_selection();
        app.right_cursor = 1;
        app.toggle_selection();

        let result = app.confirm().unwrap();
        assert_eq!(
            result["ApexClass"],
            vec!["AccountController", "ContactService"]
        );
    }

    // -- cancel --

    #[test]
    fn cancel_sets_flags() {
        let mut app = AppState::new(sample_types());
        app.cancel();
        assert!(app.should_quit);
        assert!(app.cancelled);
    }

    // -- component loading --

    #[test]
    fn set_components_ok() {
        let mut app = AppState::new(sample_types());
        app.set_components("ApexClass", Ok(vec!["Foo".to_string()]));
        match &app.component_cache["ApexClass"] {
            ComponentLoadState::Loaded(components) => {
                assert_eq!(components, &vec!["Foo".to_string()]);
            }
            other => panic!("Expected Loaded, got: {other:?}"),
        }
    }

    #[test]
    fn set_components_error() {
        let mut app = AppState::new(sample_types());
        app.set_components("ApexClass", Err("fetch failed".to_string()));
        match &app.component_cache["ApexClass"] {
            ComponentLoadState::Error(msg) => assert_eq!(msg, "fetch failed"),
            other => panic!("Expected Error, got: {other:?}"),
        }
    }

    #[test]
    fn request_components_if_needed_first_time() {
        let mut app = AppState::new(sample_types());
        let result = app.request_components_if_needed();
        assert_eq!(result, Some("ApexClass".to_string()));
        assert!(matches!(
            app.component_cache["ApexClass"],
            ComponentLoadState::Loading
        ));
    }

    #[test]
    fn request_components_if_needed_already_cached() {
        let mut app = app_with_loaded_components();
        let result = app.request_components_if_needed();
        assert!(result.is_none());
    }

    // -- build_component_list --

    #[test]
    fn build_component_list_with_wildcard() {
        let list =
            AppState::build_component_list("ApexClass", vec!["Foo".to_string(), "Bar".to_string()]);
        assert_eq!(list, vec!["*", "Foo", "Bar"]);
    }

    #[test]
    fn build_component_list_folder_type_no_wildcard() {
        let list = AppState::build_component_list("Report", vec!["SalesReport".to_string()]);
        assert_eq!(list, vec!["SalesReport"]);
    }
}
