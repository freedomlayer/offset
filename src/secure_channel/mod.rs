#![allow(unused)]

use std::rc::Rc;
use futures::{stream, Stream, Sink, Future};
use futures::prelude::{async, await};
use tokio_core::reactor::Handle;

use futures::sync::mpsc;
use ring::rand::SecureRandom;

use crypto::identity::PublicKey;
use identity::client::IdentityClient;
use timer::TimerClient;

use self::state::{ScStateInitial, ScStateHalf, ScState, ScStateError};
use self::serialize::{serialize_exchange_rand_nonce, deserialize_exchange_rand_nonce,
                        serialize_exchange_dh, deserialize_exchange_dh};
use self::messages::{EncryptedData, PlainData};

mod messages;
mod serialize;
mod state;

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
    HandleExchangeRandNonceError(ScStateError),
    DeserializeExchangeScStateError,
    HandleExchangeScStateError(ScStateError),
    UnexpectedRemotePublicKey,
    RequestTimerStreamError,
    SomeReceiverClosed,
    FromUserError,
    TimerStreamError,
    HandleIncomingError,
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
                            -> Result<(ScState, M, K), SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| SecureChannelError::IdentityFailure)?;

    let (dh_state_initial, exchange_rand_nonce) = ScStateInitial::new(&local_public_key, &*rng);
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
        .map_err(|_| SecureChannelError::DeserializeExchangeScStateError)?;
    let dh_state = dh_state_half.handle_exchange_dh(exchange_dh)
        .map_err(SecureChannelError::HandleExchangeScStateError)?;

    Ok((dh_state, reader, writer))
}

enum SecureChannelEvent {
    Reader(Vec<u8>),
    User(Vec<u8>),
    TimerTick,
    /// Any of the receivers was closed:
    ReceiverClosed,
}


#[async]
fn secure_channel_loop<M: 'static,K: 'static, R: SecureRandom + 'static>(
                              mut dh_state: ScState,
                              reader: M, mut writer: K, 
                              from_user: mpsc::Receiver<Vec<u8>>,
                              mut to_user: mpsc::Sender<Vec<u8>>,
                              rng: Rc<R>,
                              ticks_to_rekey: usize,
                              timer_client: TimerClient)
    -> Result<!, SecureChannelError>
where
    R: SecureRandom,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    // TODO: How to perform greceful shutdown of sinks?
    // Is there a way to do it?

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| SecureChannelError::RequestTimerStreamError)?;
    let timer_stream = timer_stream.map(|_| SecureChannelEvent::TimerTick)
        .map_err(|_| SecureChannelError::TimerStreamError)
        .chain(stream::once(Ok(SecureChannelEvent::ReceiverClosed)));

    let reader = reader.map(SecureChannelEvent::Reader)
        .map_err(|_| SecureChannelError::ReaderError)
        .chain(stream::once(Ok(SecureChannelEvent::ReceiverClosed)));
    let from_user = from_user.map(SecureChannelEvent::User)
        .map_err(|_| SecureChannelError::FromUserError)
        .chain(stream::once(Ok(SecureChannelEvent::ReceiverClosed)));

    let mut cur_ticks_to_rekey = ticks_to_rekey;
    let events = reader.select(from_user).select(timer_stream);

    #[async]
    for event in events {
        match event {
            SecureChannelEvent::Reader(data) => {
                let hi_output = dh_state.handle_incoming(&EncryptedData(data), &*rng)
                    .map_err(|_| SecureChannelError::HandleIncomingError)?;
                if hi_output.rekey_occured {
                    cur_ticks_to_rekey = ticks_to_rekey;
                }
                if let Some(send_message) = hi_output.opt_send_message {
                    writer = await!(writer.send(send_message.0))
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
                if let Some(incoming_message) = hi_output.opt_incoming_message {
                    to_user = await!(to_user.send(incoming_message.0))
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
            },
            SecureChannelEvent::User(data) => {
                let enc_data = dh_state.create_outgoing(&PlainData(data), &*rng);
                writer = await!(writer.send(enc_data.0))
                    .map_err(|_| SecureChannelError::WriterError)?;
            },
            SecureChannelEvent::TimerTick => {
                if let Some(new_cur_ticks_to_rekey) = cur_ticks_to_rekey.checked_sub(1) {
                    cur_ticks_to_rekey = new_cur_ticks_to_rekey;
                    continue;
                }
                let enc_data = match dh_state.create_rekey(&*rng) {
                    Ok(enc_data) => enc_data,
                    Err(ScStateError::RekeyInProgress) => continue,
                    Err(_) => unreachable!(),
                };
                writer = await!(writer.send(enc_data.0))
                    .map_err(|_| SecureChannelError::WriterError)?;
                cur_ticks_to_rekey = ticks_to_rekey;
            },
            SecureChannelEvent::ReceiverClosed => break,
        }
    }
    Err(SecureChannelError::SomeReceiverClosed)
}


#[async]
fn create_secure_channel<M: 'static,K: 'static,R: SecureRandom + 'static>(reader: M, writer: K, 
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: Rc<R>,
                              timer_client: TimerClient,
                              ticks_to_rekey: usize,
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

    let sc_loop = secure_channel_loop(dh_state, 
                                      reader, writer,
                                      from_user,
                                      to_user,
                                      rng,
                                      ticks_to_rekey,
                                      timer_client);

    let sc_loop_report_error = sc_loop.then(|res| {
        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Secure Channel error: {:?}", e);
                Ok(())
            },
        }
    });
    handle.spawn(sc_loop_report_error);

    Ok(SecureChannel {
        sender: user_sender,
        receiver: user_receiver,
    })
}

// TODO: How to make SecureChannel behave like tokio's TcpStream?



