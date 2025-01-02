use async_std::io;
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use libp2p::{
    identity::Keypair,
    kad,
    multiaddr::{Multiaddr, Protocol},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    PeerId, StreamProtocol, SwarmBuilder,
};
use libp2p_stream as stream;
use rand::{thread_rng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    time::Duration,
};

const BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

const JUNKANOO_PROTOCOL: StreamProtocol = StreamProtocol::new("/junkanoo");

/// Creates the network components, namely:
///
/// - The network client to interact with the network layer from anywhere within your application.
///
/// - The network event stream, e.g. for incoming requests.
///
/// - The network task driving the network itself.
pub(crate) async fn new() -> Result<
    (
        Client,
        impl Stream<Item = Event>,
        EventLoop,
        Vec<Multiaddr>,
        PeerId,
    ),
    Box<dyn Error>,
> {
    // Create a public/private key pair, either random or based on a seed.
    let id_keys = Keypair::generate_ed25519();
    let peer_id = id_keys.public().to_peer_id();

    let mut swarm = SwarmBuilder::with_existing_identity(id_keys)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| Behaviour {
            kademlia: kad::Behaviour::new(
                peer_id,
                kad::store::MemoryStore::new(key.public().to_peer_id()),
            ),
            stream: stream::Behaviour::new(),
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(10)))
        .build();

    // Add the bootnodes to the local routing table. `libp2p-dns` built
    // into the `transport` resolves the `dnsaddr` when Kademlia tries
    // to dial these nodes.
    for peer in &BOOTNODES {
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer.parse()?, "/dnsaddr/bootstrap.libp2p.io".parse()?);
    }

    let mut incoming_streams = swarm
        .behaviour()
        .stream
        .new_control()
        .accept(JUNKANOO_PROTOCOL)
        .unwrap();

    // Deal with incoming streams.
    // Spawning a dedicated task is just one way of doing this.
    // libp2p doesn't care how you handle incoming streams but you _must_ handle them somehow.
    // To mitigate DoS attacks, libp2p will internally drop incoming streams if your application
    // cannot keep up processing them.
    tokio::spawn(async move {
        // This loop handles incoming streams _sequentially_ but that doesn't have to be the case.
        // You can also spawn a dedicated task per stream if you want to.
        // Be aware that this breaks backpressure though as spawning new tasks is equivalent to an
        // unbounded buffer. Each task needs memory meaning an aggressive remote peer may
        // force you OOM this way.

        while let Some((peer, stream)) = incoming_streams.next().await {
            // Send data to the peer
        }
    });

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);

    swarm.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse()?)?;

    let listeners = swarm.listeners().map(|addr| addr.clone()).collect();

    Ok((
        Client {
            sender: command_sender,
        },
        event_receiver,
        EventLoop::new(swarm, command_receiver, event_sender),
        listeners,
        peer_id,
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

    async fn send(&self, mut stream: libp2p::Stream) -> io::Result<()> {
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

    /// `async fn`-based connection handler for our custom junkanoo protocol.
    pub(crate) async fn connection_handler(&self, peer: PeerId, mut control: stream::Control) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let stream = match control.open_stream(peer, JUNKANOO_PROTOCOL).await {
                Ok(stream) => stream,
                Err(error @ stream::OpenStreamError::UnsupportedProtocol(_)) => return,
                Err(_) => continue,
            };

            if let Err(_) = self.send(stream).await {
                continue;
            }
        }
    }

    /// Dial the given peer at the given address.
    pub(crate) async fn dial(&mut self, peer_addr: Multiaddr) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::Dial { peer_addr, sender })
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
                eprintln!("New external address of peer {peer_id}: {address}");
            }
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed {
                    result:
                        kad::QueryResult::GetProviders(Ok(
                            kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. },
                        )),
                    ..
                },
            )) => {}
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(_)) => {}
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                eprintln!(
                    "Local node is listening on {:?}",
                    address.with(Protocol::P2p(local_peer_id))
                );
            }
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
            }
            SwarmEvent::ConnectionClosed { .. } => {}
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
            } => eprintln!("Dialing {peer_id}"),
            e => panic!("{e:?}"),
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
            Command::Dial { peer_addr, sender } => match self.swarm.dial(peer_addr) {
                Ok(()) => {
                    let _ = sender.send(Ok(()));
                }
                Err(e) => {
                    let _ = sender.send(Err(Box::new(e)));
                }
            },
            Command::FindPeer { peer_id, sender } => {
                let _ = sender.send(());
            }
            Command::OpenStream { peer_id, sender } => {
                let _ = sender.send(());
            }
        }
    }
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    stream: stream::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
}

#[derive(Debug)]
enum Command {
    StartListening {
        addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    Dial {
        peer_addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    FindPeer {
        peer_id: PeerId,
        sender: oneshot::Sender<()>,
    },
    OpenStream {
        peer_id: PeerId,
        sender: oneshot::Sender<()>,
    },
}

#[derive(Debug)]
pub(crate) enum Event {
    InboundRequest { request: String },
}

// Simple file exchange protocol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileRequest(String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileResponse(Vec<u8>);
