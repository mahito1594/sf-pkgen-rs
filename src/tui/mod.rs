pub mod app;
pub mod event;
pub mod fuzzy;
pub mod ui;

use std::collections::BTreeMap;
use std::panic::{self, PanicHookInfo};

use crossterm::event::{Event, read as crossterm_read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::error::AppError;
use crate::sf_client::{MetadataType, SfClient};

use self::app::AppState;
use self::event::{Action, handle_key_event};
use self::ui::draw;

type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;

struct PanicHookGuard {
    original_hook: Option<PanicHook>,
}

impl PanicHookGuard {
    fn install() -> Self {
        let original_hook = panic::take_hook();
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
        let _ = panic::take_hook();
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

pub fn run_tui(
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
    loop {
        terminal.draw(|f| draw(f, app)).map_err(AppError::IoError)?;

        if let Event::Key(key_event) = crossterm_read().map_err(AppError::IoError)? {
            match handle_key_event(app, key_event) {
                Action::None => {}
                Action::LoadComponents(type_name) => {
                    // Redraw with Loading state before blocking
                    terminal.draw(|f| draw(f, app)).map_err(AppError::IoError)?;
                    load_components(app, sf_client, &type_name, target_org, api_version);
                }
                Action::Confirm(selections) => {
                    return Ok(selections);
                }
                Action::NoComponentsSelected => {
                    return Err(AppError::NoComponentsSelected);
                }
                Action::Cancel => {
                    return Err(AppError::Cancelled);
                }
            }
        }
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
