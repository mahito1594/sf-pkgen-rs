use std::collections::BTreeMap;
use std::panic::{self, PanicHookInfo};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{Event, poll as crossterm_poll, read as crossterm_read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::error::AppError;
use crate::sf_client::{MetadataType, SfClient};

use super::app::{AppState, ComponentLoadState};
use super::event::{Action, handle_key_event};
use super::ui::draw;

struct LoadResult {
    type_name: String,
    result: Result<Vec<String>, String>,
}

type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;

struct PanicHookGuard {
    original_hook: Option<PanicHook>,
}

impl PanicHookGuard {
    fn install() -> Self {
        let original_hook = panic::take_hook();
        // The original hook is stored in the guard for restoration on normal exit.
        // During panic, we restore the terminal and print panic info directly rather
        // than chaining to the original hook, since ownership prevents sharing it
        // between the closure and the guard.
        panic::set_hook(Box::new(|info| {
            if let Ok(mut panic_tty) = open_tty() {
                restore_terminal(&mut panic_tty);
            }
            eprintln!("{info}");
        }));
        PanicHookGuard {
            original_hook: Some(original_hook),
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        if let Some(hook) = self.original_hook.take() {
            panic::set_hook(hook);
        }
    }
}

fn open_tty() -> Result<std::fs::File, AppError> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(AppError::IoError)
}

fn restore_terminal(tty: &mut std::fs::File) {
    let _ = disable_raw_mode();
    let _ = execute!(tty, LeaveAlternateScreen);
}

pub(crate) fn run_tui(
    metadata_types: Vec<MetadataType>,
    sf_client: &dyn SfClient,
    target_org: Option<&str>,
    api_version: &str,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    let mut tty = open_tty()?;

    // Setup terminal
    enable_raw_mode()?;
    if let Err(e) = execute!(tty, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(AppError::IoError(e));
    }

    // Install panic hook to restore terminal on panic (RAII: Drop restores original hook)
    let _panic_guard = PanicHookGuard::install();

    let backend = CrosstermBackend::new(match open_tty() {
        Ok(f) => f,
        Err(e) => {
            restore_terminal(&mut tty);
            return Err(e);
        }
    });
    let mut terminal = Terminal::new(backend).map_err(|e| {
        restore_terminal(&mut tty);
        AppError::IoError(e)
    })?;

    let mut app = AppState::new(metadata_types);

    // Initial component load for the first highlighted type
    if let Some(type_name) = app.request_components_if_needed() {
        load_components(&mut app, sf_client, &type_name, target_org, api_version);
    }

    let result = run_event_loop(&mut terminal, &mut app, sf_client, target_org, api_version);

    // Restore terminal
    restore_terminal(&mut tty);

    // _panic_guard is dropped here, restoring the original panic hook

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::fs::File>>,
    app: &mut AppState,
    sf_client: &dyn SfClient,
    target_org: Option<&str>,
    api_version: &str,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<LoadResult>();
        let mut loading_active = false;

        loop {
            // 1. Draw
            terminal.draw(|f| draw(f, app)).map_err(AppError::IoError)?;

            // 2. Check for completed background load
            if let Ok(load_result) = rx.try_recv() {
                apply_load_result(app, load_result);
                loading_active = false;
                continue; // Redraw immediately with updated state
            }

            // 3. If idle, start background load for current position if needed
            if !loading_active && let Some(type_name) = app.request_components_if_needed() {
                spawn_load(scope, &tx, sf_client, type_name, target_org, api_version);
                loading_active = true;
            }

            // 4. Event waiting strategy:
            //    - Loading active: 50ms poll (check channel frequently)
            //    - Idle: blocking read (no CPU waste)
            if loading_active
                && !crossterm_poll(Duration::from_millis(50)).map_err(AppError::IoError)?
            {
                continue; // Timeout → back to top (draw + channel check)
            }

            // 5. Process events (same batching logic as before)
            let first_event = crossterm_read().map_err(AppError::IoError)?;

            let mut pending_loads: Vec<String> = Vec::new();
            if let Some(result) = process_event(app, &first_event, &mut pending_loads) {
                cleanup_stale_loading(app, &pending_loads);
                return result;
            }

            // Drain any queued events without blocking
            while crossterm_poll(Duration::ZERO).map_err(AppError::IoError)? {
                let event = crossterm_read().map_err(AppError::IoError)?;
                if let Some(result) = process_event(app, &event, &mut pending_loads) {
                    cleanup_stale_loading(app, &pending_loads);
                    return result;
                }
            }

            // 6. Clean up stale Loading entries
            cleanup_stale_loading(app, &pending_loads);

            // 7. Start background load for final cursor position, or
            //    ensure current type shows "Loading..." while another load runs
            if !loading_active && let Some(type_name) = app.request_components_if_needed() {
                spawn_load(scope, &tx, sf_client, type_name, target_org, api_version);
                loading_active = true;
            } else if loading_active && let Some(ht) = app.highlighted_type() {
                let name = ht.xml_name.clone();
                app.component_cache
                    .entry(name)
                    .or_insert(ComponentLoadState::Loading);
            }
        }
    })
}

/// Processes a single event and returns `Some(result)` if the TUI should exit.
/// When a `LoadComponents` action is produced, the type name is recorded in
/// `pending_loads` instead of triggering an immediate sf CLI call.
fn process_event(
    app: &mut AppState,
    event: &Event,
    pending_loads: &mut Vec<String>,
) -> Option<Result<BTreeMap<String, Vec<String>>, AppError>> {
    let Event::Key(key_event) = event else {
        return None;
    };
    match handle_key_event(app, *key_event) {
        Action::None => None,
        Action::LoadComponents(type_name) => {
            pending_loads.push(type_name);
            None
        }
        Action::Confirm(selections) => Some(Ok(selections)),
        Action::NoComponentsSelected => Some(Err(AppError::NoComponentsSelected)),
        Action::Cancel => Some(Err(AppError::Cancelled)),
    }
}

