use crate::service::node::{Client, FileMetadata};
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
    pub listening_addrs: Vec<Multiaddr>,
    pub state: AppState,
    pub is_host: bool,
    pub items_to_share: HashSet<PathBuf>,
    pub items_being_shared: HashSet<PathBuf>,
    pub items_to_download: HashSet<PathBuf>,
    pub items_being_downloaded: HashSet<PathBuf>,
    pub clipboard_success: bool,
    refresh_sender: Option<Sender<()>>,
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

#[derive(Clone)]
pub enum AppState {
    Share,
    Download,
    Loading,
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
            state: AppState::Share,
            is_host: true,
            listening_addrs: Vec::new(),
            items_to_share: HashSet::new(),
            items_being_shared: HashSet::new(),
            items_to_download: HashSet::new(),
            items_being_downloaded: HashSet::new(),
            clipboard_success: false,
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
        tracing::debug!("Populating directory items");
        // Check if we have cached items for this directory
        if let Some(cached_items) = self.directory_cache.get(&self.current_path) {
            self.directory_items = cached_items.clone();
            // Initialize selected_index if it's None
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
                tracing::debug!("Initialized selected_index to 0");
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
                let selected = match self.state {
                    AppState::Share => self.items_to_share.contains(&path),
                    AppState::Download => self.items_to_download.contains(&path),
                    _ => false,
                };

                self.directory_items.push(DirectoryItem {
                    name,
                    path,
                    is_dir,
                    index,
                    depth: 0,
                    selected,
                });
            }

