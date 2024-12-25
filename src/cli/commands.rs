use clap::{arg, Arg, Command};

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
