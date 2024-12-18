use anyhow::Result;
use libp2p::{
    core::upgrade,
    identity,
    noise::{NoiseConfig, X25519Spec},
    tcp::TokioTcpConfig,
    yamux::YamuxConfig,
    PeerId,
    Swarm,
    Transport,
    futures::StreamExt,
};
use std::time::Duration;
use tokio::sync::mpsc;

// Custom behavior combining multiple protocols
#[derive(NetworkBehaviour)]
struct NodeBehaviour {
    identify: libp2p::identify::Behaviour,
    ping: libp2p::ping::Behaviour,
    kademlia: Kademlia,
}

pub struct Network {
    swarm: Swarm<NodeBehaviour>,
    _shutdown_sender: mpsc::Sender<()>,
    shutdown_receiver: mpsc::Receiver<()>,
}

impl Network {
    pub async fn new() -> Result<Self> {
        // Generate new key pair
        let keypair = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        // Create transport with noise encryption and yamux multiplexing
        let transport = TokioTcpConfig::new()
            .nodelay(true)
            .upgrade(upgrade::Version::V1)
            .authenticate(NoiseConfig::xx(keypair.clone()).into_authenticated())
            .multiplex(YamuxConfig::default())
            .boxed();

        // Create Kademlia instance
        let mut kademlia_config = KademliaConfig::default();
        kademlia_config.set_query_timeout(Duration::from_secs(5 * 60));
        let store = libp2p::kad::store::MemoryStore::new(peer_id);
        let mut kademlia = Kademlia::with_config(peer_id, store, kademlia_config);

        // Add bootstrap nodes (IPFS)
        let bootstrap_nodes = vec![
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
            "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
        ];

        for addr in bootstrap_nodes {
            if let Ok((peer_id, multiaddr)) = convert_bootstrap_node(addr) {
                kademlia.add_address(&peer_id, multiaddr);
            }
        }

        // Create network behavior
        let behaviour = NodeBehaviour {
            identify: libp2p::identify::Behaviour::new(
                libp2p::identify::Config::new("dogon/1.0.0".into(), keypair.public())
            ),
            ping: libp2p::ping::Behaviour::new(
                libp2p::ping::Config::new()
                    .with_interval(Duration::from_secs(30))
                    .with_timeout(Duration::from_secs(10))
            ),
            kademlia,
        };

        // Create swarm
        let swarm = Swarm::new(transport, behaviour, peer_id);

        // Create shutdown channel
        let (shutdown_sender, shutdown_receiver) = mpsc::channel(1);

        Ok(Self {
            swarm,
            _shutdown_sender: shutdown_sender,
            shutdown_receiver,
        })
    }

    pub async fn start(mut self) -> Result<()> {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_event(event).await?;
                }
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: libp2p::swarm::SwarmEvent<NodeBehaviourEvent>) -> Result<()> {
        match event {
            libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on: {address}");
                // Bootstrap the Kademlia DHT
                if let Err(e) = self.swarm.behaviour_mut().kademlia.bootstrap() {
                    println!("Failed to bootstrap Kademlia DHT: {e}");
                }
            }
            libp2p::swarm::SwarmEvent::Behaviour(event) => {
                match event {
                    NodeBehaviourEvent::Identify(event) => {
                        println!("Identify event: {event:?}");
                        // Add Kademlia addresses from identified peers
                        if let libp2p::identify::Event::Received { peer_id, info, .. } = event {
                            for addr in info.listen_addrs {
                                self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            }
                        }
                    }
                    NodeBehaviourEvent::Ping(event) => {
                        if let libp2p::ping::Event {
                            peer,
                            result: Ok(duration),
                            ..
                        } = event
                        {
                            println!("Ping success from {peer}: {duration:?}");
                        }
                    }
                    NodeBehaviourEvent::Kademlia(event) => {
                        match event {
                            KademliaEvent::OutboundQueryCompleted { result, .. } => {
                                match result {
                                    QueryResult::GetClosestPeers(Ok(peers)) => {
                                        println!("Found closest peers: {:?}", peers.peers);
                                    }
                                    QueryResult::Bootstrap(Ok(stats)) => {
                                        println!("Bootstrap complete: {:?}", stats);
                                    }
                                    QueryResult::GetProviders(Ok(providers)) => {
                                        println!("Found providers: {:?}", providers.providers);
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }
    
    impl Network {
        // Add new methods for peer discovery and DHT operations
        pub async fn start_discovery(&mut self) -> Result<()> {
            self.swarm.behaviour_mut().kademlia.bootstrap()?;
            Ok(())
        }
        
        pub async fn store_value(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
            self.swarm.behaviour_mut().kademlia.put_record(
                libp2p::kad::Record::new(key, value),
                libp2p::kad::Quorum::One
            )?;
            Ok(())
        }
        
        pub async fn get_value(&mut self, key: Vec<u8>) -> Result<()> {
            self.swarm.behaviour_mut().kademlia.get_record(key);
            Ok(())
        }
        
        pub async fn find_peer(&mut self, peer_id: PeerId) -> Result<()> {
            self.swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
            Ok(())
        }
    }
    
    fn convert_bootstrap_node(addr: &str) -> Result<(PeerId, Multiaddr)> {
        let multiaddr: Multiaddr = addr.parse()?;
        let peer_id = match multiaddr.iter().last() {
            Some(libp2p::multiaddr::Protocol::P2p(hash)) => PeerId::from_multihash(hash)?,
            _ => return Err(anyhow::anyhow!("Invalid bootstrap address")),
        };
        let multiaddr = multiaddr
            .iter()
            .take_while(|p| !matches!(p, libp2p::multiaddr::Protocol::P2p(_)))
            .collect();
        Ok((peer_id, multiaddr))
    }
}

