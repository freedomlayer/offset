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
mod sc_stream;

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

    let (reader_message, reader) = await!(read_from_reader(reader))?;

    let exchange_rand_nonce = deserialize_exchange_rand_nonce(&reader_message)
        .map_err(|_| SecureChannelError::DeserializeRandNonceError)?;
    let (dh_state_half, exchange_dh) = await!(dh_state_initial.handle_exchange_rand_nonce(
                                                exchange_rand_nonce,
                                                identity_client.clone(),
                                                Rc::clone(&rng)))
        .map_err(SecureChannelError::HandleExchangeRandNonceError)?;

    if let Some(expected_remote) = opt_expected_remote {
        if expected_remote != dh_state_half.remote_public_key {
            return Err(SecureChannelError::UnexpectedRemotePublicKey);
        }
    }


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
pub fn create_secure_channel<M: 'static,K: 'static,R: SecureRandom + 'static>(reader: M, writer: K, 
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


#[cfg(test)]
mod tests {
    use super::*;
    use timer::create_timer_incoming;
    use futures::prelude::{async, await};
    use futures::Future;
    use futures::sync::oneshot;
    use tokio_core::reactor::Core;
    use ring::test::rand::FixedByteRandom;
    use ring::signature;
    use crypto::identity::{Identity, SoftwareEd25519Identity};
    use identity::create_identity;
    use identity::client::IdentityClient;

    #[async]
    fn secure_channel1(fut_sc: impl Future<Item=SecureChannel, Error=SecureChannelError> + 'static,
                       mut tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) -> Result<(),()> {
        let SecureChannel {sender, receiver} = await!(fut_sc).unwrap();
        let sender = await!(sender.send(vec![0,1,2,3,4,5])).unwrap();
        let (data, receiver) = await!(read_from_reader(receiver)).unwrap();
        assert_eq!(data, vec![5,4,3]);

        // Move time forward, to cause rekying:
        for _ in 0_usize .. 20 {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }
        let sender = await!(sender.send(vec![0,1,2])).unwrap();

        output_sender.send(true);
        Ok(())
    }

    #[async]
    fn secure_channel2(fut_sc: impl Future<Item=SecureChannel, Error=SecureChannelError> + 'static,
                       tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) -> Result<(),()> {

        let SecureChannel {sender, receiver} = await!(fut_sc).unwrap();
        let (data, receiver) = await!(read_from_reader(receiver)).unwrap();
        assert_eq!(data, vec![0,1,2,3,4,5]);
        let sender = await!(sender.send(vec![5,4,3])).unwrap();

        let (data, receiver) = await!(read_from_reader(receiver)).unwrap();
        assert_eq!(data, vec![0,1,2]);

        output_sender.send(true);
        Ok(())
    }

    #[test]
    fn test_secure_channel_basic() {
        // Start the Identity service:
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        let rng1 = FixedByteRandom { byte: 0x1 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng1).unwrap();
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key1 = identity1.get_public_key();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);

        let rng2 = FixedByteRandom { byte: 0x2 };
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng2).unwrap();
        let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key2 = identity2.get_public_key();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);

        handle.spawn(identity_server1.then(|_| Ok(())));
        handle.spawn(identity_server2.then(|_| Ok(())));

        let (sender1, receiver2) = mpsc::channel::<Vec<u8>>(0);
        let (sender2, receiver1) = mpsc::channel::<Vec<u8>>(0);


        let rc_rng1 = Rc::new(rng1);
        let rc_rng2 = Rc::new(rng2);

        let ticks_to_rekey: usize = 16;

        let fut_sc1 = create_secure_channel(receiver1, 
                              sender1.sink_map_err(|_| ()),
                              identity_client1,
                              Some(public_key2),
                              Rc::clone(&rc_rng1),
                              timer_client.clone(),
                              ticks_to_rekey,
                              handle.clone());

        let fut_sc2 = create_secure_channel(receiver2,
                              sender2.sink_map_err(|_| ()),
                              identity_client2,
                              Some(public_key1),
                              Rc::clone(&rc_rng2),
                              timer_client.clone(),
                              ticks_to_rekey,
                              handle.clone());

        let (output_sender1, output_receiver1) = oneshot::channel::<bool>();
        let (output_sender2, output_receiver2) = oneshot::channel::<bool>();

        handle.spawn(secure_channel1(fut_sc1, tick_sender.clone(), output_sender1));
        handle.spawn(secure_channel2(fut_sc2, tick_sender.clone(), output_sender2));

        assert_eq!(true, core.run(output_receiver1).unwrap());
        assert_eq!(true, core.run(output_receiver2).unwrap());
    }
}



