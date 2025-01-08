use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
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
            app.peer_id.to_string()
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
    render_file_tree(frame, app, left_chunks[1]);
    render_status(frame, app, left_chunks[2]);
    render_connect_info(frame, app, left_chunks[3]);

    // Right panel with preview
    let preview_block = Block::default().title(" Preview ").borders(Borders::ALL);

    let preview_content = if let Some(index) = app.selected_index {
        if let Some(item) = app.directory_items.get(index) {
            if !item.is_dir {
                match std::fs::read_to_string(&item.path) {
                    Ok(contents) => contents,
                    Err(_) => "Unable to read file contents".to_string(),
                }
            } else {
                format!("Directory: {}", item.name)
            }
        } else {
            "No file selected".to_string()
        }
    } else {
        "No file selected".to_string()
    };

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
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, area);
}

fn render_file_tree(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .directory_items
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            let selected = match app.state {
                AppState::Share if app.items_to_share.contains(&item.path) => "🔵 ",
                AppState::Download if app.items_to_download.contains(&item.path) => "🔵 ",
                _ => "  ",
            };
            let prefix = if item.is_dir { "📁 " } else { "📄 " };

            let style = if Some(item.index) == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if matches!(app.state, AppState::Share)
                && app.items_to_share.contains(&item.path)
                || matches!(app.state, AppState::Download)
                    && app.items_to_download.contains(&item.path)
            {
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

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let total_selected = match app.state {
        AppState::Share => app.items_to_share.len(),
        AppState::Download => app.items_to_download.len(),
        _ => 0,
    };

    let status = if app.connected {
        format!(
            "Connected to: {} | Selected items: {}",
            app.peer_id, total_selected
        )
    } else {
        format!("Disconnected | Selected items: {}", total_selected)
    };

    let status_style = if app.connected {
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
            // .filter(|addr: &&libp2p::Multiaddr| !addr.to_string().contains("127.0.0"))
            .map(|addr| {
                let addr_str = if addr.to_string().contains("/p2p/") {
                    addr.to_string()
                } else {
                    format!("{}/p2p/{}", addr, app.peer_id)
                };
                ListItem::new(addr_str)
            })
            .collect()
    };

    let connect_widget = List::new(items)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().title(" Addresses ").borders(Borders::ALL));

    frame.render_widget(connect_widget, area);
}
