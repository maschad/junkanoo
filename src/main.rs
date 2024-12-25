use std::io::Stdout;

use app::App;
use cli::commands::get_args;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::CrosstermBackend, Terminal};

mod app;
mod cli;

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
            .draw(|frame| cli::ui::render(frame, &app))
            .expect("Failed to draw");

        if crossterm::event::poll(std::time::Duration::from_millis(16))
            .expect("Failed to poll events")
        {
            if let crossterm::event::Event::Key(key) =
                crossterm::event::read().expect("Failed to read event")
            {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    match key.code {
                        crossterm::event::KeyCode::Char('q') => break,
                        crossterm::event::KeyCode::Down => app.select_next(),
                        crossterm::event::KeyCode::Up => app.select_previous(),
                        crossterm::event::KeyCode::Enter => {
                            app.enter_directory();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
