use crate::service::node::Client;
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use walkdir;

#[derive(Clone)]
pub struct App {
    pub directory_items: Vec<DirectoryItem>,
    pub directory_cache: HashMap<PathBuf, Vec<DirectoryItem>>,
    pub selected_index: Option<usize>,
    pub current_path: PathBuf,
    pub connected: bool,
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
    pub clipboard_success: bool,
    pub is_warning: bool,
    pub warning_message: String,
    pub warning_timer: Option<std::time::Instant>,
    pub refresh_sender: Option<Sender<()>>,
    client: Option<Client>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub index: usize,
    pub depth: usize,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppState {
    Share,
    Download,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            directory_items: Vec::new(),
            directory_cache: HashMap::new(),
            selected_index: None,
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            connected: false,
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
            clipboard_success: false,
            is_warning: false,
            warning_message: String::new(),
            warning_timer: None,
            refresh_sender: None,
            client: None,
        };

        // Populate directory items in both share and download modes
        if matches!(app.state, AppState::Share) || matches!(app.state, AppState::Download) {
            app.populate_directory_items();
        }

        app
    }

    pub fn set_client(&mut self, client: Client) {
        self.client = Some(client);
    }

    pub fn populate_directory_items(&mut self) {
        // If in Download mode and directory_items are already set, filter to show only children of current_path
        if self.state == AppState::Download && !self.directory_items.is_empty() {
            let current = if self.current_path.as_os_str().is_empty() {
                PathBuf::new()
            } else {
                self.current_path.clone()
            };
            let mut children: Vec<DirectoryItem> = self
                .directory_items
                .iter()
                .filter(|item| {
                    // The parent of the item's path should match current_path
                    item.path.parent().unwrap_or(&PathBuf::new()) == current
                })
                .cloned()
                .collect();
            // Sort as before
            children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
            self.directory_items = children;
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
            }
            return;
        }

        // Check if we have cached items for this directory
        if let Some(cached_items) = self.directory_cache.get(&self.current_path) {
            self.directory_items = cached_items.clone();
            // Initialize selected_index if it's None
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
            }
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

                // In share mode, only filter items if we have selected items and we're in a subdirectory
                if self.state == AppState::Share && !self.items_to_share.is_empty() {
                    // Get the root shared directory (the first selected directory)
                    let root_shared_dir = self
                        .items_to_share
                        .iter()
                        .filter(|path| path.is_dir())
                        .next()
                        .map(|path| self.current_path.join(path));

                    // Only filter if we're in a subdirectory of the root shared directory
                    if let Some(root_dir) = root_shared_dir {
                        if self.current_path != root_dir && self.current_path.starts_with(&root_dir)
                        {
                            let should_show = if is_dir {
                                // Show directory if it contains any selected items
                                self.items_to_share.iter().any(|selected_path| {
                                    let abs_selected = self.current_path.join(selected_path);
                                    path.starts_with(&abs_selected)
                                        || abs_selected.starts_with(&path)
                                })
                            } else {
                                // Show file if it's selected
                                let abs_path = path.clone();
                                self.items_to_share.iter().any(|selected_path| {
                                    let abs_selected = self.current_path.join(selected_path);
                                    abs_path == abs_selected
                                })
                            };

                            if !should_show {
                                continue;
                            }
                        }
                    }
                }

                let selected = match self.state {
                    AppState::Share => {
                        // Check if this file/directory is selected using absolute paths
                        let abs_path = path.clone();
                        self.items_to_share.iter().any(|selected_path| {
                            let abs_selected = self.current_path.join(selected_path);
                            abs_path == abs_selected
                        })
                    }
                    AppState::Download => {
                        // In download mode, check if the file/directory is in the shared items
                        let abs_path = path.clone();
                        self.directory_items.iter().any(|item| {
                            if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                                let abs_selected = self.current_path.join(rel_path);
                                abs_path == abs_selected
                            } else {
                                false
                            }
                        })
                    }
                };

                // Calculate the depth of the item relative to the current path
                let depth = if let Ok(rel_path) = path.strip_prefix(&self.current_path) {
                    rel_path.components().count()
                } else {
                    0
                };

                self.directory_items.push(DirectoryItem {
                    name,
                    path,
                    is_dir,
                    index,
                    depth,
                    selected,
                });
            }

            // Sort directories first, then files, and maintain directory structure
            self.directory_items.sort_by(|a, b| {
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => {
                        // If both are directories or both are files, sort by depth first
                        match a.depth.cmp(&b.depth) {
                            std::cmp::Ordering::Equal => {
                                // If same depth, sort alphabetically
                                a.name.to_lowercase().cmp(&b.name.to_lowercase())
                            }
                            other => other,
                        }
                    }
                }
            });

            // Update indices after sorting
            for (i, item) in self.directory_items.iter_mut().enumerate() {
                item.index = i;
            }

            // Cache the items
            self.directory_cache
                .insert(self.current_path.clone(), self.directory_items.clone());

            // Initialize selected_index if it's None
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
            }
        }
    }

    pub fn navigate_next_file(&mut self) {
        if self.directory_items.is_empty() {
            return;
        }

        self.selected_index = match self.selected_index {
            Some(i) if i < self.directory_items.len() - 1 => Some(i + 1),
            None => Some(0),
            _ => self.selected_index,
        };
    }

    pub fn navigate_previous_file(&mut self) {
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
                if let Some(root_shared_dir) = self
                    .items_to_share
                    .iter()
                    .filter(|path| path.is_dir())
                    .next()
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
                                items_set.insert(path_buf.clone());

                                // Add all files and subdirectories with their relative paths
                                for entry in walkdir::WalkDir::new(&item.path)
                                    .into_iter()
                                    .filter_map(|e| e.ok())
                                {
                                    if let Ok(entry_rel_path) =
                                        entry.path().strip_prefix(&self.current_path)
                                    {
                                        let path_buf = entry_rel_path.to_path_buf();
                                        items_set.insert(path_buf.clone());
                                    }
                                }
                            }
                        } else {
                            // For single files, store relative to current directory
                            if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                                let path_buf = rel_path.to_path_buf();
                                items_set.insert(path_buf.clone());
                            } else {
                                // If we can't strip the prefix, just use the filename
                                let path_buf = PathBuf::from(&item.name);
                                items_set.insert(path_buf.clone());
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
                for item in self.directory_items.iter_mut() {
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
                for item in self.directory_items.iter_mut() {
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

    pub fn disconnect(&mut self) {
        if self.connected && !self.is_loading {
            self.connected = false;
            self.connected_peer_id = None;
        }
    }

    pub fn start_share(&mut self) {
        if !self.connected {
            panic!("Cannot start sharing - not connected to a peer");
        }
        self.items_being_shared = self.items_to_share.clone();
    }

    pub async fn start_download(&mut self) {
        if !self.connected {
            tracing::error!("Cannot start downloading - not connected to a peer");
            return;
        }

        let peer_id = match self.connected_peer_id {
            Some(id) => id,
            None => {
                tracing::error!("No peer ID available for download");
                return;
            }
        };

        self.items_being_downloaded = self.items_to_download.clone();

        // Get the list of files to download
        let file_names: Vec<String> = self
            .items_to_download
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        tracing::info!("Starting download of files: {:?}", file_names);

        // Request files from peer
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

        // Notify UI to refresh
        if let Some(refresh_sender) = &self.refresh_sender {
            let _ = refresh_sender.try_send(());
        }
    }

    pub fn refresh_sender(&self) -> Option<&Sender<()>> {
        self.refresh_sender.as_ref()
    }
}
