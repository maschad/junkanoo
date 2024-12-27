use std::io::Stdout;

use app::App;
use cli::ui;
use crossterm::{
    event::{
        poll, read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::CrosstermBackend, Terminal};

mod app;
mod cli;
mod service;

fn main() {
    // Initialize logging
    env_logger::init();
    // let matches = get_args().get_matches();

    // match matches.subcommand() {
    //     Some(("send", sub_matches)) => {
    //         println!(
    //             "Sending file: {}",
    //             sub_matches
    //                 .get_one::<String>("file_path")
    //                 .expect("file path is required")
    //         );
    //     }
    //     Some(("receive", sub_matches)) => {
    //         println!(
    //             "Connecting to peer: {}",
    //             sub_matches
    //                 .get_one::<String>("peer_identifier")
    //                 .expect("peer identifier is required")
    //         );
    //     }

    //     _ => println!("Unknown subcommand"),
    // }

    let mut app = app::App::new();
    app.connected = true;
    app.peer_id = "test-peer-123".to_string();

    let mut terminal = setup_terminal();
    render_loop(&mut terminal, &mut app);
    cleanup_terminal();
}

fn setup_terminal() -> Terminal<CrosstermBackend<Stdout>> {
    // Setup terminal
    let terminal = {
        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        ratatui::Terminal::new(backend).expect("Failed to create terminal")
    };

    enable_raw_mode().expect("Failed to enable raw mode");
    execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture)
        .expect("Failed to setup terminal");

    terminal
}

fn cleanup_terminal() {
    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
        .expect("Failed to restore terminal");
}

fn render_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) {
    // Render loop
    loop {
        terminal
            .draw(|frame| ui::render(frame, &app))
            .expect("Failed to draw");

        if poll(std::time::Duration::from_millis(16)).expect("Failed to poll events") {
            if let Event::Key(key) = read().expect("Failed to read event") {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        KeyCode::Char('q') => {
                            app.disconnect();
                        }
                        KeyCode::Char('u') => {
                            app.unselect_all();
                        }
                        KeyCode::Esc => break,
                        KeyCode::Down => app.navigate_next_file(),
                        KeyCode::Up => app.navigate_previous_file(),
                        KeyCode::Enter => {
                            app.enter_directory();
                        }
                        KeyCode::Backspace => app.go_up_previous_directory(),
                        KeyCode::Char('y') => app.select_item(),
                        KeyCode::Char('n') => app.unselect_item(),
                        KeyCode::Char('d') => {
                            if app.is_host {
                                app.start_share();
                            } else {
                                app.start_download();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