/// Removes cache entries that are still in `Loading` state for the given type names.
/// This prevents intermediate positions (from rapid cursor movement) from being
/// permanently stuck as "Loading", allowing them to be re-fetched when the user
/// navigates back.
fn cleanup_stale_loading(app: &mut AppState, pending_loads: &[String]) {
    for type_name in pending_loads {
        if let Some(ComponentLoadState::Loading) = app.component_cache.get(type_name) {
            app.component_cache.remove(type_name);
        }
    }
}

fn spawn_load<'scope, 'env>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    tx: &mpsc::Sender<LoadResult>,
    sf_client: &'env dyn SfClient,
    type_name: String,
    target_org: Option<&'env str>,
    api_version: &'env str,
) {
    let tx = tx.clone();
    scope.spawn(move || {
        let result = sf_client
            .list_metadata(&type_name, target_org, api_version)
            .map(|components| {
                let names = components.into_iter().map(|c| c.full_name).collect();
                AppState::build_component_list(&type_name, names)
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(LoadResult { type_name, result });
    });
}

fn apply_load_result(app: &mut AppState, load_result: LoadResult) {
    match load_result.result {
        Ok(components) => app.set_components(&load_result.type_name, Ok(components)),
        Err(msg) => app.set_components(&load_result.type_name, Err(msg)),
    }
}

fn load_components(
    app: &mut AppState,
    sf_client: &dyn SfClient,
    type_name: &str,
    target_org: Option<&str>,
    api_version: &str,
) {
    match sf_client.list_metadata(type_name, target_org, api_version) {
        Ok(components) => {
            let names: Vec<String> = components.into_iter().map(|c| c.full_name).collect();
            let list = AppState::build_component_list(type_name, names);
            app.set_components(type_name, Ok(list));
        }
        Err(e) => {
            app.set_components(type_name, Err(e.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::FocusPane;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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

    fn make_key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    // -- cleanup_stale_loading --

    #[test]
    fn cleanup_removes_loading_entries() {
        let mut app = AppState::new(sample_types());
        app.component_cache
            .insert("ApexClass".to_string(), ComponentLoadState::Loading);
        app.component_cache
            .insert("CustomObject".to_string(), ComponentLoadState::Loading);

        cleanup_stale_loading(
            &mut app,
            &["ApexClass".to_string(), "CustomObject".to_string()],
        );

        assert!(!app.component_cache.contains_key("ApexClass"));
        assert!(!app.component_cache.contains_key("CustomObject"));
    }

    #[test]
    fn cleanup_preserves_loaded_and_error_entries() {
        let mut app = AppState::new(sample_types());
        app.set_components("ApexClass", Ok(vec!["Foo".to_string()]));
        app.set_components("CustomObject", Err("fail".to_string()));

        cleanup_stale_loading(
            &mut app,
            &["ApexClass".to_string(), "CustomObject".to_string()],
        );

        assert!(matches!(
            app.component_cache.get("ApexClass"),
            Some(ComponentLoadState::Loaded(_))
        ));
        assert!(matches!(
            app.component_cache.get("CustomObject"),
            Some(ComponentLoadState::Error(_))
        ));
    }

    #[test]
    fn cleanup_then_request_re_fetches_loading_type() {
        let mut app = AppState::new(sample_types());
        // Insert a Loading entry, then clean it up
        app.component_cache
            .insert("ApexClass".to_string(), ComponentLoadState::Loading);
        cleanup_stale_loading(&mut app, &["ApexClass".to_string()]);
        assert!(!app.component_cache.contains_key("ApexClass"));

        // request_components_if_needed should now return Some for the cleaned type
        let result = app.request_components_if_needed();
        assert_eq!(result, Some("ApexClass".to_string()));
        assert!(matches!(
            app.component_cache.get("ApexClass"),
            Some(ComponentLoadState::Loading)
        ));
    }

    // -- process_event --

    #[test]
    fn process_event_ignores_non_key_events() {
        let mut app = AppState::new(sample_types());
        let mut pending = Vec::new();
        let event = Event::FocusGained;
        let result = process_event(&mut app, &event, &mut pending);
        assert!(result.is_none());
        assert!(pending.is_empty());
    }

    #[test]
    fn process_event_records_load_components() {
        let mut app = AppState::new(sample_types());
        let mut pending = Vec::new();
        // Down arrow moves cursor and triggers LoadComponents for CustomObject
        let event = make_key_event(KeyCode::Down);
        let result = process_event(&mut app, &event, &mut pending);
        assert!(result.is_none());
        assert_eq!(pending, vec!["CustomObject".to_string()]);
    }

    #[test]
    fn process_event_returns_some_on_cancel() {
        let mut app = AppState::new(sample_types());
        let mut pending = Vec::new();
        let event = make_key_event(KeyCode::Esc);
        let result = process_event(&mut app, &event, &mut pending);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), Err(AppError::Cancelled)));
    }

    #[test]
    fn process_event_returns_some_on_confirm() {
        let mut app = AppState::new(sample_types());
        // Set up a selection so confirm succeeds
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["Foo".to_string()],
            )),
        );
        app.focus = FocusPane::Right;
        app.right_cursor = 0; // "*"
        app.toggle_selection();

        let mut pending = Vec::new();
        let event = make_key_event(KeyCode::Enter);
        let result = process_event(&mut app, &event, &mut pending);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }
}
