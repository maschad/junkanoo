use std::path::PathBuf;
use std::{io::Stdout, sync::Arc};

use app::{App, ConnectionState, DirectoryItem};
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
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use parking_lot::Mutex;
use ratatui::{prelude::CrosstermBackend, Terminal};
use service::node::{Client, Event as NetworkEvent};
use std::io::BufReader;
use std::io::Read;
use tokio::spawn;
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;

mod app;
mod cli;
mod service;
mod tests;

#[tokio::main]
async fn main() {
    setup_panic_handler();

    #[cfg(debug_assertions)]
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
            app.current_path = sub_matches.get_one::<String>("FILE_PATH").map_or_else(
                || std::env::current_dir().unwrap_or_default(),
                std::path::PathBuf::from,
            );
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
    let app_network = Arc::clone(&app);
    let app_ui = Arc::clone(&app);
    let app_ui_refresh = Arc::clone(&app_ui);

    // Set up refresh channel
    {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut app = app.lock();
        app.refresh_sender = Some(tx);
        drop(app); // Release the lock

        // Spawn a task to handle refresh notifications
        tokio::spawn(async move {
            while (rx.recv().await).is_some() {
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
    render_loop(&mut terminal, &app);
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

fn render_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &Arc<Mutex<App>>) {
    loop {
        // Check warning timer before rendering
        {
            let mut app = app.lock();
            if let Some(warning) = &app.warning {
                if warning.timer.elapsed() >= std::time::Duration::from_secs(2) {
                    app.clear_warning();
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
                                    app.set_warning("No files selected for download. Please select files first.".to_string());
                                    // Notify UI to refresh
                                    if let Some(refresh_sender) = app.refresh_sender() {
                                        let _ = refresh_sender.try_send(());
                                    }
                                } else {
                                    app.is_loading = true;
                                    // Clone the app before dropping the lock
                                    let mut app_clone = app.clone();
                                    tracing::debug!(
                                        "Starting download with {:#?} items selected",
                                        app.items_to_download
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

async fn handle_host_mode(client: &mut Client, peer_id: PeerId, app: Arc<Mutex<App>>) {
    loop {
        let directory_items = {
            let app = app.lock();
            let all_paths: Vec<_> = app.items_to_share.iter().cloned().collect();
            drop(app); // Release the lock early

            if all_paths.is_empty() {
                Vec::new()
            } else {
                let mut virtual_root = all_paths[0].clone();
                for path in &all_paths[1..] {
                    virtual_root = virtual_root
                        .ancestors()
                        .find(|ancestor| path.starts_with(ancestor))
                        .unwrap_or(&virtual_root)
                        .to_path_buf();
                }
                tracing::info!("Virtual root path: {:?}", virtual_root);
                all_paths
                    .iter()
                    .enumerate()
                    .map(|(index, path)| {
                        tracing::info!("Processing path: {:?}", path);

                        // Get the absolute path for file operations
                        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
                        tracing::info!("Absolute path: {:?}", abs_path);

                        // Get the name from the path
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        // Get the relative path for UI display
                        let rel_path = path
                            .strip_prefix(&virtual_root)
                            .unwrap_or(path)
                            .to_path_buf();

                        tracing::info!(
                            "Name: {}, Relative path: {:?}, Absolute path: {:?}",
                            name,
                            rel_path,
                            abs_path
                        );

                        let is_dir = path.is_dir();
                        let depth = rel_path.components().count();
                        let preview = if is_dir {
                            format!("Directory: {name}")
                        } else {
                            std::fs::File::open(path).map_or_else(
                                |_| "Unable to read file contents".to_string(),
                                |file| {
                                    let reader = BufReader::new(file);
                                    let mut buffer = String::new();
                                    reader.take(4000).read_to_string(&mut buffer).ok();
                                    buffer.chars().take(1000).collect()
                                },
                            )
                        };
                        let item = DirectoryItem {
                            name,
                            path: abs_path, // Use the absolute path for file operations
                            display_path: rel_path, // Use the relative path for UI display
                            is_dir,
                            index,
                            depth,
                            selected: true,
                            preview,
                        };
                        tracing::info!("Created DirectoryItem: {:?}", item);
                        item
                    })
                    .collect()
            }
        };

        // Only send updates if there are changes
        if let Err(e) = client
            .insert_directory_items(peer_id, directory_items)
            .await
        {
            tracing::error!("Failed to send directory items: {}", e);
            break;
        }

        // Sleep for a shorter duration to be more responsive
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}

async fn handle_download_mode(
    client: &mut Client,
    target_peer_addr: Multiaddr,
    app: Arc<Mutex<App>>,
) -> Result<(), &'static str> {
    let target_peer_id = target_peer_addr
        .iter()
        .find_map(|p| match p {
            Protocol::P2p(peer_id) => Some(peer_id),
            _ => None,
        })
        .ok_or("Peer address must contain a peer ID component (/p2p/...)")?;

    client.dial(target_peer_id, target_peer_addr).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Initial directory request
    match client.request_directory(target_peer_id).await {
        Ok(display_response) => {
            let mut items = display_response.items;
            // For download mode, we need to ensure both paths are properly set
            for item in &mut items {
                tracing::info!("path before setting paths is: {:?}", item.path);
                // Keep the absolute path for file operations
                let abs_path = item.path.clone();
                // Use just the name for display
                let display_path = PathBuf::from(&item.name);
                item.path = abs_path;
                item.display_path = display_path;
                tracing::info!(
                    "Set paths - absolute: {:?}, display: {:?}",
                    item.path,
                    item.display_path
                );
            }
            items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match a.depth.cmp(&b.depth) {
                    std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    other => other,
                },
            });

            {
                let mut app = app.lock();
                app.all_shared_items.clone_from(&items);
                app.directory_items = items;
                app.current_path = PathBuf::new();
                app.populate_directory_items();
                tracing::info!("Initial directory items: {:?}", app.directory_items);
            }

            // Start a background task to handle directory updates
            let mut client_clone = client.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    match client_clone.request_directory(target_peer_id).await {
                        Ok(display_response) => {
                            let mut items = display_response.items;
                            // For download mode, we need to ensure both paths are properly set
                            for item in &mut items {
                                // Keep the absolute path for file operations
                                let abs_path = item.path.clone();
                                // Use just the name for display
                                let display_path = PathBuf::from(&item.name);
                                item.path = abs_path;
                                item.display_path = display_path;
                            }
                            items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                                (true, false) => std::cmp::Ordering::Less,
                                (false, true) => std::cmp::Ordering::Greater,
                                _ => match a.depth.cmp(&b.depth) {
                                    std::cmp::Ordering::Equal => {
                                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                                    }
                                    other => other,
                                },
                            });

                            let mut app = app_clone.lock();
                            if app.all_shared_items != items {
                                app.all_shared_items.clone_from(&items);
                                app.directory_items = items;
                                app.populate_directory_items();
                                tracing::info!(
                                    "Updated directory items: {:?}",
                                    app.directory_items
                                );
                                // Notify UI to refresh
                                if let Some(refresh_sender) = &app.refresh_sender {
                                    let _ = refresh_sender.try_send(());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to request directory: {}", e);
                            break;
                        }
                    }
                }
            });

            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to request directory: {}", e);
            Err("Failed to request directory")
        }
    }
}

async fn start_network(
    app: Arc<Mutex<App>>,
    target_peer_addr: Option<Multiaddr>,
) -> Result<(), &'static str> {
    let (mut client, event_stream, event_loop, peer_id) =
        service::node::new().map_err(|_| "Failed to create node")?;

    {
        let mut app = app.lock();
        app.peer_id = peer_id;
        app.set_client(client.clone());
    }

    spawn(event_loop.run());
    spawn(handle_network_events(event_stream, app.clone()));

    // Get IP and port from command line args, defaulting to 0.0.0.0:0
    let matches = cli::commands::get_args().get_matches();
    let default_ip = "0.0.0.0".to_string();
    let default_port = "0".to_string();
    let ip = matches.get_one::<String>("address").unwrap_or(&default_ip);
    let port = matches.get_one::<String>("port").unwrap_or(&default_port);

    let addr = format!("/ip4/{ip}/udp/{port}/quic-v1")
        .parse()
        .unwrap_or_else(|e| {
            tracing::error!("Failed to parse listening address: {}", e);
            eprintln!("Error: Invalid listening address format. Please check your IP and port.");
            std::process::exit(1);
        });

    client.start_listening(addr).await.unwrap_or_else(|e| {
        tracing::error!("Failed to start listening: {}", e);
        eprintln!(
            "Error: Could not start listening on the specified address. The port might be in use."
        );
        std::process::exit(1);
    });

    let listening_addrs: Vec<Multiaddr> = client.get_listening_addrs().await.unwrap_or_else(|e| {
        tracing::error!("Failed to get listening addresses: {}", e);
        eprintln!("Error: Could not get listening addresses. Please try again.");
        std::process::exit(1);
    });

    {
        let mut app = app.lock();
        app.listening_addrs = listening_addrs;
    }

    if app.lock().is_host {
        handle_host_mode(&mut client, peer_id, app).await;
    } else {
        let target_peer_addr = target_peer_addr.ok_or("No peer address provided")?;
        handle_download_mode(&mut client, target_peer_addr, app).await?;
    }

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
                app.connection_state = ConnectionState::Connected;
                app.connected_peer_id = Some(peer_id);
                // Notify the UI to refresh
                if let Some(tx) = app.refresh_sender() {
                    let _ = tx.try_send(());
                }
            }
            NetworkEvent::PeerDisconnected() => {
                let mut app = app.lock();
                app.connection_state = ConnectionState::Disconnected;
                app.connected_peer_id = None;
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
