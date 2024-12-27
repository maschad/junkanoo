use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

pub struct App {
    pub directory_items: Vec<DirectoryItem>,
    pub selected_index: Option<usize>,
    pub current_path: PathBuf,
    pub connected: bool,
    pub peer_id: String,
    pub state: AppState,
    pub is_host: bool,
    pub items_to_share: HashSet<PathBuf>,
    pub items_being_shared: HashSet<PathBuf>,
    pub items_to_download: HashSet<PathBuf>,
    pub items_being_downloaded: HashSet<PathBuf>,
}

pub struct DirectoryItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub index: usize,
    pub depth: usize,
    pub selected: bool,
}

pub enum AppState {
    Share,
    Download,
    Loading,
    Searching,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            directory_items: Vec::new(),
            selected_index: None,
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            connected: false,
            peer_id: String::new(),
            state: AppState::Share,
            is_host: true,
            items_to_share: HashSet::new(),
            items_being_shared: HashSet::new(),
            items_to_download: HashSet::new(),
            items_being_downloaded: HashSet::new(),
        };

        if app.is_host && matches!(app.state, AppState::Share) {
            app.populate_directory_items();
        }

        app
    }

    fn populate_directory_items(&mut self) {
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
            self.current_path = parent.to_path_buf();
            self.selected_index = None;
            self.populate_directory_items();
        }
    }

    pub fn select_item(&mut self) {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get_mut(index) {
                match self.state {
                    AppState::Share => {
                        self.items_to_share.insert(item.path.clone());
                    }
                    AppState::Download => {
                        self.items_to_download.insert(item.path.clone());
                    }
                    _ => {}
                }
                item.selected = true;
            }
        }
    }

    pub fn unselect_item(&mut self) {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get_mut(index) {
                match self.state {
                    AppState::Share => {
                        self.items_to_share.remove(&item.path);
                    }
                    AppState::Download => {
                        self.items_to_download.remove(&item.path);
                    }
                    _ => {}
                }
                item.selected = false;
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
            }
            AppState::Download => {
                self.items_to_download.clear();
                for item in self.directory_items.iter_mut() {
                    item.selected = false;
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
        // TODO: Store files in peer store for remote download
    }

    pub fn start_download(&mut self) {
        if !self.connected {
            panic!("Cannot start downloading - not connected to a peer");
        }
        self.items_being_downloaded = self.items_to_download.clone();
        self.state = AppState::Loading;
        // TODO: Request files from peer store for remote download
    }
}
