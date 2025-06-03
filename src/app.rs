use crate::service::node::Client;
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub struct App {
    pub directory_items: Vec<DirectoryItem>,
    pub all_shared_items: Vec<DirectoryItem>,
    pub directory_cache: HashMap<PathBuf, Vec<DirectoryItem>>,
    pub selected_index: Option<usize>,
    pub current_path: PathBuf,
    pub connection_state: ConnectionState,
    pub peer_id: PeerId,
    pub connected_peer_id: Option<PeerId>,
    pub listening_addrs: Vec<Multiaddr>,
    pub state: AppState,
    pub is_host: bool,
    pub is_loading: bool,
    pub items_to_share: HashSet<PathBuf>,
    pub items_being_shared: HashSet<PathBuf>,
    pub items_to_download: HashSet<PathBuf>,
    pub items_being_downloaded: HashSet<PathBuf>,
    pub warning: Option<Warning>,
    pub refresh_sender: Option<Sender<()>>,
    pub client: Option<Client>,
    pub clipboard_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub index: usize,
    pub depth: usize,
    pub selected: bool,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppState {
    Share,
    Download,
}

#[derive(Clone, Debug)]
pub struct Warning {
    pub message: String,
    pub timer: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            directory_items: Vec::new(),
            all_shared_items: Vec::new(),
            directory_cache: HashMap::new(),
            selected_index: None,
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            connection_state: ConnectionState::Disconnected,
            peer_id: PeerId::random(),
            connected_peer_id: None,
            state: AppState::Share,
            is_host: true,
            is_loading: false,
            listening_addrs: Vec::new(),
            items_to_share: HashSet::new(),
            items_being_shared: HashSet::new(),
            items_to_download: HashSet::new(),
            items_being_downloaded: HashSet::new(),
            warning: None,
            refresh_sender: None,
            client: None,
            clipboard_success: false,
        };

        app.populate_directory_items();
        app
    }

    pub fn set_client(&mut self, client: Client) {
        self.client = Some(client);
    }

    fn handle_download_mode(&mut self) -> bool {
        if self.state == AppState::Download && !self.all_shared_items.is_empty() {
            let current = if self.current_path.as_os_str().is_empty() {
                PathBuf::new()
            } else {
                self.current_path.clone()
            };
            let mut children: Vec<DirectoryItem> = self
                .all_shared_items
                .iter()
                .filter(|item| item.path.parent().unwrap_or(&PathBuf::new()) == current)
                .cloned()
                .collect();
            children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });

            // Update indices for the new items
            for (i, item) in children.iter_mut().enumerate() {
                item.index = i;
            }

            self.directory_items = children;
            if self.directory_items.is_empty() {
                self.selected_index = None;
            } else if self.selected_index.is_none() {
                self.selected_index = Some(0);
            }
            true
        } else {
            false
        }
    }

    fn handle_cached_items(&mut self) -> bool {
        if let Some(cached_items) = self.directory_cache.get(&self.current_path) {
            self.directory_items = cached_items.clone();
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
            }
            true
        } else {
            false
        }
    }

    fn create_directory_item(
        &self,
        path: PathBuf,
        name: String,
        is_dir: bool,
        index: usize,
    ) -> DirectoryItem {
        let selected = match self.state {
            AppState::Share => self.items_to_share.iter().any(|selected_path| {
                let abs_selected = self.current_path.join(selected_path);
                path == abs_selected
            }),
            AppState::Download => self.directory_items.iter().any(|item| {
                item.path
                    .strip_prefix(&self.current_path)
                    .is_ok_and(|rel_path| {
                        let abs_selected = self.current_path.join(rel_path);
                        path == abs_selected
                    })
            }),
        };

        let depth = path
            .strip_prefix(&self.current_path)
            .map(|rel_path| rel_path.components().count())
            .unwrap_or(0);

        let preview = if is_dir {
            format!("Directory: {name}")
        } else {
            std::fs::File::open(&path).map_or_else(
                |_| "Unable to read file contents".to_string(),
                |file| {
                    let reader = BufReader::new(file);
                    let mut buffer = String::new();
                    // Read up to 1000 UTF-8 characters (not bytes)
                    reader
                        .take(4000) // Read up to 4000 bytes, adjust as needed for long UTF-8 chars
                        .read_to_string(&mut buffer)
                        .ok();
                    buffer.chars().take(1000).collect()
                },
            )
        };
        DirectoryItem {
            name,
            path,
            is_dir,
            index,
            depth,
            selected,
            preview,
        }
    }

    pub fn populate_directory_items(&mut self) {
        if self.handle_download_mode() || self.handle_cached_items() {
            return;
        }

        self.directory_items.clear();
        if let Ok(entries) = fs::read_dir(&self.current_path) {
            for (index, entry) in entries.flatten().enumerate() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let is_dir = path.is_dir();

                if self.should_show_item(&path, is_dir) {
                    self.directory_items
                        .push(self.create_directory_item(path, name, is_dir, index));
                }
            }

            self.sort_and_cache_items();
        }
    }

    fn should_show_item(&self, path: &PathBuf, is_dir: bool) -> bool {
        if self.state == AppState::Share && !self.items_to_share.is_empty() {
            if let Some(root_dir) = self.get_root_shared_dir() {
                if self.current_path != root_dir && self.current_path.starts_with(&root_dir) {
                    return if is_dir {
                        self.items_to_share.iter().any(|selected_path| {
                            let abs_selected = self.current_path.join(selected_path);
                            path.starts_with(&abs_selected) || abs_selected.starts_with(path)
                        })
                    } else {
                        let abs_path = path.clone();
                        self.items_to_share.iter().any(|selected_path| {
                            let abs_selected = self.current_path.join(selected_path);
                            abs_path == abs_selected
                        })
                    };
                }
            }
        }
        true
    }

    fn get_root_shared_dir(&self) -> Option<PathBuf> {
        self.items_to_share
            .iter()
            .find(|path| path.is_dir())
            .map(|path| self.current_path.join(path))
    }

    fn sort_and_cache_items(&mut self) {
        self.directory_items
            .sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match a.depth.cmp(&b.depth) {
                    std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    other => other,
                },
            });

        for (i, item) in self.directory_items.iter_mut().enumerate() {
            item.index = i;
        }

        self.directory_cache
            .insert(self.current_path.clone(), self.directory_items.clone());

        if self.selected_index.is_none() && !self.directory_items.is_empty() {
            self.selected_index = Some(0);
        }
    }

    pub const fn navigate_next_file(&mut self) {
        if self.directory_items.is_empty() {
            return;
        }

        self.selected_index = match self.selected_index {
            Some(i) if i < self.directory_items.len() - 1 => Some(i + 1),
            None => Some(0),
            _ => self.selected_index,
        };
    }

    pub const fn navigate_previous_file(&mut self) {
        if self.directory_items.is_empty() {
            return;
        }

        self.selected_index = match self.selected_index {
            Some(i) if i > 0 => Some(i - 1),
            None => Some(self.directory_items.len() - 1),
            _ => self.selected_index,
        };
    }

    pub fn enter_directory(&mut self) -> bool {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get(index) {
                if item.is_dir {
                    self.current_path = item.path.clone();
                    self.selected_index = None;
                    self.populate_directory_items();
                    return true;
                }
            }
        }
        false
    }

    pub fn go_up_previous_directory(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            // Check if we're in a shared directory
            if self.state == AppState::Share {
                // Get the root shared directory (the first selected directory)
                if let Some(root_shared_dir) = self.items_to_share.iter().find(|path| path.is_dir())
                {
                    // Only allow going up if we're not at or below the root shared directory
                    if !self.current_path.starts_with(root_shared_dir) {
                        self.current_path = parent.to_path_buf();
                        self.populate_directory_items();
                    }
                } else {
                    // If no directory is selected, allow normal navigation
                    self.current_path = parent.to_path_buf();
                    self.populate_directory_items();
                }
            } else {
                // In download mode, allow normal navigation
                self.current_path = parent.to_path_buf();
                self.populate_directory_items();
            }
        }
    }

    pub fn select_item(&mut self) {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get_mut(index) {
                match self.state {
                    AppState::Share | AppState::Download => {
                        let items_set = match self.state {
                            AppState::Share => &mut self.items_to_share,
                            AppState::Download => &mut self.items_to_download,
                        };

                        if item.is_dir {
                            // Add the directory itself with its relative path
                            if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                                let path_buf = rel_path.to_path_buf();
                                items_set.insert(path_buf);

                                // Add all files and subdirectories with their relative paths
                                for entry in walkdir::WalkDir::new(&item.path)
                                    .into_iter()
                                    .filter_map(std::result::Result::ok)
                                {
                                    if let Ok(entry_rel_path) =
                                        entry.path().strip_prefix(&self.current_path)
                                    {
                                        items_set.insert(entry_rel_path.to_path_buf());
                                    }
                                }
                            }
                        } else {
                            match self.state {
                                AppState::Share => {
                                    // For share mode, store relative to current directory
                                    if let Ok(rel_path) = item.path.strip_prefix(&self.current_path)
                                    {
                                        items_set.insert(rel_path.to_path_buf());
                                    } else {
                                        items_set.insert(PathBuf::from(&item.name));
                                    }
                                }
                                AppState::Download => {
                                    // For download mode, use the path directly since it's already relative
                                    tracing::info!("inserting item is: {:?}", item);
                                    items_set.insert(item.path.clone());
                                }
                            }
                        }
                        item.selected = true;
                        // Update the cached version
                        if let Some(cached_items) = self.directory_cache.get_mut(&self.current_path)
                        {
                            if let Some(cached_item) = cached_items.get_mut(index) {
                                cached_item.selected = true;
                            }
                        }
                        // Notify UI to refresh
                        if let Some(refresh_sender) = &self.refresh_sender {
                            let _ = refresh_sender.try_send(());
                        }
                    }
                }
            }
        }
    }

    pub fn unselect_item(&mut self) {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get_mut(index) {
                match self.state {
                    AppState::Share => {
                        if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                            let path_buf = rel_path.to_path_buf();
                            self.items_to_share.remove(&path_buf);
                        }
                    }
                    AppState::Download => {
                        if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                            let path_buf = rel_path.to_path_buf();
                            self.items_to_download.remove(&path_buf);
                        } else {
                            // If we can't strip the prefix, try removing by filename
                            let path_buf = PathBuf::from(&item.name);
                            self.items_to_download.remove(&path_buf);
                        }
                    }
                }
                item.selected = false;
                // Update the cached version
                if let Some(cached_items) = self.directory_cache.get_mut(&self.current_path) {
                    if let Some(cached_item) = cached_items.get_mut(index) {
                        cached_item.selected = false;
                    }
                }
                // Notify UI to refresh
                if let Some(refresh_sender) = &self.refresh_sender {
                    let _ = refresh_sender.try_send(());
                }
            }
        }
    }

    pub fn unselect_all(&mut self) {
        match self.state {
            AppState::Share => {
                self.items_to_share.clear();
                for item in &mut self.directory_items {
                    item.selected = false;
                }
                // Update cache
                if let Some(cached_items) = self.directory_cache.get_mut(&self.current_path) {
                    for item in cached_items.iter_mut() {
                        item.selected = false;
                    }
                }
            }
            AppState::Download => {
                self.items_to_download.clear();
                for item in &mut self.directory_items {
                    item.selected = false;
                }
                // Update cache
                if let Some(cached_items) = self.directory_cache.get_mut(&self.current_path) {
                    for item in cached_items.iter_mut() {
                        item.selected = false;
                    }
                }
            }
        }
    }

    pub const fn disconnect(&mut self) {
        if self.is_connected() && !self.is_loading() {
            self.connection_state = ConnectionState::Disconnected;
            self.connected_peer_id = None;
        }
    }

    pub fn start_share(&mut self) {
        assert!(
            self.is_connected(),
            "Cannot start sharing - not connected to a peer"
        );
        self.items_being_shared = self.items_to_share.clone();
    }

    pub async fn start_download(&mut self) {
        if !self.is_connected() {
            tracing::error!("Cannot start downloading - not connected to a peer");
            return;
        }

        let Some(peer_id) = self.connected_peer_id else {
            tracing::error!("No peer ID available for download");
            return;
        };

        tracing::info!("items_to_download are: {:?}", self.items_to_download);

        self.items_being_downloaded
            .clone_from(&self.items_to_download);

        let file_names: Vec<String> = self
            .items_to_download
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        tracing::info!("Starting download of files: {:?}", file_names);

        if let Some(client) = &mut self.client {
            match client.request_files(peer_id, file_names).await {
                Ok(_) => {
                    tracing::info!("Download completed successfully");
                }
                Err(e) => {
                    tracing::error!("Failed to request files: {}", e);
                }
            }
        }

        if let Some(refresh_sender) = &self.refresh_sender {
            let _ = refresh_sender.try_send(());
        }
    }

    pub const fn refresh_sender(&self) -> Option<&Sender<()>> {
        self.refresh_sender.as_ref()
    }

    pub const fn is_connected(&self) -> bool {
        matches!(self.connection_state, ConnectionState::Connected)
    }

    pub const fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub const fn is_warning(&self) -> bool {
        self.warning.is_some()
    }

    pub fn warning_message(&self) -> &str {
        self.warning.as_ref().map_or("", |w| &w.message)
    }

    pub fn set_warning(&mut self, message: String) {
        self.warning = Some(Warning {
            message,
            timer: std::time::Instant::now(),
        });
    }

    pub fn clear_warning(&mut self) {
        self.warning = None;
    }
}
