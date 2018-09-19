#![allow(unused)]

use std::rc::Rc;
use futures::{Stream, Sink};
use futures::prelude::{async, await};
use futures::sync::mpsc;
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use identity::client::IdentityClient;
use timer::messages::FromTimer;

mod messages;
mod serialize;
mod state;

// const MAX_FRAME_LEN: usize = 0x1000

struct SecureChannel;

#[derive(Debug)]
enum SecureChannelError {
    IdentityFailure,
}

#[async]
fn create_secure_channel<M,K,R>(reader: M, writer: K, 
                              identity_client: IdentityClient,
                              expected_remote: Option<PublicKey>,
                              rng: Rc<R>,
                              from_timer: mpsc::Receiver<FromTimer>) 
    -> Result<SecureChannel, SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| SecureChannelError::IdentityFailure)?;

    // let (dh_state_initial, exchange_rand_nonce) = DhStateInitial::new(&local_public_key1, &*rng1);
    
    // TODO:
    // - Send rand nonce exchange
    // - Wait (timeout)
    // - Send exchange dh
    // - Wait (timeout)
    // - Create two pairs of (sender, receiver)
    // - spawn an even loop (Separate async function) over the following:
    //      - Timer event
    //          - Send keepalive if required
    //          - rekey
    //      - incoming message from remote
    //          - Forward remote message to user
    //      - incoming message from user
    //          - Forward user message to remote
    //
    // - Return to user a SecureChannel, that contains user sender and user receiver.

    Ok(SecureChannel)
}

// TODO: How to make SecureChannel behave like tokio's TcpStream?



