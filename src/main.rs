use cli::commands::get_args;

mod app;
mod cli;

fn main() {
    // Initialize logging
    env_logger::init();
    let matches = get_args().get_matches();

    match matches.subcommand() {
        Some(("send", sub_matches)) => {
            println!(
                "Sending file: {}",
                sub_matches
                    .get_one::<String>("file_path")
                    .expect("file path is required")
            );
        }
        Some(("receive", sub_matches)) => {
            println!(
                "Connecting to peer: {}",
                sub_matches
                    .get_one::<String>("peer_identifier")
                    .expect("peer identifier is required")
            );
        }

        _ => println!("Unknown subcommand"),
    }
}
