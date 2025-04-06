use async_std::io;
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use libp2p::{
    kad,
    multiaddr::{Multiaddr, Protocol},
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, PeerId, StreamProtocol, SwarmBuilder,
};
use libp2p_stream as stream;
use once_cell::sync::Lazy;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map, HashMap},
    error::Error,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::Semaphore};

use crate::app::DirectoryItem;

use super::utils::FileTransfer;
// 10 minutes
const CONNECTION_TIMEOUT: u64 = 600;

// Amino Bootnode https://docs.ipfs.tech/concepts/public-utilities/#amino-dht-bootstrappers
const BOOTNODES: [&str; 5] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    "12D3KooWKnDdG3iXw9eTFijk3EWSunZcFi54Zka4wmtqtt6rPxc",
];

const JUNKANOO_REQUEST_RESPONSE_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/junkanoo/request-response");

const JUNKANOO_FILE_PROTOCOL: StreamProtocol = StreamProtocol::new("/junkanoo/stream");

// Limit concurrent transfers to prevent resource exhaustion
// This can be tuned based on system capabilities and requirements
static TRANSFER_SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(4));

/// Creates the network components, namely:
///
/// - The network client to interact with the network layer from anywhere within your application.
///
/// - The network event stream, e.g. for incoming requests.
///
/// - The network task driving the network itself.
pub(crate) async fn new(
) -> Result<(Client, impl Stream<Item = Event>, EventLoop, PeerId), Box<dyn Error>> {
    // Create a public/private key pair, either random or based on a seed.
    // let id_keys = Keypair::generate_ed25519();
    // let peer_id = id_keys.public().to_peer_id();
    let peer_id = PeerId::random();

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| Behaviour {
            kademlia: kad::Behaviour::new(
                peer_id,
                kad::store::MemoryStore::new(key.public().to_peer_id()),
            ),
            request_response: request_response::cbor::Behaviour::new(
                [(JUNKANOO_REQUEST_RESPONSE_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
            file_stream: stream::Behaviour::new(),
        })?
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(Duration::from_secs(CONNECTION_TIMEOUT))
        })
        .build();

    // Set Kademlia into server mode before adding bootnodes
    swarm
        .behaviour_mut()
        .kademlia
        .set_mode(Some(kad::Mode::Server));

    // Then add the bootnodes
    for peer in &BOOTNODES {
        if let Ok(peer_id) = peer.parse() {
            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, "/dnsaddr/bootstrap.libp2p.io".parse()?);
        }
    }

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);

    let local_peer_id = *swarm.local_peer_id();

    Ok((
        Client {
            sender: command_sender,
        },
        event_receiver,
        EventLoop::new(swarm, command_receiver, event_sender),
        local_peer_id,
    ))
}

#[derive(Clone)]
pub(crate) struct Client {
    sender: mpsc::Sender<Command>,
}

