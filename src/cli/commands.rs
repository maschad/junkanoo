use clap::{arg, Command};

pub fn get_args() -> Command {
    Command::new("junkanoo")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(arg!(-v --debug "Print debug information"))
        .subcommand(
            Command::new("share")
                .about("Send a file or directory to another peer")
                .arg(arg!([FILE_PATH] "The file path or directory to send (defaults to current directory)")),
        )
        .subcommand(
            Command::new("download")
                .about("Receive a file or directory from another peer")
                .arg(arg!(<PEER_ADDR_IDENTIFIER> "The multiaddr to connect to"))
                .arg_required_else_help(true),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_structure() {
        let app = get_args();
        assert_eq!(app.get_name(), "junkanoo");
        assert!(app.is_subcommand_required_set());
        assert!(app.is_arg_required_else_help_set());
    }

    #[test]
    fn test_subcommands() {
        let app = get_args();

        // Test send subcommand
        let send = app
            .get_subcommands()
            .find(|cmd| cmd.get_name() == "share")
            .unwrap();
        assert_eq!(send.get_arguments().count(), 1);

        // Test receive subcommand
        let download = app
            .get_subcommands()
            .find(|cmd| cmd.get_name() == "download")
            .unwrap();
        assert!(download.is_arg_required_else_help_set());
        assert_eq!(download.get_arguments().count(), 1);
    }

    #[test]
    fn test_debug_flag() {
        let app = get_args();
        let debug = app.get_arguments().find(|arg| arg.get_id() == "debug");
        assert!(debug.is_some());
    }
}
