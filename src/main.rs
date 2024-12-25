use cli::commands::get_args;

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

    // Setup terminal
    let mut terminal = {
        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        ratatui::Terminal::new(backend).expect("Failed to create terminal")
    };

    // Configure terminal
    crossterm::terminal::enable_raw_mode().expect("Failed to enable raw mode");
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )
    .expect("Failed to setup terminal");

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

    // Cleanup terminal
    crossterm::terminal::disable_raw_mode().expect("Failed to disable raw mode");
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )
    .expect("Failed to restore terminal");
    terminal.show_cursor().expect("Failed to show cursor");
}
