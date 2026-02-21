use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{AppState, FocusPane};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    LoadComponents(String),
    Confirm(BTreeMap<String, Vec<String>>),
    NoComponentsSelected,
    Cancel,
}

pub(crate) fn handle_key_event(app: &mut AppState, key: KeyEvent) -> Action {
    // Ctrl+C always cancels
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.cancel();
        return Action::Cancel;
    }

    // Search mode
    if app.is_searching {
        return handle_search_key(app, key);
    }

    // Normal mode
    match key.code {
        KeyCode::Esc => {
            app.cancel();
            Action::Cancel
        }
        KeyCode::Tab => {
            app.switch_focus();
            maybe_load_components(app)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_cursor_up();
            maybe_load_components(app)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_cursor_down();
            maybe_load_components(app)
        }
        KeyCode::Char('h') => {
            app.focus_left();
            maybe_load_components(app)
        }
        KeyCode::Char('l') => {
            app.focus_right();
            maybe_load_components(app)
        }
        KeyCode::Char('/') if app.focus == FocusPane::Left => {
            app.start_search();
            Action::None
        }
        KeyCode::Char(' ') if app.focus == FocusPane::Right => {
            app.toggle_selection();
            Action::None
        }
        KeyCode::Enter => {
            if let Some(selections) = app.confirm() {
                app.should_quit = true;
                Action::Confirm(selections)
            } else {
                app.should_quit = true;
                Action::NoComponentsSelected
            }
        }
        _ => Action::None,
    }
}

fn handle_search_key(app: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.end_search();
            Action::None
        }
        KeyCode::Enter => {
            app.end_search();
            maybe_load_components(app)
        }
        KeyCode::Backspace => {
            app.backspace_search();
            Action::None
        }
        KeyCode::Char(c) => {
            app.update_search(c);
            Action::None
        }
        _ => Action::None,
    }
}

