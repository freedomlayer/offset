#![allow(unused)]

use futures::{future, stream, Stream, StreamExt, 
    Sink, SinkExt, Future, FutureExt};
use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use identity::IdentityClient;
use timer::TimerClient;

use crate::state::{ScStateInitial, ScStateHalf, ScState, ScStateError};
use proto::secure_channel::serialize::{serialize_exchange_rand_nonce, deserialize_exchange_rand_nonce,
                        serialize_exchange_dh, deserialize_exchange_dh};
use proto::secure_channel::messages::{EncryptedData, PlainData};


pub struct SecureChannel {
    pub sender: mpsc::Sender<Vec<u8>>,
    pub receiver: mpsc::Receiver<Vec<u8>>,
}

#[derive(Debug)]
pub enum SecureChannelError {
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

/*
/// Read one message from reader
async fn read_from_reader<M: 'static>(reader: M) -> Result<(Vec<u8>, M), SecureChannelError>
    where M: Stream<Item=Vec<u8>>,
{
    match await!(reader.next()) {
        Some(reader_message) => Ok((reader_message, ret_reader)),
        None => return Err(SecureChannelError::ReaderClosed),
    }
}
*/

async fn initial_exchange<EK, M: 'static,K: 'static,R: CryptoRandom + 'static>(mut reader: M, mut writer: K, 
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: R)
                            -> Result<(ScState, M, K), SecureChannelError>
where
    R: CryptoRandom,
    M: Stream<Item=Vec<u8>> + std::marker::Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + std::marker::Unpin,
{
    let local_public_key = await!(identity_client.request_public_key())
        .map_err(|_| SecureChannelError::IdentityFailure)?;

    let (dh_state_initial, exchange_rand_nonce) = ScStateInitial::new(&local_public_key, &rng);
    let ser_exchange_rand_nonce = serialize_exchange_rand_nonce(&exchange_rand_nonce);
    await!(writer.send(ser_exchange_rand_nonce))
        .map_err(|_| SecureChannelError::WriterError)?;

    let reader_message = await!(reader.next())
        .ok_or(SecureChannelError::ReaderClosed)?;


    let exchange_rand_nonce = deserialize_exchange_rand_nonce(&reader_message)
        .map_err(|_| SecureChannelError::DeserializeRandNonceError)?;
    let (dh_state_half, exchange_dh) = await!(dh_state_initial.handle_exchange_rand_nonce(
                                                exchange_rand_nonce,
                                                identity_client.clone(),
                                                rng.clone()))
        .map_err(SecureChannelError::HandleExchangeRandNonceError)?;

    if let Some(expected_remote) = opt_expected_remote {
        if expected_remote != dh_state_half.remote_public_key {
            return Err(SecureChannelError::UnexpectedRemotePublicKey);
        }
    }


    let ser_exchange_dh = serialize_exchange_dh(&exchange_dh);
    await!(writer.send(ser_exchange_dh))
        .map_err(|_| SecureChannelError::WriterError)?;

    let reader_message = await!(reader.next())
        .ok_or(SecureChannelError::ReaderClosed)?;
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


async fn secure_channel_loop<EK, M: 'static,K: 'static, R: CryptoRandom + 'static>(
                              mut dh_state: ScState,
                              reader: M, mut writer: K, 
                              from_user: mpsc::Receiver<Vec<u8>>,
                              mut to_user: mpsc::Sender<Vec<u8>>,
                              rng: R,
                              ticks_to_rekey: usize,
                              mut timer_client: TimerClient)
    -> Result<!, SecureChannelError>
