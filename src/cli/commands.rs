use clap::{arg, Command};

pub fn get_args() -> Command {
    Command::new("junkanoo")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            arg!(-d --"dht-only" "Use only DHT for peer discovery")
                .id("dht")
                .conflicts_with("mdns"),
        )
        .arg(
            arg!(-m --"mdns-only" "Use only mDNS for peer discovery")
                .id("mdns")
                .conflicts_with("dht"),
        )
        .arg(arg!(-v --debug "Print debug information"))
        .subcommand(
            Command::new("send")
                .about("Send a file or directory to another peer")
                .arg(arg!(<FILE_PATH> "The file path or directory to send"))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("receive")
                .about("Receive a file or directory from another peer")
                .arg(arg!(<PEER_IDENTIFIER> "The peer identifier to connect to"))
                .arg_required_else_help(true),
        )
        .subcommand(Command::new("list-peers").about("List all available peers"))
        .subcommand(Command::new("status").about("Show status of current connections"))
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
            .find(|cmd| cmd.get_name() == "send")
            .unwrap();
        assert!(send.is_arg_required_else_help_set());
        assert_eq!(send.get_arguments().count(), 1);

        // Test receive subcommand
        let receive = app
            .get_subcommands()
            .find(|cmd| cmd.get_name() == "receive")
            .unwrap();
        assert!(receive.is_arg_required_else_help_set());
        assert_eq!(receive.get_arguments().count(), 1);

        // Test list-peers and status subcommands exist
        assert!(app
            .get_subcommands()
            .any(|cmd| cmd.get_name() == "list-peers"));
        assert!(app.get_subcommands().any(|cmd| cmd.get_name() == "status"));
    }

    #[test]
    fn test_debug_flag() {
        let app = get_args();
        let debug = app.get_arguments().find(|arg| arg.get_id() == "debug");
        assert!(debug.is_some());
    }
}
