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

    // Right pane
    pub(crate) component_cache: HashMap<String, ComponentLoadState>,
    pub(crate) right_cursor: usize,
    pub(crate) right_search_query: String,
    pub(crate) right_filtered_indices: Vec<usize>,
    pub(crate) selections: HashMap<String, HashSet<String>>,

    // Common
    pub(crate) searching_pane: Option<FocusPane>,
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
            component_cache: HashMap::new(),
            right_cursor: 0,
            right_search_query: String::new(),
            right_filtered_indices: Vec::new(),
            selections: HashMap::new(),
            searching_pane: None,
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
                    self.clear_right_search();
                }
            }
            FocusPane::Right => {
                if !self.right_filtered_indices.is_empty() {
                    if self.right_cursor == 0 {
                        self.right_cursor = self.right_filtered_indices.len() - 1;
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
                    self.clear_right_search();
                }
            }
            FocusPane::Right => {
                if !self.right_filtered_indices.is_empty() {
                    self.right_cursor = (self.right_cursor + 1) % self.right_filtered_indices.len();
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

    pub(crate) fn focus_left(&mut self) {
        self.focus = FocusPane::Left;
    }

    pub(crate) fn focus_right(&mut self) {
        self.focus = FocusPane::Right;
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

        let actual_index = match self.right_filtered_indices.get(self.right_cursor) {
            Some(&i) => i,
            None => return,
        };

        let component_name = match self.highlighted_components() {
            Some(components) => match components.get(actual_index) {
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
        match self.focus {
            FocusPane::Left => {
                self.searching_pane = Some(FocusPane::Left);
                self.apply_fuzzy_filter();
            }
            FocusPane::Right => {
                if self.can_search_right() {
                    self.searching_pane = Some(FocusPane::Right);
                    self.apply_right_fuzzy_filter();
                }
            }
        }
    }

    pub(crate) fn update_search(&mut self, ch: char) {
        match self.searching_pane {
            Some(FocusPane::Left) => {
                self.search_query.push(ch);
                self.apply_fuzzy_filter();
            }
            Some(FocusPane::Right) => {
                self.right_search_query.push(ch);
                self.apply_right_fuzzy_filter();
            }
            None => {}
        }
    }

    pub(crate) fn backspace_search(&mut self) {
        match self.searching_pane {
            Some(FocusPane::Left) => {
                self.search_query.pop();
                self.apply_fuzzy_filter();
            }
            Some(FocusPane::Right) => {
                self.right_search_query.pop();
                self.apply_right_fuzzy_filter();
            }
            None => {}
        }
    }

    pub(crate) fn end_search(&mut self) {
        self.searching_pane = None;
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
        self.clear_right_search();
    }

    pub(crate) fn apply_right_fuzzy_filter(&mut self) {
        let components = match self.highlighted_components() {
            Some(c) => c,
            None => {
                self.right_filtered_indices.clear();
                self.right_cursor = 0;
                return;
            }
        };

        let results = fuzzy_filter(&self.right_search_query, components);

        // Find `*` dynamically so that `apply_right_fuzzy_filter` does not depend on
        // `*` always being at index 0 (an implicit assumption of `build_component_list`).
        let wildcard_idx = components.iter().position(|c| c == "*");

        self.right_filtered_indices = if let Some(wc_idx) = wildcard_idx {
            let mut indices = vec![wc_idx]; // Always include `*` at the top
            indices.extend(results.into_iter().map(|(i, _)| i).filter(|&i| i != wc_idx));
            indices
        } else {
            results.into_iter().map(|(i, _)| i).collect()
        };

        self.right_cursor = 0;
    }

    pub(crate) fn rebuild_right_filtered_indices(&mut self) {
        match self.highlighted_components() {
            Some(components) => {
                self.right_filtered_indices = (0..components.len()).collect();
            }
            None => {
                self.right_filtered_indices.clear();
            }
        }
        self.right_cursor = 0;
    }

    pub(crate) fn clear_right_search(&mut self) {
        self.right_search_query.clear();
        self.rebuild_right_filtered_indices(); // This also resets right_cursor to 0
        if self.searching_pane == Some(FocusPane::Right) {
            self.searching_pane = None;
        }
    }

    pub(crate) fn can_search_right(&self) -> bool {
        self.highlighted_components().is_some()
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
        // Rebuild right filtered indices if the loaded type is currently highlighted
        if self
            .highlighted_type()
            .is_some_and(|t| t.xml_name == type_name)
        {
            self.rebuild_right_filtered_indices();
        }
    }

    /// Checks if components need to be loaded for the highlighted type.
    /// If so, sets state to Loading and returns the type name.
    /// Returns `Some` when cache is empty or contains a stale `Loading` placeholder
    /// (inserted for UI display but not backed by an actual background load).
    pub(crate) fn request_components_if_needed(&mut self) -> Option<String> {
        let type_name = self.highlighted_type()?.xml_name.clone();
        match self.component_cache.get(&type_name) {
            Some(ComponentLoadState::Loaded(_) | ComponentLoadState::Error(_)) => None,
            _ => {
                // None or Loading (stale placeholder) — request load
                self.component_cache
                    .insert(type_name.clone(), ComponentLoadState::Loading);
                Some(type_name)
            }
        }
    }

    /// Builds the component list for a type, prepending `*` if wildcard is supported.
    pub(crate) fn build_component_list(
        type_name: &str,
        mut components: Vec<String>,
    ) -> Vec<String> {
        components.sort();
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
        assert!(app.searching_pane.is_none());
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

    #[test]
    fn focus_left_sets_left() {
        let mut app = AppState::new(sample_types());
        app.focus = FocusPane::Right;
        app.focus_left();
        assert_eq!(app.focus, FocusPane::Left);
    }

    #[test]
    fn focus_left_noop_when_already_left() {
        let mut app = AppState::new(sample_types());
        assert_eq!(app.focus, FocusPane::Left);
        app.focus_left();
        assert_eq!(app.focus, FocusPane::Left);
    }

    #[test]
    fn focus_right_sets_right() {
        let mut app = AppState::new(sample_types());
        app.focus_right();
        assert_eq!(app.focus, FocusPane::Right);
    }

    #[test]
    fn focus_right_noop_when_already_right() {
        let mut app = AppState::new(sample_types());
        app.focus = FocusPane::Right;
        app.focus_right();
        assert_eq!(app.focus, FocusPane::Right);
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
        app.rebuild_right_filtered_indices();
        app.focus = FocusPane::Right;
        // Report components: ["MarketingReport", "SalesReport"] (no *, sorted)
        app.right_cursor = 0;
        app.toggle_selection();
        let selected = &app.selections["Report"];
        assert!(selected.contains("MarketingReport"));
    }

    // -- search --

    #[test]
    fn search_filters_types() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        assert_eq!(app.searching_pane, Some(FocusPane::Left));

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
        assert!(app.searching_pane.is_none());
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

    #[test]
    fn request_components_if_needed_re_requests_loading_state() {
        let mut app = AppState::new(sample_types());
        // Simulate a stale Loading placeholder (inserted for UI but not backed by a real load)
        app.component_cache
            .insert("ApexClass".to_string(), ComponentLoadState::Loading);
        let result = app.request_components_if_needed();
        assert_eq!(result, Some("ApexClass".to_string()));
    }

    #[test]
    fn request_components_if_needed_skips_error_state() {
        let mut app = AppState::new(sample_types());
        app.set_components("ApexClass", Err("fail".to_string()));
        let result = app.request_components_if_needed();
        assert!(result.is_none());
    }

    // -- build_component_list --

    #[test]
    fn build_component_list_with_wildcard() {
        let list =
            AppState::build_component_list("ApexClass", vec!["Foo".to_string(), "Bar".to_string()]);
        assert_eq!(list, vec!["*", "Bar", "Foo"]);
    }

    #[test]
    fn build_component_list_folder_type_no_wildcard() {
        let list = AppState::build_component_list("Report", vec!["SalesReport".to_string()]);
        assert_eq!(list, vec!["SalesReport"]);
    }

    #[test]
    fn build_component_list_sorts_folder_type() {
        let list = AppState::build_component_list(
            "Report",
            vec!["SalesReport".to_string(), "MarketingReport".to_string()],
        );
        assert_eq!(list, vec!["MarketingReport", "SalesReport"]);
    }

    #[test]
    fn build_component_list_empty_with_wildcard() {
        let list = AppState::build_component_list("ApexClass", vec![]);
        assert_eq!(list, vec!["*"]);
    }

    #[test]
    fn build_component_list_empty_without_wildcard() {
        let list = AppState::build_component_list("Report", vec![]);
        assert!(list.is_empty());
    }

    // -- right pane search --

    #[test]
    fn right_search_filters_components() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        assert_eq!(app.searching_pane, Some(FocusPane::Right));

        app.update_search('A');
        app.update_search('c');
        app.update_search('c');

        // Should filter to AccountController (and wildcard *)
        let filtered_names: Vec<&str> = app
            .right_filtered_indices
            .iter()
            .filter_map(|&i| {
                app.highlighted_components()
                    .and_then(|c| c.get(i))
                    .map(|s| s.as_str())
            })
            .collect();
        assert!(filtered_names.contains(&"AccountController"));
        assert!(!filtered_names.contains(&"ContactService"));
    }

    #[test]
    fn right_search_always_shows_wildcard() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();

        // Search for something that doesn't match "*"
        app.update_search('C');
        app.update_search('o');
        app.update_search('n');

        // Wildcard should still be in the list
        let filtered_names: Vec<&str> = app
            .right_filtered_indices
            .iter()
            .filter_map(|&i| {
                app.highlighted_components()
                    .and_then(|c| c.get(i))
                    .map(|s| s.as_str())
            })
            .collect();
        assert!(
            filtered_names.contains(&"*"),
            "Wildcard should always be shown: {filtered_names:?}"
        );
        assert!(filtered_names[0] == "*", "Wildcard should be first");
    }

    #[test]
    fn right_search_no_wildcard_for_folder_type() {
        let mut app = app_with_loaded_components();
        // Move to Report (folder-based, no wildcard)
        app.left_cursor = 2;
        app.rebuild_right_filtered_indices();
        app.focus = FocusPane::Right;
        app.start_search();

        app.update_search('S');
        app.update_search('a');

        let filtered_names: Vec<&str> = app
            .right_filtered_indices
            .iter()
            .filter_map(|&i| {
                app.highlighted_components()
                    .and_then(|c| c.get(i))
                    .map(|s| s.as_str())
            })
            .collect();
        assert!(filtered_names.contains(&"SalesReport"));
        assert!(!filtered_names.contains(&"*"));
    }

    #[test]
    fn right_search_always_shows_wildcard_regardless_of_position() {
        // Inject components with `*` NOT at index 0, bypassing build_component_list.
        // This verifies that apply_right_fuzzy_filter does not rely on `*` being at index 0.
        let mut app = AppState::new(sample_types());
        app.component_cache.insert(
            "ApexClass".to_string(),
            ComponentLoadState::Loaded(vec!["Foo".to_string(), "*".to_string(), "Bar".to_string()]),
        );
        app.rebuild_right_filtered_indices();
        app.focus = FocusPane::Right;
        app.start_search();
        app.update_search('B'); // matches "Bar"; "*" would not match fuzzy

        let components = app.highlighted_components().unwrap().clone();
        let filtered_names: Vec<&str> = app
            .right_filtered_indices
            .iter()
            .filter_map(|&i| components.get(i).map(|s| s.as_str()))
            .collect();
        assert!(
            filtered_names.contains(&"*"),
            "Wildcard should be shown even when not at index 0: {filtered_names:?}"
        );
        assert_eq!(
            filtered_names[0], "*",
            "Wildcard should be first in results"
        );
    }

    #[test]
    fn left_cursor_move_clears_right_search() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        app.update_search('A');
        assert!(!app.right_search_query.is_empty());

        // Switch to left pane and move cursor
        app.focus = FocusPane::Left;
        app.move_cursor_down();

        assert!(app.right_search_query.is_empty());
        assert!(app.searching_pane.is_none());
    }

    #[test]
    fn right_search_toggle_selection_correct_component() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        // Search for "Contact" to filter
        app.update_search('C');
        app.update_search('o');
        app.update_search('n');
        app.update_search('t');

        // right_filtered_indices should have: [0 (*), index_of_ContactService]
        // Move cursor to the ContactService entry (skip wildcard)
        app.right_cursor = 1;
        app.toggle_selection();

        let selected = app.selections.get("ApexClass").unwrap();
        assert!(
            selected.contains("ContactService"),
            "Should select ContactService via filtered index"
        );
    }

    #[test]
    fn right_search_cursor_wraps_on_filtered_len() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        // Filter to reduce list size
        app.update_search('A');
        app.update_search('c');
        app.update_search('c');

        let filtered_len = app.right_filtered_indices.len();
        // Move cursor down past the end should wrap
        for _ in 0..filtered_len {
            app.move_cursor_down();
        }
        assert_eq!(app.right_cursor, 0);
    }

    #[test]
    fn set_components_rebuilds_right_filtered_indices() {
        let mut app = AppState::new(sample_types());
        assert!(app.right_filtered_indices.is_empty());

        // Load components for the highlighted type (ApexClass)
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["Foo".to_string(), "Bar".to_string()],
            )),
        );

        // right_filtered_indices should be rebuilt: [0, 1, 2] for ["*", "Bar", "Foo"]
        assert_eq!(app.right_filtered_indices, vec![0, 1, 2]);
    }

    #[test]
    fn empty_filter_result_safe_toggle_and_cursor() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        // Search for something that matches nothing
        app.update_search('z');
        app.update_search('z');
        app.update_search('z');

        // Wildcard should still be there for wildcard-supporting types
        // but no other matches
        assert!(app.right_filtered_indices.len() <= 1); // only wildcard or empty

        // Cursor movement should be safe
        app.move_cursor_down();
        app.move_cursor_up();

        // toggle_selection should be safe (no crash)
        app.toggle_selection();
    }

    #[test]
    fn can_search_right_false_when_no_components() {
        let app = AppState::new(sample_types());
        assert!(!app.can_search_right());
    }

    #[test]
    fn can_search_right_true_when_loaded() {
        let app = app_with_loaded_components();
        assert!(app.can_search_right());
    }

    #[test]
    fn start_search_noop_when_right_not_loaded() {
        let mut app = AppState::new(sample_types());
        app.focus = FocusPane::Right;
        app.start_search();
        assert!(
            app.searching_pane.is_none(),
            "Should not enter search mode when components not loaded"
        );
    }

    #[test]
    fn empty_filter_result_folder_type_safe_toggle_and_cursor() {
        let mut app = app_with_loaded_components();
        // Move to Report (folder-based, no wildcard)
        app.left_cursor = 2;
        app.rebuild_right_filtered_indices();
        app.focus = FocusPane::Right;
        app.start_search();
        // Search for something that matches nothing
        app.update_search('z');
        app.update_search('z');
        app.update_search('z');

        // No wildcard for folder-based types, so list should be truly empty
        assert!(
            app.right_filtered_indices.is_empty(),
            "Folder-based type with no matches should have empty filtered indices"
        );

        // Cursor movement should be safe
        app.move_cursor_down();
        app.move_cursor_up();

        // toggle_selection should be safe (no crash)
        app.toggle_selection();
    }

    #[test]
    fn start_search_preserves_left_query() {
        let mut app = AppState::new(sample_types());
        app.focus = FocusPane::Left;
        app.start_search();
        app.update_search('a');
        app.update_search('p');
        app.end_search();

        assert_eq!(app.search_query, "ap");

        // Re-enter search mode: query should be preserved
        app.start_search();
        assert_eq!(app.search_query, "ap");
        assert!(app.searching_pane.is_some());
    }

    #[test]
    fn start_search_preserves_right_query() {
        let mut app = app_with_loaded_components();
        app.focus = FocusPane::Right;
        app.start_search();
        app.update_search('c');
        app.update_search('o');
        app.end_search();

        assert_eq!(app.right_search_query, "co");

        // Re-enter search mode: query should be preserved
        app.start_search();
        assert_eq!(app.right_search_query, "co");
        assert!(app.searching_pane.is_some());
    }
}
