use std::marker::Unpin;
use futures::{future, stream, Stream, StreamExt, 
    Sink, SinkExt, FutureExt};
use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;

use common::conn::{FutTransform, ConnPairVec, BoxFuture};
use common::select_streams::{BoxStream, select_streams};

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use identity::IdentityClient;
use timer::TimerClient;

use crate::state::{ScStateInitial, ScState, ScStateError};
use proto::secure_channel::serialize::{serialize_exchange_rand_nonce, deserialize_exchange_rand_nonce,
                        serialize_exchange_dh, deserialize_exchange_dh};
use proto::secure_channel::messages::{EncryptedData, PlainData};


#[derive(Debug)]
enum SecureChannelError {
    IdentityFailure,
    WriterError,
    ReaderClosed,
    DeserializeRandNonceError,
    HandleExchangeRandNonceError(ScStateError),
    DeserializeExchangeScStateError,
    HandleExchangeScStateError(ScStateError),
    UnexpectedRemotePublicKey,
    RequestTimerStreamError,
    HandleIncomingError,
    SpawnError,
}


async fn initial_exchange<EK, M: 'static,K: 'static,R: CryptoRandom + 'static>(mut writer: K, mut reader: M,
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: R)
                            -> Result<(ScState, K, M), SecureChannelError>
where
    R: CryptoRandom + Clone,
    M: Stream<Item=Vec<u8>> + Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + Unpin,
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

    Ok((dh_state, writer, reader))
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
                              mut writer: K, reader: M, 
                              from_user: mpsc::Receiver<Vec<u8>>,
                              mut to_user: mpsc::Sender<Vec<u8>>,
                              rng: R,
                              ticks_to_rekey: usize,
                              mut timer_client: TimerClient)
    -> Result<(), SecureChannelError>
where
    R: CryptoRandom,
    M: Stream<Item=Vec<u8>> + Unpin + Send,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + Unpin,
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
    let mut events = select_streams![reader, from_user, timer_stream];

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
    Ok(())
}


/// Wrap an existing communication channel (writer, reader) with an encryption layer.
/// Returns back a pair of (writer, reader) that allows to send messages using the encryption
/// layer.
///
/// opt_expected_remote is the expected identity of the remote side. `None` means that any remote
/// identity is permitted. `Some(public_key)` means that only the identity `public_key` is allowed.
///
/// `ticks_to_rekey` is the amount of time ticks it takes to issue a rekey, changing the symmetric
/// key used for the encryption.
async fn create_secure_channel<EK,M,K,R,S>(writer: K, reader: M,
                              identity_client: IdentityClient,
                              opt_expected_remote: Option<PublicKey>,
                              rng: R,
                              timer_client: TimerClient,
                              ticks_to_rekey: usize,
                              mut spawner: S)
    -> Result<(PublicKey, ConnPairVec), SecureChannelError>
where
    EK: 'static,
    M: Stream<Item=Vec<u8>> + Unpin + Send + 'static,
    K: Sink<SinkItem=Vec<u8>, SinkError=EK> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn,
{

    let (dh_state, writer, reader) = await!(initial_exchange(
                                                writer, 
                                                reader, 
                                                identity_client, 
                                                opt_expected_remote, 
                                                rng.clone()))?;

    let remote_public_key = dh_state.get_remote_public_key().clone();

    let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);
    let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(0);

    let sc_loop = secure_channel_loop(dh_state, 
                                      writer, reader,
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
    spawner.spawn(sc_loop_report_error)
        .map_err(|_| SecureChannelError::SpawnError)?;

    Ok((remote_public_key, (user_sender, user_receiver)))
}

#[derive(Clone)]
pub struct SecureChannel<R,S> {
    identity_client: IdentityClient,
    rng: R,
    timer_client: TimerClient,
    ticks_to_rekey: usize,
    spawner: S,
}

impl<R,S> SecureChannel<R,S> {
    pub fn new(identity_client: IdentityClient,
               rng: R,
               timer_client: TimerClient,
               ticks_to_rekey: usize,
               spawner: S) -> SecureChannel<R,S> {

        SecureChannel {
            identity_client,
            rng,
            timer_client,
            ticks_to_rekey,
            spawner,
        }
    }
}

impl <R,S> FutTransform for SecureChannel<R,S>
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + Sync,
{
    /// Input:
    /// - Expected public key of the remote side.
    /// - (sender, receiver) of the plain channel.
    type Input = (Option<PublicKey>, ConnPairVec);
    /// Output:
    /// - Public key of remote side (Must match the expected public key of remote side if
    /// specified).
    /// - (sender, receiver) for the resulting encrypted channel.
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, input: (Option<PublicKey>, ConnPairVec))
        -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> {

        let (opt_expected_remote, conn_pair) = input;
        let (sender, receiver) = conn_pair;

        Box::pin(async move {
            await!(create_secure_channel(sender, receiver,
                      self.identity_client.clone(),
                      opt_expected_remote.clone(),
                      self.rng.clone(),
                      self.timer_client.clone(),
                      self.ticks_to_rekey,
                      self.spawner.clone())).ok()
        })
    }
}



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

    async fn secure_channel1(fut_sc: impl Future<Output=Result<(PublicKey, ConnPairVec), SecureChannelError>> + 'static,
                       mut tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) {

        let (_public_key, (mut sender, mut receiver)) = await!(fut_sc).unwrap();
        await!(sender.send(vec![0,1,2,3,4,5])).unwrap();
        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![5,4,3]);

        // Move time forward, to cause rekying:
        for _ in 0_usize .. 20 {
            await!(tick_sender.send(())).unwrap();
        }
        await!(sender.send(vec![0,1,2])).unwrap();

        output_sender.send(true).unwrap();
    }

    async fn secure_channel2(fut_sc: impl Future<Output=Result<(PublicKey, ConnPairVec), SecureChannelError>> + 'static,
                       _tick_sender: mpsc::Sender<()>,
                       output_sender: oneshot::Sender<bool>) {

        let (_public_key, (mut sender, mut receiver)) = await!(fut_sc).unwrap();
        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![0,1,2,3,4,5]);
        await!(sender.send(vec![5,4,3])).unwrap();

        let data = await!(receiver.next()).unwrap();
        assert_eq!(data, vec![0,1,2]);

        output_sender.send(true).unwrap();
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

        let fut_sc1 = create_secure_channel(
                              sender1.sink_map_err(|_| ()),
                              receiver1, 
                              identity_client1,
                              Some(public_key2),
                              rng1.clone(),
                              timer_client.clone(),
                              ticks_to_rekey,
                              thread_pool.clone());

        let fut_sc2 = create_secure_channel(
                              sender2.sink_map_err(|_| ()),
                              receiver2,
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



