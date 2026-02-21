use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use super::app::{AppState, ComponentLoadState, FocusPane};

pub(crate) fn draw(frame: &mut Frame, app: &AppState) {
    let [main_area, help_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(main_area);

    draw_left_pane(frame, app, left_area);
    draw_right_pane(frame, app, right_area);
    draw_help_bar(frame, app, help_area);
}

fn draw_left_pane(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let title = if app.is_searching {
        format!("Metadata Types [/{}]", app.search_query)
    } else {
        "Metadata Types".to_string()
    };

    let border_style = if app.focus == FocusPane::Left {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&i| {
            let mt = &app.metadata_types[i];
            let has_selection = app
                .selections
                .get(&mt.xml_name)
                .is_some_and(|s| !s.is_empty());
            let prefix = if has_selection { "+ " } else { "  " };
            ListItem::new(format!("{prefix}{}", mt.xml_name))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.filtered_indices.is_empty() {
        state.select(Some(app.left_cursor));
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_right_pane(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let title = match app.highlighted_type() {
        Some(t) => t.xml_name.clone(),
        None => "Components".to_string(),
    };

    let border_style = if app.focus == FocusPane::Right {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let type_name = app.highlighted_type().map(|t| t.xml_name.as_str());

    match type_name.and_then(|name| app.component_cache.get(name)) {
        None => {
            let paragraph = Paragraph::new("").block(block);
            frame.render_widget(paragraph, area);
        }
        Some(ComponentLoadState::Loading) => {
            let paragraph = Paragraph::new("Loading...").block(block);
            frame.render_widget(paragraph, area);
        }
        Some(ComponentLoadState::Error(msg)) => {
            let paragraph = Paragraph::new(msg.as_str())
                .style(Style::default().fg(Color::Red))
                .block(block);
            frame.render_widget(paragraph, area);
        }
        Some(ComponentLoadState::Loaded(components)) => {
            let selected_set = type_name.and_then(|name| app.selections.get(name));

            let items: Vec<ListItem> = components
                .iter()
                .map(|name| {
                    let checked = selected_set.is_some_and(|s| s.contains(name));
                    let checkbox = if checked { "[x] " } else { "[ ] " };
                    ListItem::new(format!("{checkbox}{name}"))
                })
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");

            let mut state = ListState::default();
            if !components.is_empty() {
                state.select(Some(app.right_cursor));
            }

            frame.render_stateful_widget(list, area, &mut state);
        }
    }
}

fn draw_help_bar(frame: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let help_text = if app.is_searching {
        Line::from(vec![
            Span::styled("Type", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": filter  "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": stop search  "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": confirm"),
        ])
    } else {
        Line::from(vec![
            Span::styled("Tab/h/l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": pane  "),
            Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": move  "),
            Span::styled("Space", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": select  "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": confirm  "),
            Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": search  "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": cancel"),
        ])
    };

    let paragraph = Paragraph::new(help_text);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sf_client::MetadataType;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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

    fn render_to_string(app: &AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push(buffer[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn renders_metadata_types_title() {
        let app = AppState::new(sample_types());
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("Metadata Types"),
            "Should show left pane title"
        );
    }

    #[test]
    fn renders_type_names() {
        let app = AppState::new(sample_types());
        let output = render_to_string(&app, 80, 10);
        assert!(output.contains("ApexClass"), "Should show ApexClass");
        assert!(output.contains("CustomObject"), "Should show CustomObject");
    }

    #[test]
    fn renders_right_pane_with_highlighted_type_name() {
        let mut app = AppState::new(sample_types());
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["Foo".to_string()],
            )),
        );
        let output = render_to_string(&app, 80, 10);
        // Right pane title should be the highlighted type
        // The title appears in the right pane block border
        assert!(
            output.contains("ApexClass"),
            "Right pane should show highlighted type name"
        );
    }

    #[test]
    fn renders_loading_state() {
        let mut app = AppState::new(sample_types());
        app.component_cache
            .insert("ApexClass".to_string(), ComponentLoadState::Loading);
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("Loading..."),
            "Should show loading indicator"
        );
    }

    #[test]
    fn renders_error_state() {
        let mut app = AppState::new(sample_types());
        app.set_components("ApexClass", Err("Connection failed".to_string()));
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("Connection failed"),
            "Should show error message"
        );
    }

    #[test]
    fn renders_components_with_checkboxes() {
        let mut app = AppState::new(sample_types());
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["AccountController".to_string()],
            )),
        );
        let output = render_to_string(&app, 80, 10);
        assert!(output.contains("[ ]"), "Should show unchecked checkbox");
    }

    #[test]
    fn renders_selected_component() {
        let mut app = AppState::new(sample_types());
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["AccountController".to_string()],
            )),
        );
        // Select wildcard
        app.right_cursor = 0;
        app.toggle_selection();
        let output = render_to_string(&app, 80, 10);
        assert!(output.contains("[x]"), "Should show checked checkbox");
    }

    #[test]
    fn renders_search_mode() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        app.update_search('A');
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("/A"),
            "Should show search query in title: {output}"
        );
    }

    #[test]
    fn renders_help_bar() {
        let app = AppState::new(sample_types());
        let output = render_to_string(&app, 80, 10);
        assert!(output.contains("Tab/h/l"), "Help bar should show Tab/h/l");
        assert!(output.contains("j/k"), "Help bar should show j/k");
        assert!(output.contains("Space"), "Help bar should show Space");
        assert!(output.contains("Enter"), "Help bar should show Enter");
        assert!(
            output.contains("Esc: cancel"),
            "Help bar should show Esc: cancel without truncation"
        );
    }

    #[test]
    fn renders_selection_indicator_in_left_pane() {
        let mut app = AppState::new(sample_types());
        app.set_components(
            "ApexClass",
            Ok(AppState::build_component_list(
                "ApexClass",
                vec!["Foo".to_string()],
            )),
        );
        app.right_cursor = 0;
        app.toggle_selection();
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("+ ApexClass"),
            "Should show + indicator for selected type: {output}"
        );
    }

    #[test]
    fn renders_empty_right_pane_when_no_cache() {
        let app = AppState::new(sample_types());
        // No components loaded — right pane should render without crash
        let output = render_to_string(&app, 80, 10);
        assert!(!output.contains("Loading..."));
        assert!(!output.contains("[x]"));
    }

    #[test]
    fn renders_search_mode_help_bar() {
        let mut app = AppState::new(sample_types());
        app.start_search();
        let output = render_to_string(&app, 80, 10);
        assert!(
            output.contains("filter"),
            "Search help should show 'filter'"
        );
        assert!(
            output.contains("stop search"),
            "Search help should show 'stop search'"
        );
    }
}
