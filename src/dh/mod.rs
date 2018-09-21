#![allow(unused)]

use std::rc::Rc;
use futures::{Stream, Sink, Future};
use futures::prelude::{async, await};
use tokio_core::reactor::Handle;

use futures::sync::mpsc;
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use identity::client::IdentityClient;
use timer::TimerClient;

use self::state::{DhStateInitial, DhStateHalf, DhState, DhError};
use self::serialize::{serialize_exchange_rand_nonce, deserialize_exchange_rand_nonce,
                        serialize_exchange_dh, deserialize_exchange_dh};

mod messages;
mod serialize;
mod state;

// const MAX_FRAME_LEN: usize = 0x1000

struct SecureChannel {
    sender: mpsc::Sender<Vec<u8>>,
    receiver: mpsc::Receiver<Vec<u8>>,
}

#[derive(Debug)]
enum SecureChannelError {
    IdentityFailure,
    WriterError,
    ReaderClosed,
    ReaderError,
    DeserializeRandNonceError,
    HandleExchangeRandNonceError(DhError),
    DeserializeExchangeDhError,
    HandleExchangeDhError(DhError),
    UnexpectedRemotePublicKey,
}

/// Read one message from reader
#[async]
fn read_from_reader<M: 'static>(reader: M) -> Result<(Vec<u8>, M), SecureChannelError>
    where M: Stream<Item=Vec<u8>, Error=()>,
{
    match await!(reader.into_future()) {
        Ok((opt_reader_message, ret_reader)) => {
            match opt_reader_message {
                Some(reader_message) => Ok((reader_message, ret_reader)),
                None => return Err(SecureChannelError::ReaderClosed),
            }
        },
        Err(_) => return Err(SecureChannelError::ReaderError),
    }
}

#[async]
fn initial_exchange<M: 'static,K: 'static,R: SecureRandom + 'static>(reader: M, writer: K, 
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: Rc<R>)
                            -> Result<(DhState, M, K), SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| SecureChannelError::IdentityFailure)?;

    let (dh_state_initial, exchange_rand_nonce) = DhStateInitial::new(&local_public_key, &*rng);
    let ser_exchange_rand_nonce = serialize_exchange_rand_nonce(&exchange_rand_nonce);
    let writer = await!(writer.send(ser_exchange_rand_nonce))
        .map_err(|_| SecureChannelError::WriterError)?;

    if let Some(expected_remote) = opt_expected_remote {
        if expected_remote != local_public_key {
            return Err(SecureChannelError::UnexpectedRemotePublicKey);
        }
    }

    let (reader_message, reader) = await!(read_from_reader(reader))?;

    let exchange_rand_nonce = deserialize_exchange_rand_nonce(&reader_message)
        .map_err(|_| SecureChannelError::DeserializeRandNonceError)?;
    let (dh_state_half, exchange_dh) = await!(dh_state_initial.handle_exchange_rand_nonce(
                                                exchange_rand_nonce,
                                                identity_client.clone(),
                                                Rc::clone(&rng)))
        .map_err(SecureChannelError::HandleExchangeRandNonceError)?;

    let ser_exchange_dh = serialize_exchange_dh(&exchange_dh);
    let writer = await!(writer.send(ser_exchange_dh))
        .map_err(|_| SecureChannelError::WriterError)?;

    let (reader_message, reader) = await!(read_from_reader(reader))?;
    let exchange_dh = deserialize_exchange_dh(&reader_message)
        .map_err(|_| SecureChannelError::DeserializeExchangeDhError)?;
    let dh_state = dh_state_half.handle_exchange_dh(exchange_dh)
        .map_err(SecureChannelError::HandleExchangeDhError)?;

    Ok((dh_state, reader, writer))
}

#[async]
fn secure_channel_loop<M: 'static,K: 'static,R: SecureRandom + 'static>(reader: M, writer: K, 
                              from_user: mpsc::Receiver<Vec<u8>>,
                              to_user: mpsc::Sender<Vec<u8>>,
                              rng: Rc<R>,
                              timer_client: TimerClient)
    -> Result<SecureChannel, SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    // TODO:
    // - Timer event
    //      - rekey
    // - incoming message from remote
    //      - Forward remote message to user
    // - incoming message from user
    //      - Forward user message to remote
    
    unimplemented!();

}


#[async]
fn create_secure_channel<M: 'static,K: 'static,R: SecureRandom + 'static>(reader: M, writer: K, 
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: Rc<R>,
                              timer_client: TimerClient,
                              handle: Handle)
    -> Result<SecureChannel, SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{

    let (dh_state, reader, writer) = await!(initial_exchange(
                                                reader, 
                                                writer, 
                                                identity_client, 
                                                opt_expected_remote, 
                                                Rc::clone(&rng)))?;

    let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);
    let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(0);

    let sc_loop = secure_channel_loop(reader, writer,
                                      from_user,
                                      to_user,
                                      rng,
                                      timer_client);

    handle.spawn(sc_loop.then(|_| Ok(())));

    Ok(SecureChannel {
        sender: user_sender,
        receiver: user_receiver,
    })
}

// TODO: How to make SecureChannel behave like tokio's TcpStream?



