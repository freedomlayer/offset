use super::*;
use std::rc::Rc;

use futures::executor::ThreadPool;
use futures::{future, FutureExt};
use futures::task::SpawnExt;

use identity::{create_identity, IdentityClient};

use crypto::test_utils::DummyRandom;
use crypto::identity::{SoftwareEd25519Identity,
                        generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                        PublicKey};

use crate::token_channel::{is_public_key_lower, TcDirection};
use crate::types::{FunderIncoming, IncomingControlMessage, 
    AddFriend, ChannelerConfig};


async fn task_handler_pair_basic(identity_client1: IdentityClient, 
                                 identity_client2: IdentityClient) {
    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = await!(identity_client1.request_public_key()).unwrap();
    let pk2 = await!(identity_client2.request_public_key()).unwrap();
    let (identity_client1, pk1, identity_client2, pk2) = if is_public_key_lower(&pk1, &pk2) {
        (identity_client1, pk1, identity_client2, pk2)
    } else {
        (identity_client2, pk2, identity_client1, pk1)
    };

    let state1 = FunderState::<u32>::new(&pk1);
    let ephemeral1 = FunderEphemeral::new(&state1);
    let state2 = FunderState::<u32>::new(&pk2);
    let ephemeral2 = FunderEphemeral::new(&state2);

    let rc_rng = Rc::new(DummyRandom::new(&[3u8]));
    // Initialize 1:
    let funder_incoming = FunderIncoming::Init;
    let funder_handler_output = await!(funder_handle_message(identity_client1.clone(),
                          rc_rng.clone(),
                          state1.clone(),
                          ephemeral1,
                          funder_incoming)).unwrap();
    let FunderHandlerOutput {ephemeral: ephemeral1, mutations, outgoing_comms, outgoing_control}
        = funder_handler_output;
    assert!(mutations.is_empty());
    assert!(outgoing_comms.is_empty());
    assert!(outgoing_control.is_empty());

    // Initialize 2:
    let funder_incoming = FunderIncoming::Init;
    let funder_handler_output = await!(funder_handle_message(identity_client2.clone(),
                          rc_rng.clone(),
                          state2.clone(),
                          ephemeral2,
                          funder_incoming)).unwrap();
    let FunderHandlerOutput {ephemeral: ephemeral2, mutations, outgoing_comms, outgoing_control}
        = funder_handler_output;
    assert!(mutations.is_empty());
    assert!(outgoing_comms.is_empty());
    assert!(outgoing_control.is_empty());

    // Add friend 1:
    let add_friend = AddFriend {
        friend_public_key: pk2.clone(),
        address: 22u32,
    };
    let incoming_control_message = IncomingControlMessage::AddFriend(add_friend);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let funder_handler_output = await!(funder_handle_message(identity_client1.clone(),
                          rc_rng.clone(),
                          state1.clone(),
                          ephemeral1,
                          funder_incoming)).unwrap();
    let FunderHandlerOutput {ephemeral: ephemeral1, mut mutations, mut outgoing_comms, outgoing_control}
        = funder_handler_output;

    assert_eq!(mutations.len(), 1);
    let mutation = mutations.pop().unwrap();
    if let FunderMutation::AddFriend((pk, a)) = mutation {
        assert_eq!(pk, pk2);
        assert_eq!(a, 22);
    } else {
        unreachable!();
    }

    assert_eq!(outgoing_comms.len(), 1);
    let outgoing_comm = outgoing_comms.pop().unwrap();
    if let FunderOutgoingComm::ChannelerConfig(ChannelerConfig::AddFriend((pk, a))) = outgoing_comm {
        assert_eq!(pk, pk2);
        assert_eq!(a, 22);
    } else {
        unreachable!();
    }

    // We expect that a report will be sent:
    assert_eq!(outgoing_control.len(), 1);

    // TODO: Continue here.

}

#[test]
fn test_handler_pair_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();

    let rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng1);
    let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool.spawn(identity_server1.then(|_| future::ready(()))).unwrap();

    let rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng2);
    let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool.spawn(identity_server2.then(|_| future::ready(()))).unwrap();

    thread_pool.run(task_handler_pair_basic(identity_client1, identity_client2));
}