            // Sort directories first, then files
            self.directory_items
                .sort_by(|a, b| match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                });

            // Update indices after sorting
            for (i, item) in self.directory_items.iter_mut().enumerate() {
                item.index = i;
            }

            // Initialize selected_index if it's None
            if self.selected_index.is_none() && !self.directory_items.is_empty() {
                self.selected_index = Some(0);
                tracing::debug!("Initialized selected_index to 0");
            }

            // Cache the items for this directory
            self.directory_cache
                .insert(self.current_path.clone(), self.directory_items.clone());
        }
        tracing::debug!(
            "Directory items populated, selected_index: {:?}",
            self.selected_index
        );
    }

    pub fn navigate_next_file(&mut self) {
        if self.directory_items.is_empty() {
            return;
        }

        tracing::debug!("Navigating to next file");
        self.selected_index = match self.selected_index {
            Some(i) if i < self.directory_items.len() - 1 => Some(i + 1),
            None => Some(0),
            _ => self.selected_index,
        };
        tracing::debug!("Selected index is now: {:?}", self.selected_index);
    }

    pub fn navigate_previous_file(&mut self) {
        if self.directory_items.is_empty() {
            return;
        }

        tracing::debug!("Navigating to previous file");
        self.selected_index = match self.selected_index {
            Some(i) if i > 0 => Some(i - 1),
            None => Some(self.directory_items.len() - 1),
            _ => self.selected_index,
        };
        tracing::debug!("Selected index is now: {:?}", self.selected_index);
    }

    pub fn enter_directory(&mut self) -> bool {
        tracing::debug!("Entering directory");
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get(index) {
                if item.is_dir {
                    self.current_path = item.path.clone();
                    self.selected_index = None;
                    self.populate_directory_items();
                    tracing::debug!("Entered directory: {}", self.current_path.display());
                    return true;
                }
            }
        }
        false
    }

    pub fn go_up_previous_directory(&mut self) {
        tracing::debug!("Going up to previous directory");
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.selected_index = None;
            self.populate_directory_items();
            tracing::debug!("Went up to directory: {}", self.current_path.display());
        }
    }

    pub fn select_item(&mut self) {
        tracing::debug!("select_item called");
        if let Some(index) = self.selected_index {
            tracing::debug!("Selected index: {}", index);
            if let Some(item) = self.directory_items.get_mut(index) {
                tracing::debug!("Found item: {}", item.name);
                match self.state {
                    AppState::Share | AppState::Download => {
                        tracing::debug!("In Share/Download state");
                        let items_set = match self.state {
                            AppState::Share => &mut self.items_to_share,
                            AppState::Download => &mut self.items_to_download,
                            _ => unreachable!(),
                        };

                        if item.is_dir {
                            tracing::debug!("Item is directory");
                            // Add the directory itself with its relative path
                            if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                                let path_buf = rel_path.to_path_buf();
                                items_set.insert(path_buf.clone());
                                tracing::debug!("Added directory to selection: {:?}", path_buf);

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
                                        tracing::debug!(
                                            "Added sub-item to selection: {:?}",
                                            path_buf
                                        );
                                    }
                                }
                            }
                        } else {
                            tracing::debug!("Item is file");
                            // For single files, store relative to current directory
                            if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                                let path_buf = rel_path.to_path_buf();
                                items_set.insert(path_buf.clone());
                                tracing::debug!("Added file to selection: {:?}", path_buf);
                            } else {
                                // If we can't strip the prefix, just use the filename
                                let path_buf = PathBuf::from(&item.name);
                                items_set.insert(path_buf.clone());
                                tracing::debug!(
                                    "Added file to selection using filename: {:?}",
                                    path_buf
                                );
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
                            tracing::debug!("Selecting item");
                            let _ = refresh_sender.try_send(());
                        }
                        // Log the current state of selections
                        tracing::debug!("Current selections: {:?}", items_set);
                    }
                    _ => {
                        tracing::debug!("Not in Share/Download state");
                    }
                }
            } else {
                tracing::debug!("No item found at index {}", index);
            }
        } else {
            tracing::debug!("No selected index");
        }
    }

    pub fn unselect_item(&mut self) {
        tracing::debug!("unselect_item called");
        if let Some(index) = self.selected_index {
            tracing::debug!("Selected index: {}", index);
            if let Some(item) = self.directory_items.get_mut(index) {
                tracing::debug!("Found item: {}", item.name);
                match self.state {
                    AppState::Share => {
                        tracing::debug!("In Share state");
                        if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                            let path_buf = rel_path.to_path_buf();
                            self.items_to_share.remove(&path_buf);
                            tracing::debug!("Removed from share selection: {:?}", path_buf);
                        }
                    }
                    AppState::Download => {
                        tracing::debug!("In Download state");
                        if let Ok(rel_path) = item.path.strip_prefix(&self.current_path) {
                            let path_buf = rel_path.to_path_buf();
                            self.items_to_download.remove(&path_buf);
                            tracing::debug!("Removed from download selection: {:?}", path_buf);
                        } else {
                            // If we can't strip the prefix, try removing by filename
                            let path_buf = PathBuf::from(&item.name);
                            self.items_to_download.remove(&path_buf);
                            tracing::debug!(
                                "Removed from download selection using filename: {:?}",
                                path_buf
                            );
                        }
                    }
                    _ => {
                        tracing::debug!("Not in Share/Download state");
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
                    tracing::debug!("Unselecting item");
                    let _ = refresh_sender.try_send(());
                }
                // Log the current state of selections
                match self.state {
                    AppState::Share => {
                        tracing::debug!("Current share selections: {:?}", self.items_to_share)
                    }
                    AppState::Download => {
                        tracing::debug!("Current download selections: {:?}", self.items_to_download)
                    }
                    _ => {}
                }
            } else {
                tracing::debug!("No item found at index {}", index);
            }
        } else {
            tracing::debug!("No selected index");
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
            _ => {}
        }
    }

    pub fn disconnect(&mut self) {
        if self.connected && !matches!(self.state, AppState::Loading) {
            self.connected = false;
        }
    }

    pub fn start_share(&mut self) {
        if !self.connected {
            panic!("Cannot start sharing - not connected to a peer");
        }
        self.items_being_shared = self.items_to_share.clone();
        self.state = AppState::Loading;
    }

    pub async fn start_download(&mut self) {
        if !self.connected {
            panic!("Cannot start downloading - not connected to a peer");
        }

        // Check if any files are selected
        if self.items_to_download.is_empty() {
            tracing::warn!("No files selected for download");
            return;
        }

        tracing::debug!(
            "Starting download with {} items",
            self.items_to_download.len()
        );
        self.items_being_downloaded = self.items_to_download.clone();
        self.state = AppState::Loading;

        // Get the list of files to download
        let file_names: Vec<String> = self
            .items_to_download
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();

        tracing::info!("Starting download of files: {:?}", file_names);

        // Request files from peer
        if let Some(client) = &mut self.client {
            match client.request_files(self.peer_id, file_names).await {
                Ok(_) => {
                    tracing::info!("Download completed successfully");
                    self.state = AppState::Download;
                }
                Err(e) => {
                    tracing::error!("Failed to request files: {}", e);
                    self.state = AppState::Download;
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
