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
use libp2p::Multiaddr;
use ratatui::{prelude::CrosstermBackend, Terminal};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

mod app;
mod cli;
mod service;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();
    let matches = cli::commands::get_args().get_matches();

    // Initialize app
    let mut app: App = app::App::new();

    // Handle peer ID for download command
    let mut target_peer_addr: Option<Multiaddr> = None;

    match matches.subcommand() {
        Some(("share", sub_matches)) => {
            app.state = app::AppState::Share;
            app.is_host = true;
            app.current_path = sub_matches
                .get_one::<String>("FILE_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        }
        Some(("download", sub_matches)) => {
            app.state = app::AppState::Download;
            app.is_host = false;
            if let Some(peer_addr_str) = sub_matches.get_one::<String>("PEER_ADDR_IDENTIFIER") {
                // Parse the peer ID string into a PeerId
                match peer_addr_str.parse::<Multiaddr>() {
                    Ok(peer_addr) => {
                        target_peer_addr = Some(peer_addr);
                    }
                    Err(e) => {
                        eprintln!("Invalid peer ID format: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Peer ID is required for download command");
                std::process::exit(1);
            }
        }

        _ => println!("Unknown subcommand"),
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(start_network(&mut app, target_peer_addr));

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

async fn start_network(app: &mut App, target_peer_addr: Option<Multiaddr>) {
    let (mut client, event_stream, event_loop, listening_addrs, peer_id) =
        service::node::new().await.unwrap();

    app.peer_id = peer_id;
    app.listening_addrs = listening_addrs;

    if !app.is_host {
        client.dial(target_peer_addr.unwrap()).await.unwrap();

        // tokio::spawn(client.connection_handler(peer_id, listening_addrs));
    }
}
