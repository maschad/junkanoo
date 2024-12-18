use std::path::PathBuf;

pub struct App {
    pub directory_items: Vec<DirectoryItem>,
    pub selected_index: Option<usize>,
    pub current_path: PathBuf,
    pub connected: bool,
    pub peer_id: String,
}

pub struct DirectoryItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub index: usize,
    pub depth: usize,
}

impl App {
    pub fn new() -> Self {
        App {
            directory_items: Vec::new(),
            selected_index: None,
            current_path: PathBuf::from("/"),
            connected: false,
            peer_id: String::new(),
        }
    }

    pub fn select_next(&mut self) {
        self.selected_index = match self.selected_index {
            Some(i) if i < self.directory_items.len() - 1 => Some(i + 1),
            None if !self.directory_items.is_empty() => Some(0),
            _ => self.selected_index,
        };
    }

    pub fn select_previous(&mut self) {
        self.selected_index = match self.selected_index {
            Some(i) if i > 0 => Some(i - 1),
            None if !self.directory_items.is_empty() => Some(0),
            _ => self.selected_index,
        };
    }

    pub fn enter_directory(&mut self) -> bool {
        if let Some(index) = self.selected_index {
            if let Some(item) = self.directory_items.get(index) {
                if item.is_dir {
                    self.current_path = item.path.clone();
                    self.selected_index = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn go_up(&mut self) -> bool {
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.selected_index = None;
            return true;
        }
        false
    }
}

