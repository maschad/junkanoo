use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Percentage(100)])
        .split(frame.area());

    let main_block = Block::default()
        .title(format!(
            "{} File Browser",
            if app.is_host { "Host" } else { "Remote" }
        ))
        .borders(Borders::ALL);
    frame.render_widget(main_block, frame.area());

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(10),   // File tree
            Constraint::Length(3), // Status
        ])
        .split(chunks[0]);

    render_title(frame, inner_chunks[0], app.is_host);
    render_file_tree(frame, app, inner_chunks[1]);
    render_status(frame, app, inner_chunks[2]);
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
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
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
            let selected = if item.selected { "⚪ " } else { "  " };
            let prefix = if item.is_dir { "📁 " } else { "📄 " };

            let style = if Some(item.index) == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if item.selected {
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
    let status = if app.connected {
        format!(
            "Connected to: {} | Selected items: {}",
            app.peer_id,
            app.directory_items.iter().filter(|i| i.selected).count()
        )
    } else {
        "Disconnected".to_string()
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
