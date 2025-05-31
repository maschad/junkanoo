use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use libp2p::{
    kad,
    multiaddr::{Multiaddr, Protocol},
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, PeerId, StreamProtocol, SwarmBuilder,
};
use libp2p_stream as stream;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map, HashMap},
    error::Error,
    sync::LazyLock,
    time::Duration,
};
use tokio::sync::Semaphore;

use crate::app::DirectoryItem;

use super::utils::{FileReceiver, FileTransfer};
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
static TRANSFER_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Creates the network components, namely:
///
/// - The network client to interact with the network layer from anywhere within your application.
///
/// - The network event stream, e.g. for incoming requests.
///
/// - The network task driving the network itself.
pub fn new() -> Result<(Client, impl Stream<Item = Event>, EventLoop, PeerId), Box<dyn Error>> {
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

    // Set up file transfer protocol listener
    let incoming_streams = swarm
        .behaviour_mut()
        .file_stream
        .new_control()
        .accept(JUNKANOO_FILE_PROTOCOL)
        .unwrap();
    tracing::info!(
        "Listening for file transfer streams on protocol: {}",
        JUNKANOO_FILE_PROTOCOL
    );

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
        EventLoop::new(swarm, command_receiver, event_sender, incoming_streams),
        local_peer_id,
    ))
}

#[derive(Clone)]
pub struct Client {
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

    /// Request files from the given peer.
    pub(crate) async fn request_files(
        &mut self,
        peer_id: PeerId,
        file_names: Vec<String>,
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
        receiver.await.expect("Sender not to be dropped.")
    }
}

// Add these type aliases before the EventLoop struct
type PendingDialSender = oneshot::Sender<Result<(), Box<dyn Error + Send>>>;
type PendingFileSender = oneshot::Sender<Result<Vec<u8>, Box<dyn Error + Send>>>;
type PendingDisplaySender = oneshot::Sender<Result<DisplayResponse, Box<dyn Error + Send>>>;

pub struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: HashMap<PeerId, PendingDialSender>,
    pending_request_file: HashMap<OutboundRequestId, PendingFileSender>,
    pending_request_display: HashMap<OutboundRequestId, PendingDisplaySender>,
    pending_directory_items: HashMap<PeerId, Vec<DirectoryItem>>,
    incoming_streams: stream::IncomingStreams,
}

impl EventLoop {
    fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
        incoming_streams: stream::IncomingStreams,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: HashMap::default(),
            pending_request_file: HashMap::default(),
            pending_request_display: HashMap::default(),
            pending_directory_items: HashMap::default(),
            incoming_streams,
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => self.handle_event(event).await,
                command = self.command_receiver.next() => match command {
                    Some(c) => self.handle_command(c),
                    // Command channel closed, thus shutting down the network event loop.
                    None=>  return,
                },
                stream = self.incoming_streams.next() => {
                    if let Some((peer, mut stream)) = stream {
                        tracing::info!("Received file transfer stream from peer {}", peer);

                        // Spawn a task to handle the file transfer
                        let permit = TRANSFER_SEMAPHORE.acquire().await.unwrap();
                        tokio::spawn(async move {
                            let mut receiver = FileReceiver::new();

                            match receiver.receive_file(&mut stream).await {
                                Ok(file_name) => {
                                    tracing::info!("Successfully received file '{}' from peer {}", file_name, peer);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to receive file from peer {}: {}", peer, e);
                                }
                            }

                            drop(permit);
                        });
                    }
                }
            }
        }
    }

    async fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                tracing::info!("New external address of peer {peer_id}: {address}");
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                let addr_with_peer = address.with(Protocol::P2p(local_peer_id));
                tracing::info!("Local node is listening on {:?}", addr_with_peer);

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
                // Try both maps since we don't know which type of request failed
                if let Some(sender) = self.pending_request_display.remove(&request_id) {
                    let _ = sender.send(Err(Box::new(error)));
                } else if let Some(sender) = self.pending_request_file.remove(&request_id) {
                    let _ = sender.send(Err(Box::new(error)));
                } else {
                    tracing::warn!("Received failure for unknown request ID: {:?}", request_id);
                }
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                tracing::info!("Connected to {peer_id}");

                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
                self.event_sender
                    .send(Event::PeerConnected(peer_id))
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

    #[allow(clippy::too_many_lines)]
    fn handle_command(&mut self, command: Command) {
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

                // Remove the sender from pending_request_file since we're moving it into the task
                self.pending_request_file.remove(&request_id);

                let mut stream_control = self.swarm.behaviour().file_stream.new_control();
                let file_names_clone = file_names;
                let mut event_sender = self.event_sender.clone();

                tokio::spawn(async move {
                    // Open a new stream to the peer instead of waiting for an incoming stream
                    match stream_control
                        .open_stream(peer_id, JUNKANOO_FILE_PROTOCOL)
                        .await
                    {
                        Ok(mut stream) => {
                            let mut transfer = FileTransfer::new(FileMetadata {
                                path: file_names_clone[0].clone(),
                                size: 0,
                                chunks: 0,
                            });
                            match transfer.stream_file(&mut stream).await {
                                Ok(()) => {
                                    event_sender
                                        .send(Event::DownloadCompleted(file_names_clone))
                                        .await
                                        .expect("Event receiver not to be dropped.");
                                    let _ = sender.send(Ok(Vec::new()));
                                }
                                Err(e) => {
                                    tracing::error!("Transfer failed with error: {}", e);
                                    event_sender
                                        .send(Event::DownloadFailed(file_names_clone))
                                        .await
                                        .expect("Event receiver not to be dropped.");
                                    let _ = sender.send(Err(Box::new(e) as Box<dyn Error + Send>));
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to open stream: {}", e);
                            event_sender
                                .send(Event::DownloadFailed(file_names_clone))
                                .await
                                .expect("Event receiver not to be dropped.");
                            let _ = sender.send(Err(Box::new(e) as Box<dyn Error + Send>));
                        }
                    }
                });
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
            Command::GetListeningAddrs { sender } => {
                let _ = sender.send(Ok(self.swarm.listeners().cloned().collect()));
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
    GetListeningAddrs {
        sender: oneshot::Sender<Result<Vec<Multiaddr>, Box<dyn Error + Send>>>,
    },
    RequestDisplay {
        peer_id: PeerId,
        sender: oneshot::Sender<Result<DisplayResponse, Box<dyn Error + Send>>>,
    },
}

#[derive(Debug)]
pub enum Event {
    NewListenAddr(Multiaddr),
    PeerConnected(PeerId),
    PeerDisconnected(),
    DownloadCompleted(Vec<String>),
    DownloadFailed(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum FileResponse {
    DirectoryListing(Vec<DisplayResponse>),
    DownloadInfo(Vec<FileMetadata>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayResponse {
    #[serde(default)]
    pub items: Vec<DirectoryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: String,
    pub size: u64,
    pub chunks: u64,
}

// Simple file exchange protocol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileRequest(Vec<String>);