fn maybe_load_components(app: &mut AppState) -> Action {
    match app.request_components_if_needed() {
        Some(type_name) => Action::LoadComponents(type_name),
        None => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sf_client::MetadataType;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn sample_types() -> Vec<MetadataType> {
        vec![
            MetadataType {
                xml_name: "ApexClass".to_string(),
            },
            MetadataType {
                xml_name: "CustomObject".to_string(),
            },
        ]
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    fn app_with_components() -> AppState {
        let mut app = AppState::new(sample_types());
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["Foo".to_string()],
            )),
        );
        app
    }

    // -- Cancel --

    #[test]
    fn esc_cancels() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, key(KeyCode::Esc));
        assert_eq!(action, Action::Cancel);
        assert!(app.cancelled);
    }

    #[test]
    fn ctrl_c_cancels() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, ctrl_c());
        assert_eq!(action, Action::Cancel);
        assert!(app.cancelled);
    }

    // -- Cursor movement --

    #[test]
    fn down_moves_cursor_and_may_load() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, key(KeyCode::Down));
        assert_eq!(app.left_cursor, 1);
        // Should request loading for CustomObject
        assert_eq!(action, Action::LoadComponents("CustomObject".to_string()));
    }

    #[test]
    fn up_moves_cursor() {
        let mut app = AppState::new(sample_types());
        app.left_cursor = 1;
        // Cache CustomObject to avoid LoadComponents
        app.set_components("CustomObject", Ok(vec![]));
        let action = handle_key_event(&mut app, key(KeyCode::Up));
        assert_eq!(app.left_cursor, 0);
        // ApexClass not cached, should request load
        assert_eq!(action, Action::LoadComponents("ApexClass".to_string()));
    }

    #[test]
    fn j_moves_cursor_down() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.left_cursor, 1);
        assert_eq!(action, Action::LoadComponents("CustomObject".to_string()));
    }

    #[test]
    fn j_moves_cursor_down_in_right_pane() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        let action = handle_key_event(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.right_cursor, 1);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn k_moves_cursor_up() {
        let mut app = AppState::new(sample_types());
        app.left_cursor = 1;
        app.set_components("CustomObject", Ok(vec![]));
        let action = handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.left_cursor, 0);
        assert_eq!(action, Action::LoadComponents("ApexClass".to_string()));
    }

    #[test]
    fn k_moves_cursor_up_in_right_pane() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        // ApexClass has: ["*", "Foo"] = 2 items; cursor wraps to last
        let action = handle_key_event(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.right_cursor, 1);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn h_focuses_left_pane() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        let action = handle_key_event(&mut app, key(KeyCode::Char('h')));
        assert_eq!(app.focus, FocusPane::Left);
        // ApexClass already cached, no load needed
        assert_eq!(action, Action::None);
    }

    #[test]
    fn l_focuses_right_pane() {
        let mut app = app_with_components();
        assert_eq!(app.focus, FocusPane::Left);
        let action = handle_key_event(&mut app, key(KeyCode::Char('l')));
        assert_eq!(app.focus, FocusPane::Right);
        // ApexClass already cached, no load needed
        assert_eq!(action, Action::None);
    }

    // -- Tab --

    #[test]
    fn tab_switches_focus() {
        let mut app = app_with_components();
        assert_eq!(app.focus, FocusPane::Left);
        handle_key_event(&mut app, key(KeyCode::Tab));
        assert_eq!(app.focus, FocusPane::Right);
    }

    // -- Space (selection) --

    #[test]
    fn space_toggles_selection_in_right_pane() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        app.right_cursor = 0; // "*"
        let action = handle_key_event(&mut app, key(KeyCode::Char(' ')));
        assert_eq!(action, Action::None);
        assert!(app.selections["ApexClass"].contains("*"));
    }

    #[test]
    fn space_does_nothing_in_left_pane() {
        let mut app = app_with_components();
        let action = handle_key_event(&mut app, key(KeyCode::Char(' ')));
        assert_eq!(action, Action::None);
        assert!(app.selections.is_empty());
    }

    // -- Enter (confirm) --

    #[test]
    fn enter_with_selections_confirms() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        app.right_cursor = 0;
        app.toggle_selection(); // Select *

        let action = handle_key_event(&mut app, key(KeyCode::Enter));
        match action {
            Action::Confirm(selections) => {
                assert!(selections.contains_key("ApexClass"));
            }
            other => panic!("Expected Confirm, got: {other:?}"),
        }
        assert!(app.should_quit);
    }

    #[test]
    fn enter_without_selections_returns_no_components_selected() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::NoComponentsSelected);
        assert!(app.should_quit);
    }

    // -- Search mode --

    #[test]
    fn slash_starts_search() {
        let mut app = AppState::new(sample_types());
        let action = handle_key_event(&mut app, key(KeyCode::Char('/')));
        assert_eq!(action, Action::None);
        assert!(app.is_searching);
    }

    #[test]
    fn slash_ignored_in_right_pane() {
        let mut app = app_with_components();
        app.focus = FocusPane::Right;
        handle_key_event(&mut app, key(KeyCode::Char('/')));
        assert!(!app.is_searching);
    }

    #[test]
    fn search_mode_chars_update_query() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        handle_key_event(&mut app, key(KeyCode::Char('a')));
        assert_eq!(app.search_query, "a");
    }

    #[test]
    fn search_mode_backspace_removes_char() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        app.update_search('a');
        app.update_search('b');
        handle_key_event(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.search_query, "a");
    }

    #[test]
    fn search_mode_esc_ends_search() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        handle_key_event(&mut app, key(KeyCode::Esc));
        assert!(!app.is_searching);
        assert!(!app.cancelled, "Esc in search mode should not cancel TUI");
    }

    #[test]
    fn search_mode_enter_ends_search() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        let action = handle_key_event(&mut app, key(KeyCode::Enter));
        assert!(!app.is_searching);
        // Should potentially trigger component loading
        assert!(matches!(action, Action::None | Action::LoadComponents(_)));
    }

    #[test]
    fn ctrl_c_cancels_during_search() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        let action = handle_key_event(&mut app, ctrl_c());
        assert_eq!(action, Action::Cancel);
        assert!(app.cancelled);
    }
}
