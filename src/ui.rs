use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, DirectoryItem};

pub fn render(frame: &mut Frame, app: &App) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),     // Title bar
            Constraint::Min(10),    // Main content
            Constraint::Length(3),  // Status bar
        ])
        .split(frame.size());

    render_title(frame, chunks[0]);
    render_file_tree(frame, app, chunks[1]);
    render_status(frame, app, chunks[2]);
}

fn render_title(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new("Remote File Browser")
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, area);
}

fn render_file_tree(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .directory_items
        .iter()
        .map(|item| {
            let prefix = if item.is_dir { "📁 " } else { "📄 " };
            let style = if Some(item.index) == app.selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", prefix, item.name), style)
            ]))
        })
        .collect();

    let files_list = List::new(items)
        .block(Block::default().title("Files").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Yellow));

    frame.render_widget(files_list, area);
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let status = if app.connected {
        format!("Connected to: {}", app.peer_id)
    } else {
        "Disconnected".to_string()
    };

    let status_widget = Paragraph::new(status)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL));
    
    frame.render_widget(status_widget, area);
}

