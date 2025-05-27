# `junkanoo` -

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg)](https://github.com/RichardLitt/standard-readme)
[![Go Report Card](https://goreportcard.com/badge/github.com/dennis-tra/pcp)](https://goreportcard.com/report/github.com/dennis-tra/pcp)
[![Maintainability](https://api.codeclimate.com/v1/badges/de64b09a3731b8a8842b/maintainability)](https://codeclimate.com/github/dennis-tra/pcp/maintainability)
[![Latest test suite run result](https://github.com/dennis-tra/pcp/actions/workflows/tests.yml/badge.svg)](https://github.com/dennis-tra/pcp/actions)
[![Github Releases Download Count](https://img.shields.io/github/downloads/dennis-tra/pcp/total.svg)]()

A decentralized ephemeral file sharing CLI browser built with Rust and libp2p.

## Overview

I had started building around the Christmas holidays, when [Junakoo](https://en.wikipedia.org/wiki/Junkanoo) is observed. In a sense it's a practice that helps us to share our secrets as a culture in a non-obvious way.

Junkanoo enables secure, peer-to-peer file sharing through an encrypted channel. It provides a command-line interface for browsing and transferring files between connected nodes.

## Features

- 🔒 Encrypted file transfers using libp2p
- 📁 File browsing and selection interface
- 🚀 Fast file transfers with chunked streaming
- 🔄 Real-time progress tracking
- 🎯 Simple peer-to-peer connection model

## Installation

### Using Homebrew (macOS)

```bash
brew install junkanoo
```

### Using Cargo (Rust)

```bash
cargo install junkanoo
```

### Using AUR (Arch Linux)

```bash
yay -S junkanoo
# or
paru -S junkanoo
```

### Using Nix

```bash
nix-env -iA nixpkgs.junkanoo
```

### Building from Source

1. Clone the repository:
```bash
git clone https://github.com/yourusername/junkanoo.git
cd junkanoo
```

2. Build the project:
```bash
cargo build --release
```

3. Install the binary (optional):
```bash
cargo install --path .
```

## Usage

```bash
# To start sharing files
junkanoo share

# To start downloading files
junkanoo download -- <peer-id>
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

[rust-libp2p](https://github.com/libp2p/rust-libp2p) - The Rust implementation of the libp2p Networking Stack.
[pcp](https://github.com/dennis-tra/pcp) - A command line peer-to-peer data transfer tool based on libp2p.
[croc](https://github.com/schollz/croc) -  A tool that allows any two computers to simply and securely transfer files and folders.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.