use std::{io::Stdout, sync::Arc};

use app::{App, DirectoryItem};
use arboard::Clipboard;
use cli::ui;
use crossterm::{
    event::{
        poll, read, DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyCode,
        KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{Stream, StreamExt};
use human_panic::{setup_panic, Metadata};
use libp2p::{multiaddr::Protocol, Multiaddr};
use parking_lot::Mutex;
use ratatui::{prelude::CrosstermBackend, Terminal};
use service::node::Event as NetworkEvent;
use tokio::spawn;
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;

mod app;
mod cli;
mod service;

#[tokio::main]
async fn main() {
    setup_panic_handler();
    setup_logger();

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
                        tracing::error!("Invalid peer ID format: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                tracing::error!("Peer ID is required for download command");
                std::process::exit(1);
            }
        }

        _ => tracing::error!("Unknown subcommand"),
    }

    let app = Arc::new(Mutex::new(app));
    let app_network = app.clone();
    let app_ui = app.clone();
    let app_ui_refresh = app_ui.clone();

    // Set up refresh channel
    {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut app = app.lock();
        app.refresh_sender = Some(tx);
        drop(app); // Release the lock

        // Spawn a task to handle refresh notifications
        tokio::spawn(async move {
            while let Some(_) = rx.recv().await {
                // Force a UI refresh
                if let Some(tx) = app_ui_refresh.lock().refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
        });
    }

    // Spawn network task
    tokio::spawn(async move {
        if let Err(e) = start_network(app_network, target_peer_addr).await {
            tracing::error!("Network error: {}", e);
            std::process::exit(1);
        }
    });

    // Run UI in main thread
    let mut terminal = setup_terminal();
    render_loop(&mut terminal, app_ui);
    cleanup_terminal();
}

fn setup_panic_handler() {
    setup_panic!(
        Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .authors("Chad Nehemiah <chad@nehemiah94@gmail.com>")
        .homepage("https://maschad.codes")
        .support("- Open a support request via GitHub Issues: https://github.com/maschad/junkanoo/issues")
    );
}

fn setup_logger() {
    // Initialize logging to file and terminal
    let file_appender = rolling::minutely("logs", "p2p-file-share");

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr) // Write to terminal
        .with_writer(file_appender) // Also write to file
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::DEBUG.into())
                .from_env()
                .unwrap(),
        )
        .init();
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