impl Client {
    /// Listen for incoming connections on the given address.
    pub(crate) async fn start_listening(
        &mut self,
        addr: Multiaddr,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::StartListening { addr, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn get_listening_addrs(
        &mut self,
    ) -> Result<Vec<Multiaddr>, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::GetListeningAddrs { sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    async fn send_files(&self, mut stream: libp2p::Stream) -> io::Result<()> {
        let num_bytes = rand::random::<usize>() % 1000;
        let mut bytes = vec![0; num_bytes];
        rand::thread_rng().fill_bytes(&mut bytes);

        stream.write_all(&bytes).await?;

        let mut buf = vec![0; num_bytes];
        stream.read_exact(&mut buf).await?;

        if bytes != buf {
            return Err(io::Error::new(io::ErrorKind::Other, "incorrect echo"));
        }

        stream.close().await?;

        Ok(())
    }

    /// Dial the given peer at the given address.
    pub(crate) async fn dial(
        &mut self,
        peer_id: PeerId,
        peer_addr: Multiaddr,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Dial {
                peer_id,
                peer_addr,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    /// Request the directory items from the given peer.
    pub(crate) async fn request_directory(
        &mut self,
        peer_id: PeerId,
    ) -> Result<DisplayResponse, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RequestDisplay { peer_id, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    /// Request the content of the given file from the given peer.
    pub(crate) async fn request_file(
        &mut self,
        file_names: Vec<String>,
        peer_id: PeerId,
    ) -> Result<Vec<u8>, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RequestFiles {
                peer_id,
                file_names,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not be dropped.")
    }

    /// Respond with the provided file content to the given request.
    pub(crate) async fn respond_file(
        &mut self,
        file_metadata: Vec<FileMetadata>,
        channel: ResponseChannel<FileResponse>,
    ) {
        self.sender
            .send(Command::RespondFiles {
                file_metadata,
                channel,
            })
            .await
            .expect("Command receiver not to be dropped.");
    }

    /// Send the current directory items to the given peer.
    pub(crate) async fn insert_directory_items(
        &mut self,
        peer_id: PeerId,
        directory_items: Vec<DirectoryItem>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::InsertDirectoryItems {
                peer_id,
                directory_items,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }
}

pub(crate) struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    pending_request_file:
        HashMap<OutboundRequestId, oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>>,
    pending_request_display:
        HashMap<OutboundRequestId, oneshot::Sender<Result<DisplayResponse, Box<dyn Error + Send>>>>,
    pending_directory_items: HashMap<PeerId, Vec<DirectoryItem>>,
}

impl EventLoop {
    fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            pending_request_file: Default::default(),
            pending_request_display: Default::default(),
            pending_directory_items: Default::default(),
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => self.handle_event(event).await,
                command = self.command_receiver.next() => match command {
                    Some(c) => self.handle_command(c).await,
                    // Command channel closed, thus shutting down the network event loop.
                    None=>  return,
                },
            }
        }
    }

    async fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                tracing::debug!("New external address of peer {peer_id}: {address}");
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                let addr_with_peer = address.with(Protocol::P2p(local_peer_id));
                tracing::debug!("Local node is listening on {:?}", addr_with_peer);

                self.event_sender
                    .send(Event::NewListenAddr(addr_with_peer))
                    .await
                    .expect("Event receiver not to be dropped.");
            }
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::Message { message, .. },
            )) => match message {
                request_response::Message::Request { channel, .. } => {
                    // When receiving a directory request, respond with items from pending_directory_items
                    let items = self
                        .pending_directory_items
                        .get(self.swarm.local_peer_id())
                        .cloned()
                        .unwrap_or_default();

                    let response = DisplayResponse { items };

                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, response)
                        .expect("Response channel to be valid");
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    if let Some(sender) = self.pending_request_display.remove(&request_id) {
                        let _ = sender.send(Ok(response));
                    }
                }
            },
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::OutboundFailure {
                    request_id, error, ..
                },
            )) => {
                let _ = self
                    .pending_request_display
                    .remove(&request_id)
                    .expect("Request to still be pending.")
                    .send(Err(Box::new(error)));
            }
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::ResponseSent { .. },
            )) => {}
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::InboundFailure {
                    peer,
                    request_id,
                    error,
                },
            )) => {
                tracing::debug!("Inbound failure: {peer} {request_id} {error}");
            }
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                tracing::debug!("Connected to {peer_id}");

                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
                self.event_sender
                    .send(Event::PeerConnected())
                    .await
                    .expect("Event receiver not to be dropped.");
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                num_established,
                ..
            } => {
                tracing::debug!("Connection closed: {peer_id} {connection_id} {num_established}");
                self.event_sender
                    .send(Event::PeerDisconnected())
                    .await
                    .expect("Event receiver not to be dropped.");
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Err(Box::new(error)));
                    }
                }
            }
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::Dialing {
                peer_id: Some(peer_id),
                ..
            } => tracing::debug!("Dialing {peer_id}"),
            e => tracing::debug!("{e:?}"),
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::StartListening { addr, sender } => {
                let _ = match self.swarm.listen_on(addr) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::Dial {
                peer_id,
                peer_addr,
                sender,
            } => {
                if let hash_map::Entry::Vacant(e) = self.pending_dial.entry(peer_id) {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, peer_addr.clone());
                    match self.swarm.dial(peer_addr) {
                        Ok(()) => {
                            e.insert(sender);
                        }
                        Err(e) => {
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                } else {
                    todo!("Already dialing peer.");
                }
            }
            Command::RequestFiles {
                peer_id,
                file_names,
                sender,
            } => {
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, DisplayRequest);

                // Store the sender to respond to later
                self.pending_request_file.insert(request_id, sender);

                // Set up a stream to receive the files
                let mut stream_control = self.swarm.behaviour().file_stream.new_control();

                // Spawn a task to handle incoming file streams
                let current_dir =
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let file_names_clone = file_names.clone();

                tokio::spawn(async move {
                    let mut incoming_streams =
                        stream_control.accept(JUNKANOO_FILE_PROTOCOL).unwrap();

                    while let Some((_, mut stream)) = incoming_streams.next().await {
                        // Create a directory structure based on the file path
                        for file_name in &file_names_clone {
                            let file_path = current_dir.join(file_name);

                            // Create parent directories if they don't exist
                            if let Some(parent) = file_path.parent() {
                                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                                    tracing::error!(
                                        "Failed to create directory {:?}: {}",
                                        parent,
                                        e
                                    );
                                    continue;
                                }
                            }

                            match tokio::fs::File::create(&file_path).await {
                                Ok(mut file) => {
                                    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
                                    loop {
                                        match stream.read(&mut buffer).await {
                                            Ok(0) => break, // EOF
                                            Ok(n) => {
                                                if let Err(e) = file.write_all(&buffer[..n]).await {
                                                    tracing::error!(
                                                        "Failed to write to file {:?}: {}",
                                                        file_path,
                                                        e
                                                    );
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "Failed to read from stream: {}",
                                                    e
                                                );
                                                break;
                                            }
                                        }
                                    }
                                    tracing::info!("Downloaded file: {:?}", file_path);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to create file {:?}: {}", file_path, e);
                                }
                            }
                        }
                    }
                });
            }
            Command::RespondFiles { file_metadata, .. } => {
                let mut stream_control = self.swarm.behaviour().file_stream.new_control();

                for metadata in file_metadata {
                    let mut incoming_streams =
                        stream_control.accept(JUNKANOO_FILE_PROTOCOL).unwrap();
                    let mut transfer = FileTransfer::new(metadata);

                    // Use a bounded semaphore to limit concurrent transfers
                    let permit = TRANSFER_SEMAPHORE.acquire().await.unwrap();
                    tokio::spawn(async move {
                        while let Some((peer, mut stream)) = incoming_streams.next().await {
                            if let Err(e) = transfer.stream_file(&mut stream).await {
                                tracing::error!("Transfer failed to peer {peer} with error: {}", e);
                            }
                        }
                        drop(permit); // Release the semaphore
                    });
                }
            }
            Command::GetListeningAddrs { sender } => {
                let _ = sender.send(Ok(self.swarm.listeners().cloned().collect()));
            }
            Command::InsertDirectoryItems {
                peer_id,
                directory_items,
                sender,
            } => {
                self.pending_directory_items
                    .insert(peer_id, directory_items);
                let _ = sender.send(Ok(()));
            }
            Command::RequestDisplay { peer_id, sender } => {
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, DisplayRequest);
                self.pending_request_display.insert(request_id, sender);
            }
            Command::RespondDisplay { channel } => {
                let response = DisplayResponse {
                    items: self
                        .pending_directory_items
                        .remove(&self.swarm.local_peer_id())
                        .unwrap_or_default(),
                };
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
                    .expect("Response channel to be valid");
            }
        }
    }
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    request_response: request_response::cbor::Behaviour<DisplayRequest, DisplayResponse>,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    file_stream: stream::Behaviour,
}

