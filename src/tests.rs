#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::app::{App, AppState, ConnectionState, DirectoryItem};
    use libp2p::PeerId;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary directory structure for testing
    fn setup_test_directory() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create some test files and directories
        fs::create_dir(dir_path.join("test_dir")).unwrap();
        fs::create_dir(dir_path.join("test_dir/subdir")).unwrap();

        let mut file1 = File::create(dir_path.join("test_file1.txt")).unwrap();
        file1.write_all(b"test content 1").unwrap();

        let mut file2 = File::create(dir_path.join("test_dir/test_file2.txt")).unwrap();
        file2.write_all(b"test content 2").unwrap();

        let mut file3 = File::create(dir_path.join("test_dir/subdir/test_file3.txt")).unwrap();
        file3.write_all(b"test content 3").unwrap();

        temp_dir
    }

    // Helper function to create a new App instance for testing
    fn create_test_app() -> App {
        let mut app = App::new();
        // Reset all state
        app.items_to_share.clear();
        app.items_to_download.clear();
        app.items_being_shared.clear();
        app.items_being_downloaded.clear();
        app.connection_state = ConnectionState::Disconnected;
        app.connected_peer_id = None;
        app.directory_items.clear();
        app.directory_cache.clear();
        app.selected_index = None;
        app.is_loading = false;
        app.clear_warning();
        app.clipboard_success = false;
        app
    }

    #[test]
    fn test_app_initialization() {
        let app = create_test_app();
        assert_eq!(app.state, AppState::Share);
        assert!(app.is_host);
        assert!(!app.is_connected());
        assert!(app.items_to_share.is_empty());
        assert!(app.items_to_download.is_empty());
    }

    #[test]
    fn test_directory_navigation() {
        let temp_dir = setup_test_directory();
        let mut app = create_test_app();
        app.current_path = temp_dir.path().to_path_buf();
        app.populate_directory_items();

        // Test initial state
        assert!(!app.directory_items.is_empty());
        assert_eq!(app.selected_index, Some(0));

        // Test navigation
        app.navigate_next_file();
        assert_eq!(app.selected_index, Some(1));

        app.navigate_previous_file();
        assert_eq!(app.selected_index, Some(0));

        // Find first directory item
        let dir_index = app.directory_items.iter().position(|item| item.is_dir);
        if let Some(index) = dir_index {
            // Select the directory
            app.selected_index = Some(index);
            let initial_count = app.directory_items.len();

            // Enter directory
            assert!(app.enter_directory());

            // Go back up
            app.go_up_previous_directory();
            assert_eq!(app.directory_items.len(), initial_count);
        }
    }

    #[test]
    fn test_file_selection() {
        let temp_dir = setup_test_directory();
        let mut app = create_test_app();
        app.current_path = temp_dir.path().to_path_buf();
        app.populate_directory_items();

        // Test selecting a file
        app.selected_index = Some(0);
        app.select_item();
        assert!(!app.items_to_share.is_empty());
        assert!(app.directory_items[0].selected);

        // Test unselecting
        assert_eq!(app.items_to_share.len(), 4);
        app.unselect_item();
        assert_eq!(app.items_to_share.len(), 3);
        assert!(!app.directory_items[0].selected);

        // Test unselect all
        app.select_item();
        app.navigate_next_file();
        app.select_item();
        assert_eq!(app.items_to_share.len(), 5);
        app.unselect_all();
        assert!(app.items_to_share.is_empty());
    }

    #[test]
    fn test_directory_caching() {
        let temp_dir = setup_test_directory();
        let mut app = create_test_app();
        app.current_path = temp_dir.path().to_path_buf();

        // First population should cache the items
        app.populate_directory_items();
        let initial_items = app.directory_items.clone();

        // Clear items and repopulate
        app.directory_items.clear();
        app.populate_directory_items();

        // Should get cached items
        assert_eq!(app.directory_items, initial_items);
    }

    #[test]
    fn test_directory_sorting() {
        let temp_dir = setup_test_directory();
        let mut app = create_test_app();
        app.current_path = temp_dir.path().to_path_buf();
        app.populate_directory_items();

        // Verify directories come before files
        let mut found_file = false;
        for item in &app.directory_items {
            if !item.is_dir {
                found_file = true;
            } else if found_file {
                panic!("Directories should be sorted before files");
            }
        }
    }

    #[test]
    fn test_warning_system() {
        let mut app = create_test_app();

        // Test warning state
        app.set_warning("Test warning".to_string());

        assert!(app.is_warning());
        assert_eq!(app.warning_message(), "Test warning");
    }

    #[test]
    fn test_network_state_transitions() {
        let mut app = create_test_app();
        let peer_id = PeerId::random();

        // Test initial state
        assert!(!app.is_connected());
        assert!(app.connected_peer_id.is_none());

        // Test connection
        app.connection_state = ConnectionState::Connected;
        app.connected_peer_id = Some(peer_id);
        assert!(app.is_connected());
        assert_eq!(app.connected_peer_id, Some(peer_id));

        // Test disconnection
        app.is_loading = false;
        app.disconnect();
        assert!(!app.is_connected());
        assert!(app.connected_peer_id.is_none());
    }

    #[test]
    fn test_share_mode_selection() {
        let mut app = create_test_app();
        let temp_dir = setup_test_directory();
        app.current_path = temp_dir.path().to_path_buf();
        app.populate_directory_items();

        // Test selecting items in share mode
        app.state = AppState::Share;
        app.select_item();
        assert!(!app.items_to_share.is_empty());
        assert!(app.items_to_download.is_empty());
    }

    #[test]
    fn test_download_mode_selection() {
        let mut app = create_test_app();
        let temp_dir = setup_test_directory();
        app.current_path = temp_dir.path().to_path_buf();
        app.populate_directory_items();

        // Test selecting items in download mode
        app.state = AppState::Download;
        app.select_item();
        assert!(app.items_to_share.is_empty());
        assert!(!app.items_to_download.is_empty());
    }

    #[test]
    fn test_directory_item_creation() {
        let path = PathBuf::from("test/path");
        let item = DirectoryItem {
            name: "test".to_string(),
            path: path.clone(),
            is_dir: true,
            index: 0,
            depth: 0,
            selected: false,
            preview: String::new(),
            display_path: PathBuf::new(),
        };

        assert_eq!(item.name, "test");
        assert_eq!(item.path, path);
        assert!(item.is_dir);
        assert_eq!(item.index, 0);
        assert_eq!(item.depth, 0);
        assert!(!item.selected);
    }
}