fn render_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: Arc<Mutex<App>>) {
    loop {
        // Check warning timer before rendering
        {
            let mut app = app.lock();
            if let Some(timer) = app.warning_timer {
                if timer.elapsed() >= std::time::Duration::from_secs(2) {
                    app.is_warning = false;
                    app.warning_message = String::new();
                    app.warning_timer = None;
                    // Notify UI to refresh
                    if let Some(refresh_sender) = app.refresh_sender() {
                        let _ = refresh_sender.try_send(());
                    }
                }
            }
        }

        terminal
            .draw(|frame| ui::render(frame, &app.lock()))
            .expect("Failed to draw");

        if poll(std::time::Duration::from_millis(16)).expect("Failed to poll events") {
            if let CrosstermEvent::Key(key) = read().expect("Failed to read event") {
                if key.kind == KeyEventKind::Press {
                    let mut app = app.lock();
                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        KeyCode::Char('x') => {
                            let mut clipboard = Clipboard::new().unwrap();
                            if let Some(addr) = app.listening_addrs.first() {
                                let full_addr = format!("{}/p2p/{}", addr, app.peer_id);
                                if let Err(e) = clipboard.set_text(full_addr) {
                                    tracing::error!("Failed to copy address to clipboard: {}", e);
                                } else {
                                    app.clipboard_success = true;
                                    // Reset clipboard success after 2 seconds
                                    let mut app_clone = app.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(2))
                                            .await;
                                        app_clone.clipboard_success = false;
                                    });
                                }
                            }
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
                                // Check if any files are selected before spawning the task
                                if app.items_to_download.is_empty() {
                                    app.is_warning = true;
                                    app.warning_message = "No files selected for download. Please select files first.".to_string();
                                    // Notify UI to refresh
                                    if let Some(refresh_sender) = app.refresh_sender() {
                                        let _ = refresh_sender.try_send(());
                                    }
                                    // Set a timer to reset the warning
                                    app.warning_timer = Some(std::time::Instant::now());
                                } else {
                                    app.is_loading = true;
                                    // Clone the app before dropping the lock
                                    let mut app_clone = app.clone();
                                    tracing::debug!(
                                        "Starting download with {} items selected",
                                        app.items_to_download.len()
                                    );
                                    // Start the download in a new task
                                    tokio::spawn(async move {
                                        app_clone.start_download().await;
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

async fn start_network(
    app: Arc<Mutex<App>>,
    target_peer_addr: Option<Multiaddr>,
) -> Result<(), &'static str> {
    let (mut client, event_stream, event_loop, peer_id) = service::node::new().await.unwrap();

    // Scope the lock to just this operation
    {
        let mut app = app.lock();
        app.peer_id = peer_id;
        app.set_client(client.clone());
    }

    // Spawn the network event handler
    spawn(event_loop.run());
    spawn(handle_network_events(event_stream, app.clone()));

    client
        .start_listening("/ip4/0.0.0.0/udp/0/quic-v1".parse().unwrap())
        .await
        .expect("Listening not to fail.");

    let listening_addrs: Vec<Multiaddr> = client.get_listening_addrs().await.unwrap();

    // Update listening addresses in a separate lock scope
    {
        let mut app = app.lock();
        app.listening_addrs = listening_addrs;
    }

    // Handle non-host case
    if !app.lock().is_host {
        let target_peer_addr = target_peer_addr.ok_or("No peer address provided")?;

        let target_peer_id = target_peer_addr
            .iter()
            .find_map(|p| match p {
                Protocol::P2p(peer_id) => Some(peer_id),
                _ => None,
            })
            .ok_or("Peer address must contain a peer ID component (/p2p/...)")?;

        client.dial(target_peer_id, target_peer_addr).await.unwrap();

        // Add delay to allow connection to establish
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Request directory
        match client.request_directory(target_peer_id).await {
            Ok(display_response) => {
                let mut app = app.lock();
                app.directory_items = display_response.items;
            }
            Err(e) => {
                tracing::error!("Failed to request directory: {}", e);
                return Err("Failed to request directory");
            }
        }
    } else {
        // Watch for changes to items_to_share and update peer
        loop {
            let directory_items = {
                let app = app.lock();
                app.items_to_share
                    .iter()
                    .enumerate()
                    .map(|(index, path)| DirectoryItem {
                        name: path.file_name().unwrap().to_string_lossy().to_string(),
                        path: path.clone(),
                        is_dir: path.is_dir(),
                        index,
                        depth: 0,
                        selected: true,
                    })
                    .collect()
            };

            client
                .insert_directory_items(peer_id, directory_items)
                .await
                .unwrap();

            // Sleep briefly to avoid tight loop
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    // Keep the network running with minimal lock contention
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

async fn handle_network_events(
    mut event_stream: impl Stream<Item = NetworkEvent> + Unpin,
    app: Arc<Mutex<App>>,
) {
    while let Some(event) = event_stream.next().await {
        match event {
            NetworkEvent::NewListenAddr(addr) => {
                let mut app = app.lock();
                if !app.listening_addrs.contains(&addr) {
                    app.listening_addrs.push(addr);
                    // Notify the UI to refresh
                    if let Some(tx) = app.refresh_sender() {
                        let _ = tx.try_send(());
                    }
                }
            }
            NetworkEvent::PeerConnected(peer_id) => {
                let mut app = app.lock();
                app.connected = true;
                app.connected_peer_id = Some(peer_id);
                // Notify the UI to refresh
                if let Some(tx) = app.refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
            NetworkEvent::PeerDisconnected() => {
                let mut app = app.lock();
                app.connected = false;
                app.connected_peer_id = None; // Clear the connected peer ID
                                              // Notify the UI to refresh
                if let Some(tx) = app.refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
            NetworkEvent::DownloadCompleted(file_names) => {
                tracing::info!("Download completed: {:?}", file_names);
                let mut app = app.lock();
                app.is_loading = false;
                // Notify the UI to refresh
                if let Some(tx) = app.refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
            NetworkEvent::DownloadFailed(file_names) => {
                tracing::error!("Download failed: {:?}", file_names);
                let mut app = app.lock();
                app.is_loading = false;
                // Notify the UI to refresh
                if let Some(tx) = app.refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
        }
    }
}