#[derive(Debug)]
enum Command {
    StartListening {
        addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    Dial {
        peer_id: PeerId,
        peer_addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    InsertDirectoryItems {
        peer_id: PeerId,
        directory_items: Vec<DirectoryItem>,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    RequestFiles {
        peer_id: PeerId,
        file_names: Vec<String>,
        sender: oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>,
    },
    RespondFiles {
        file_metadata: Vec<FileMetadata>,
        channel: ResponseChannel<FileResponse>,
    },
    GetListeningAddrs {
        sender: oneshot::Sender<Result<Vec<Multiaddr>, Box<dyn Error + Send>>>,
    },
    RequestDisplay {
        peer_id: PeerId,
        sender: oneshot::Sender<Result<DisplayResponse, Box<dyn Error + Send>>>,
    },
    RespondDisplay {
        channel: ResponseChannel<DisplayResponse>,
    },
}

#[derive(Debug)]
pub(crate) enum Event {
    InboundRequest {
        request: DisplayRequest,
        channel: ResponseChannel<DisplayResponse>,
    },
    NewListenAddr(Multiaddr),
    PeerConnected(),
    PeerDisconnected(),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DisplayRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum FileResponse {
    DirectoryListing(Vec<DisplayResponse>),
    DownloadInfo(Vec<FileMetadata>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DisplayResponse {
    #[serde(default)]
    pub items: Vec<DirectoryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileMetadata {
    pub path: String,
    pub size: u64,
    pub chunks: u64,
}

// Simple file exchange protocol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileRequest(Vec<String>);