where
    R: CryptoRandom,
    M: Stream<Item=Vec<u8>> + std::marker::Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + std::marker::Unpin,
{
    // TODO: How to perform greceful shutdown of sinks?
    // Is there a way to do it?

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| SecureChannelError::RequestTimerStreamError)?;
    let timer_stream = timer_stream.map(|_| SecureChannelEvent::TimerTick)
        .chain(stream::once(future::ready(SecureChannelEvent::ReceiverClosed)));

    let reader = reader.map(SecureChannelEvent::Reader)
        .chain(stream::once(future::ready(SecureChannelEvent::ReceiverClosed)));
    let from_user = from_user.map(SecureChannelEvent::User)
        .chain(stream::once(future::ready(SecureChannelEvent::ReceiverClosed)));

    let mut cur_ticks_to_rekey = ticks_to_rekey;
    let mut events = reader.select(from_user).select(timer_stream);

    while let Some(event) = await!(events.next()) {
        match event {
            SecureChannelEvent::Reader(data) => {
                let hi_output = dh_state.handle_incoming(&EncryptedData(data), &rng)
                    .map_err(|_| SecureChannelError::HandleIncomingError)?;
                if hi_output.rekey_occured {
                    cur_ticks_to_rekey = ticks_to_rekey;
                }
                if let Some(send_message) = hi_output.opt_send_message {
                    await!(writer.send(send_message.0))
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
                if let Some(incoming_message) = hi_output.opt_incoming_message {
                    await!(to_user.send(incoming_message.0))
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
            },
            SecureChannelEvent::User(data) => {
                let enc_data = dh_state.create_outgoing(&PlainData(data), &rng);
                await!(writer.send(enc_data.0))
                    .map_err(|_| SecureChannelError::WriterError)?;
            },
            SecureChannelEvent::TimerTick => {
                if let Some(new_cur_ticks_to_rekey) = cur_ticks_to_rekey.checked_sub(1) {
                    cur_ticks_to_rekey = new_cur_ticks_to_rekey;
                    continue;
                }
                let enc_data = match dh_state.create_rekey(&rng) {
                    Ok(enc_data) => enc_data,
                    Err(ScStateError::RekeyInProgress) => continue,
                    Err(_) => unreachable!(),
                };
                await!(writer.send(enc_data.0))
                    .map_err(|_| SecureChannelError::WriterError)?;
                cur_ticks_to_rekey = ticks_to_rekey;
            },
            SecureChannelEvent::ReceiverClosed => break,
        }
    }
    Err(SecureChannelError::SomeReceiverClosed)
}


pub async fn create_secure_channel<EK: 'static, M: 'static,K: 'static,R: CryptoRandom + 'static>(reader: M, writer: K, 
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: R,
                              timer_client: TimerClient,
                              ticks_to_rekey: usize,
                              mut spawner: impl Spawn)
    -> Result<SecureChannel, SecureChannelError>
where
    R: CryptoRandom,
    M: Stream<Item=Vec<u8>> + std::marker::Unpin + std::marker::Send,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + std::marker::Unpin + std::marker::Send,
{

    let (dh_state, reader, writer) = await!(initial_exchange(
                                                reader, 
                                                writer, 
                                                identity_client, 
                                                opt_expected_remote, 
                                                rng.clone()))?;

    let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);
    let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(0);

    let sc_loop = secure_channel_loop(dh_state, 
                                      reader, writer,
                                      from_user,
                                      to_user,
                                      rng.clone(),
                                      ticks_to_rekey,
                                      timer_client);

    let sc_loop_report_error = sc_loop.map(|res| {
        if let Err(e) = res {
            error!("Secure Channel error: {:?}", e);
        }
    });
    spawner.spawn(sc_loop_report_error);

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
    use futures::Future;
    use futures::channel::oneshot;

    use futures::executor::ThreadPool;
    use futures::task::SpawnExt;

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{Identity, SoftwareEd25519Identity,
                            generate_pkcs8_key_pair};
    use identity::{create_identity, IdentityClient};

    async fn secure_channel1(fut_sc: impl Future<Output=Result<SecureChannel, SecureChannelError>> + 'static,
                       mut tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) {
        let SecureChannel {mut sender, mut receiver} = await!(fut_sc).unwrap();
        await!(sender.send(vec![0,1,2,3,4,5])).unwrap();
        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![5,4,3]);

        // Move time forward, to cause rekying:
        for _ in 0_usize .. 20 {
            await!(tick_sender.send(())).unwrap();
        }
        await!(sender.send(vec![0,1,2])).unwrap();

        output_sender.send(true);
    }

    async fn secure_channel2(fut_sc: impl Future<Output=Result<SecureChannel, SecureChannelError>> + 'static,
                       tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) {

        let SecureChannel {mut sender, mut receiver} = await!(fut_sc).unwrap();
        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![0,1,2,3,4,5]);
        await!(sender.send(vec![5,4,3])).unwrap();

        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![0,1,2]);

        output_sender.send(true);
    }

    #[test]
    fn test_secure_channel_basic() {
        // Start the Identity service:
        let mut thread_pool = ThreadPool::new().unwrap();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

        let rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng1);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key1 = identity1.get_public_key();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);

        let rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng2);
        let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let public_key2 = identity2.get_public_key();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);

        thread_pool.spawn(identity_server1.then(|_| future::ready(()))).unwrap();
        thread_pool.spawn(identity_server2.then(|_| future::ready(()))).unwrap();

        let (sender1, receiver2) = mpsc::channel::<Vec<u8>>(0);
        let (sender2, receiver1) = mpsc::channel::<Vec<u8>>(0);


        let ticks_to_rekey: usize = 16;

        let fut_sc1 = create_secure_channel(receiver1, 
                              sender1.sink_map_err(|_| ()),
                              identity_client1,
                              Some(public_key2),
                              rng1.clone(),
                              timer_client.clone(),
                              ticks_to_rekey,
                              thread_pool.clone());

        let fut_sc2 = create_secure_channel(receiver2,
                              sender2.sink_map_err(|_| ()),
                              identity_client2,
                              Some(public_key1),
                              rng2.clone(),
                              timer_client.clone(),
                              ticks_to_rekey,
                              thread_pool.clone());

        let (output_sender1, output_receiver1) = oneshot::channel::<bool>();
        let (output_sender2, output_receiver2) = oneshot::channel::<bool>();

        thread_pool.spawn(secure_channel1(fut_sc1, tick_sender.clone(), output_sender1)).unwrap();
        thread_pool.spawn(secure_channel2(fut_sc2, tick_sender.clone(), output_sender2)).unwrap();

        assert_eq!(true, thread_pool.run(output_receiver1).unwrap());
        assert_eq!(true, thread_pool.run(output_receiver2).unwrap());
    }
}



