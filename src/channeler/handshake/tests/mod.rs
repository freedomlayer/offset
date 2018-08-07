use super::{HandshakeServer, HandshakeClient};

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crypto::identity::{Identity, SoftwareEd25519Identity};

use channeler::types::{ChannelerNeighbor, ChannelerNeighborInfo};
use ring::{rand::SystemRandom, signature::Ed25519KeyPair, test::rand::FixedByteRandom};

// TODO: Add a macro to construct test.

#[test]
fn handshake_happy_path() {
    let (public_key_a, identity_a) = {
        let fixed_rand = FixedByteRandom { byte: 0x00 };
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key = identity.get_public_key();

        (public_key, identity)
    };

    let (public_key_b, identity_b) = {
        let fixed_rand = FixedByteRandom { byte: 0x01 };
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key = identity.get_public_key();

        (public_key, identity)
    };

    let channeler_neighbor_info_a = ChannelerNeighborInfo {
        public_key:  public_key_a.clone(),
        socket_addr: None,
    };
    let channeler_neighbor_info_b = ChannelerNeighborInfo {
        public_key:  public_key_b.clone(),
        socket_addr: Some("127.0.0.1:10001".parse().unwrap()),
    };

    let node_a = ChannelerNeighbor::new(channeler_neighbor_info_a);
    let node_b = ChannelerNeighbor::new(channeler_neighbor_info_b);

    let (neighbors_a, neighbors_b) = {
        let mut neighbors_a = HashMap::new();
        neighbors_a.insert(node_b.remote_public_key().clone(), node_b);

        let mut neighbors_b = HashMap::new();
        neighbors_b.insert(node_a.remote_public_key().clone(), node_a);

        (
            Rc::new(RefCell::new(neighbors_a)),
            Rc::new(RefCell::new(neighbors_b)),
        )
    };

    let shared_secure_rng = Rc::new(SystemRandom::new());

    //         let mut handshake_server_a = HandshakeServer::new(
    //             public_key_a.clone(),
    //             neighbors_a,
    //             Rc::clone(&shared_secure_rng),
    //         );

    let mut handshake_client_a = HandshakeClient::new(
        public_key_a.clone(),
        neighbors_a,
        Rc::clone(&shared_secure_rng),
    );

    let mut handshake_server_b = HandshakeServer::new(
        public_key_b.clone(),
        neighbors_b,
        Rc::clone(&shared_secure_rng),
    );

    //         let mut handshake_client_b = HandshakeClient::new(
    //             public_key_b.clone(),
    //             neighbors_b,
    //             Rc::clone(&shared_secure_rng),
    //         );

    // A -> B: RequestNonce
    let request_nonce_to_b = handshake_client_a
        .initiate_handshake(public_key_b.clone())
        .unwrap();

    // B -> A: RespondNonce
    let mut respond_nonce_to_a = handshake_server_b
        .handle_request_nonce(request_nonce_to_b)
        .unwrap();
    respond_nonce_to_a.signature = identity_b.sign_message(&respond_nonce_to_a.as_bytes());

    // A -> B: ExchangeActive
    let mut exchange_active_to_b = handshake_client_a
        .handle_response_nonce(respond_nonce_to_a)
        .unwrap();
    exchange_active_to_b.signature = identity_a.sign_message(&exchange_active_to_b.as_bytes());

    // B -> A: ExchangePassive
    let mut exchange_passive_to_a = handshake_server_b
        .handle_exchange_active(exchange_active_to_b)
        .unwrap();
    exchange_passive_to_a.signature = identity_b.sign_message(&exchange_passive_to_a.as_bytes());

    // A -> B: ChannelReady (A: Finish)
    let (channel_metadata_a, mut channel_ready_to_b) = handshake_client_a
        .handle_exchange_passive(exchange_passive_to_a)
        .unwrap();
    channel_ready_to_b.signature = identity_a.sign_message(&channel_ready_to_b.as_bytes());

    // B: Finish
    let channel_metadata_b = handshake_server_b
        .handle_channel_ready(channel_ready_to_b)
        .unwrap();

    assert_eq!(channel_metadata_a.tx_channel_id, channel_metadata_b.rx_channel_id);
    assert_eq!(channel_metadata_b.tx_channel_id, channel_metadata_a.rx_channel_id);
}
