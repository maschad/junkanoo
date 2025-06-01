use std::path::PathBuf;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, AppState};

pub fn render(frame: &mut Frame, app: &App) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Percentage(100)])
        .split(frame.area());

    let main_block = Block::default()
        .title(format!(
            "{} File Browser - PeerID: {}",
            if app.is_host { "Host" } else { "Remote" },
            app.peer_id
        ))
        .borders(Borders::ALL);
    frame.render_widget(main_block, frame.area());

    // Split into left and right panels
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    // Left panel with file browser
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(10),   // File tree
            Constraint::Length(3), // Status
            Constraint::Length(3), // Connect info
        ])
        .split(horizontal_chunks[0]);

    render_title(frame, left_chunks[0], app.is_host);

    if app.is_loading {
        let loading_text = "Downloading files...";
        let loading = Paragraph::new(loading_text)
            .block(Block::default().title("Loading...").borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, left_chunks[1]);
    } else if app.is_warning() {
        tracing::warn!("Warning: {}", app.warning_message());
        let warning = Paragraph::new(app.warning_message().to_string())
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        frame.render_widget(warning, left_chunks[1]);
    } else {
        render_file_tree(frame, app, left_chunks[1]);
    }

    render_status(frame, app, left_chunks[2]);
    render_connect_info(frame, app, left_chunks[3]);

    // Right panel with preview
    let preview_block = Block::default().title(" Preview ").borders(Borders::ALL);

    let preview_content = app
        .selected_index
        .and_then(|index| app.directory_items.get(index))
        .map_or("No file selected".to_string(), |item| item.preview.clone());

    let preview = Paragraph::new(preview_content)
        .block(preview_block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(preview, horizontal_chunks[1]);
}

fn render_title(frame: &mut Frame, area: Rect, is_host: bool) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} File Browser", if is_host { "Host" } else { "Remote" }),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" Navigate | "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" Open dir | "),
        Span::styled("Y", Style::default().fg(Color::Yellow)),
        Span::raw(" Select | "),
        Span::styled("N", Style::default().fg(Color::Yellow)),
        Span::raw(" Unselect | "),
        Span::styled("U", Style::default().fg(Color::Yellow)),
        Span::raw(" Unselect all | "),
        Span::styled("Backspace", Style::default().fg(Color::Yellow)),
        Span::raw(" Back"),
        Span::raw(" | "),
        Span::styled("D", Style::default().fg(Color::Yellow)),
        Span::raw(" Begin Download | "),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, area);
}

fn render_file_tree(frame: &mut Frame, app: &App, area: Rect) {
    if app.is_loading {
        let loading_text = "Downloading files...";
        let loading = Paragraph::new(loading_text)
            .block(Block::default().title("Status").borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
    } else if app.is_warning() {
        let warning = Paragraph::new(app.warning_message().to_string())
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        frame.render_widget(warning, area);
    } else {
        let items: Vec<ListItem> = app
            .directory_items
            .iter()
            .map(|item| {
                let indent = "  ".repeat(item.depth);
                let selected = match app.state {
                    AppState::Share => {
                        #[allow(clippy::option_if_let_else)]
                        if let Ok(rel_path) = item.path.strip_prefix(&app.current_path) {
                            if app.items_to_share.contains(&rel_path.to_path_buf()) {
                                "🔵 "
                            } else {
                                "  "
                            }
                        } else {
                            "  "
                        }
                    }
                    AppState::Download => {
                        if app.items_to_download.contains(&item.path) {
                            "🔵 "
                        } else {
                            "  "
                        }
                    }
                };
                let prefix = if item.is_dir { "📁 " } else { "📄 " };

                let style = if app.selected_index.is_some_and(|idx| idx == item.index) {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if match app.state {
                    AppState::Share => app.items_to_share.contains(
                        &item
                            .path
                            .strip_prefix(&app.current_path)
                            .unwrap_or(&PathBuf::new())
                            .to_path_buf(),
                    ),
                    AppState::Download => app.items_to_download.contains(&item.path),
                } {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(selected, style),
                    Span::styled(format!("{}{}", prefix, item.name), style),
                ]))
            })
            .collect();

        let current_path = format!(" {} ", app.current_path.display());
        let files_list = List::new(items)
            .block(Block::default().title(current_path).borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(files_list, area);
    }
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    // Calculate total selected items
    let total_selected = app.items_to_share.len() + app.items_to_download.len();

    // Create status bar
    let status = if app.is_connected() {
        format!(
            "Connected to peer: {} | Selected items: {}",
            app.connected_peer_id
                .map_or("Unknown".to_string(), |id| id.to_string()),
            total_selected
        )
    } else {
        format!("Disconnected | Selected items: {total_selected}")
    };

    let status_style = if app.is_connected() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };

    let status_widget = Paragraph::new(status)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(status_widget, area);
}

fn render_connect_info(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if app.listening_addrs.is_empty() {
        vec![ListItem::new("No listening addresses available")]
    } else {
        app.listening_addrs
            .iter()
            .map(|addr| {
                let addr_str = if addr.to_string().contains("/p2p/") {
                    addr.to_string()
                } else {
                    format!("{}/p2p/{}", addr, app.peer_id)
                };
                let icon = if app.clipboard_success {
                    "✅ " // Checkmark icon
                } else {
                    "📋 " // Clipboard icon
                };
                ListItem::new(Line::from(vec![
                    Span::raw(icon),
                    Span::styled(
                        addr_str,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]))
            })
            .collect()
    };

    let connect_widget = List::new(items).block(
        Block::default()
            .title(" Addresses (Press X to Copy the address) ")
            .borders(Borders::ALL),
    );

    frame.render_widget(connect_widget, area);
}
