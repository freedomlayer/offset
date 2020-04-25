use std::marker::Unpin;

use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, FutureExt, Sink, SinkExt, Stream, StreamExt};

use futures::channel::mpsc;

use derive_more::From;

use common::conn::{BoxFuture, BoxStream, ConnPairVec, FutTransform};
use common::select_streams::select_streams;

use crypto::rand::CryptoRandom;
use identity::IdentityClient;
use timer::TimerClient;

use proto::crypto::PublicKey;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize, ProtoSerializeError};
use proto::secure_channel::messages::{ExchangeDh, ExchangeRandNonce};

use crate::state::{ScState, ScStateError, ScStateInitial};
use crate::types::{EncryptedData, PlainData};

#[derive(Debug, From)]
enum SecureChannelError {
    IdentityFailure,
    WriterError,
    ReaderClosed,
    ProtoSerializeError(ProtoSerializeError),
    HandleExchangeRandNonceError(ScStateError),
    HandleExchangeScStateError(ScStateError),
    UnexpectedRemotePublicKey,
    RequestTimerStreamError,
    HandleIncomingError,
    SpawnError,
}

async fn initial_exchange<EK, M: 'static, K: 'static, R: CryptoRandom + 'static>(
    mut writer: K,
    mut reader: M,
    identity_client: IdentityClient,
    opt_expected_remote: Option<PublicKey>,
    rng: R,
) -> Result<(ScState, K, M), SecureChannelError>
where
    R: CryptoRandom + Clone,
    M: Stream<Item = Vec<u8>> + Unpin,
    K: Sink<Vec<u8>, Error = EK> + Unpin,
{
    let local_public_key = identity_client
        .request_public_key()
        .await
        .map_err(|_| SecureChannelError::IdentityFailure)?;

    let (dh_state_initial, exchange_rand_nonce) =
        ScStateInitial::new(local_public_key, opt_expected_remote.clone(), &rng);
    let ser_exchange_rand_nonce = exchange_rand_nonce.proto_serialize();
    writer
        .send(ser_exchange_rand_nonce)
        .await
        .map_err(|_| SecureChannelError::WriterError)?;

    let reader_message = reader
        .next()
        .await
        .ok_or(SecureChannelError::ReaderClosed)?;

    let exchange_rand_nonce = ExchangeRandNonce::proto_deserialize(&reader_message)?;
    let (dh_state_half, exchange_dh) = dh_state_initial
        .handle_exchange_rand_nonce(exchange_rand_nonce, identity_client.clone(), rng.clone())
        .await
        .map_err(SecureChannelError::HandleExchangeRandNonceError)?;

    if let Some(expected_remote) = opt_expected_remote {
        if expected_remote != dh_state_half.remote_public_key {
            return Err(SecureChannelError::UnexpectedRemotePublicKey);
        }
    }

    let ser_exchange_dh = exchange_dh.proto_serialize();
    writer
        .send(ser_exchange_dh)
        .await
        .map_err(|_| SecureChannelError::WriterError)?;

    let reader_message = reader
        .next()
        .await
        .ok_or(SecureChannelError::ReaderClosed)?;
    let exchange_dh = ExchangeDh::proto_deserialize(&reader_message)?;
    let dh_state = dh_state_half
        .handle_exchange_dh(exchange_dh)
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

async fn secure_channel_loop<EK, M: 'static, K: 'static, R: CryptoRandom + 'static>(
    mut dh_state: ScState,
    mut writer: K,
    reader: M,
    from_user: mpsc::Receiver<Vec<u8>>,
    mut to_user: mpsc::Sender<Vec<u8>>,
    rng: R,
    ticks_to_rekey: usize,
    mut timer_client: TimerClient,
) -> Result<(), SecureChannelError>
where
    R: CryptoRandom,
    M: Stream<Item = Vec<u8>> + Unpin + Send,
    K: Sink<Vec<u8>, Error = EK> + Unpin,
{
    // TODO: How to perform graceful shutdown of sinks?
    // Is there a way to do it?

    let timer_stream = timer_client
        .request_timer_stream("secure_channel_loop".to_owned())
        .await
        .map_err(|_| SecureChannelError::RequestTimerStreamError)?;
    let timer_stream = timer_stream
        .map(|_| SecureChannelEvent::TimerTick)
        .chain(stream::once(future::ready(
            SecureChannelEvent::ReceiverClosed,
        )));

    let reader = reader
        .map(SecureChannelEvent::Reader)
        .chain(stream::once(future::ready(
            SecureChannelEvent::ReceiverClosed,
        )));
    let from_user = from_user
        .map(SecureChannelEvent::User)
        .chain(stream::once(future::ready(
            SecureChannelEvent::ReceiverClosed,
        )));

    let mut cur_ticks_to_rekey = ticks_to_rekey;
    let mut events = select_streams![reader, from_user, timer_stream];

    while let Some(event) = events.next().await {
        match event {
            SecureChannelEvent::Reader(data) => {
                let hi_output = dh_state
                    .handle_incoming(&EncryptedData(data), &rng)
                    .map_err(|_| SecureChannelError::HandleIncomingError)?;
                if hi_output.rekey_occurred {
                    cur_ticks_to_rekey = ticks_to_rekey;
                }
                if let Some(send_message) = hi_output.opt_send_message {
                    writer
                        .send(send_message.0)
                        .await
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
                if let Some(incoming_message) = hi_output.opt_incoming_message {
                    to_user
                        .send(incoming_message.0)
                        .await
                        .map_err(|_| SecureChannelError::WriterError)?;
                }
            }
            SecureChannelEvent::User(data) => {
                let enc_data = dh_state.create_outgoing(&PlainData(data), &rng);
                writer
                    .send(enc_data.0)
                    .await
                    .map_err(|_| SecureChannelError::WriterError)?;
            }
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
                writer
                    .send(enc_data.0)
                    .await
                    .map_err(|_| SecureChannelError::WriterError)?;
                cur_ticks_to_rekey = ticks_to_rekey;
            }
            SecureChannelEvent::ReceiverClosed => {
                info!("secure_channel_loop(): ReceiverClosed");
                break;
            }
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
async fn create_secure_channel<EK, M, K, R, S>(
    writer: K,
    reader: M,
    identity_client: IdentityClient,
    opt_expected_remote: Option<PublicKey>,
    rng: R,
    timer_client: TimerClient,
    ticks_to_rekey: usize,
    spawner: S,
) -> Result<(PublicKey, ConnPairVec), SecureChannelError>
where
    EK: 'static,
    M: Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    K: Sink<Vec<u8>, Error = EK> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn,
{
    let (dh_state, writer, reader) = initial_exchange(
        writer,
        reader,
        identity_client,
        opt_expected_remote,
        rng.clone(),
    )
    .await?;

    let remote_public_key = dh_state.get_remote_public_key().clone();

    let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(1);
    let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(1);

    let sc_loop = secure_channel_loop(
        dh_state,
        writer,
        reader,
        from_user,
        to_user,
        rng.clone(),
        ticks_to_rekey,
        timer_client,
    );

    let sc_loop_report_error = sc_loop.map(|res| {
        if let Err(e) = res {
            warn!("Secure Channel error: {:?}", e);
        }
    });
    spawner
        .spawn(sc_loop_report_error)
        .map_err(|_| SecureChannelError::SpawnError)?;

    Ok((
        remote_public_key,
        ConnPairVec::from_raw(user_sender, user_receiver),
    ))
}

#[derive(Clone)]
pub struct SecureChannel<R, S> {
    identity_client: IdentityClient,
    rng: R,
    timer_client: TimerClient,
    ticks_to_rekey: usize,
    spawner: S,
}

impl<R, S> SecureChannel<R, S> {
    pub fn new(
        identity_client: IdentityClient,
        rng: R,
        timer_client: TimerClient,
        ticks_to_rekey: usize,
        spawner: S,
    ) -> SecureChannel<R, S> {
        SecureChannel {
            identity_client,
            rng,
            timer_client,
            ticks_to_rekey,
            spawner,
        }
    }
}

impl<R, S> FutTransform for SecureChannel<R, S>
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send,
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

    fn transform(
        &mut self,
        input: (Option<PublicKey>, ConnPairVec),
    ) -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> {
        let (opt_expected_remote, conn_pair) = input;
        let (sender, receiver) = conn_pair.split();

        let c_spawner = self.spawner.clone();
        Box::pin(async move {
            create_secure_channel(
                sender,
                receiver,
                self.identity_client.clone(),
                opt_expected_remote.clone(),
                self.rng.clone(),
                self.timer_client.clone(),
                self.ticks_to_rekey,
                c_spawner,
            )
            .await
            .ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::oneshot;
    // use futures::executor::LocalPool;
    use futures::task::SpawnExt;
    use futures::Future;

    use timer::create_timer_incoming;

    use crypto::identity::{Identity, SoftwareEd25519Identity};
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;
    use identity::{create_identity, IdentityClient};

    use proto::crypto::PrivateKey;

    use common::test_executor::TestExecutor;

    async fn secure_channel1(
        fut_sc: impl Future<Output = Result<(PublicKey, ConnPairVec), SecureChannelError>> + 'static,
        mut tick_sender: mpsc::Sender<()>,
        output_sender: oneshot::Sender<bool>,
        test_executor: TestExecutor,
    ) {
        let (_public_key, conn_pair_vec) = fut_sc.await.unwrap();
        let (mut sender, mut receiver) = conn_pair_vec.split();
        sender.send(vec![0, 1, 2, 3, 4, 5]).await.unwrap();
        let data = receiver.next().await.unwrap();
        assert_eq!(data, vec![5, 4, 3]);

        // Move time forward, to cause rekeying:
        for _ in 0_usize..20 {
            tick_sender.send(()).await.unwrap();
            test_executor.wait().await;
        }
        sender.send(vec![0, 1, 2]).await.unwrap();

        output_sender.send(true).unwrap();
    }

    async fn secure_channel2(
        fut_sc: impl Future<Output = Result<(PublicKey, ConnPairVec), SecureChannelError>> + 'static,
        _tick_sender: mpsc::Sender<()>,
        output_sender: oneshot::Sender<bool>,
    ) {
        let (_public_key, conn_pair_vec) = fut_sc.await.unwrap();
        let (mut sender, mut receiver) = conn_pair_vec.split();
        let data = receiver.next().await.unwrap();
        assert_eq!(data, vec![0, 1, 2, 3, 4, 5]);
        sender.send(vec![5, 4, 3]).await.unwrap();

        let data = receiver.next().await.unwrap();
        assert_eq!(data, vec![0, 1, 2]);

        output_sender.send(true).unwrap();
    }

    #[test]
    fn test_secure_channel_basic() {
        // Start the Identity service:
        let test_executor = TestExecutor::new();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, test_executor.clone()).unwrap();

        let rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = PrivateKey::rand_gen(&rng1);
        let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
        let public_key1 = identity1.get_public_key();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);

        let rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = PrivateKey::rand_gen(&rng2);
        let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
        let public_key2 = identity2.get_public_key();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);

        test_executor
            .spawn(identity_server1.then(|_| future::ready(())))
            .unwrap();
        test_executor
            .spawn(identity_server2.then(|_| future::ready(())))
            .unwrap();

        let (sender1, receiver2) = mpsc::channel::<Vec<u8>>(1);
        let (sender2, receiver1) = mpsc::channel::<Vec<u8>>(1);

        let ticks_to_rekey: usize = 16;

        let fut_sc1 = create_secure_channel(
            sender1.sink_map_err(|_| ()),
            receiver1,
            identity_client1,
            Some(public_key2),
            rng1.clone(),
            timer_client.clone(),
            ticks_to_rekey,
            test_executor.clone(),
        );

        let fut_sc2 = create_secure_channel(
            sender2.sink_map_err(|_| ()),
            receiver2,
            identity_client2,
            Some(public_key1),
            rng2.clone(),
            timer_client.clone(),
            ticks_to_rekey,
            test_executor.clone(),
        );

        let (output_sender1, output_receiver1) = oneshot::channel::<bool>();
        let (output_sender2, output_receiver2) = oneshot::channel::<bool>();

        test_executor
            .spawn(secure_channel1(
                fut_sc1,
                tick_sender.clone(),
                output_sender1,
                test_executor.clone(),
            ))
            .unwrap();

        test_executor
            .spawn(secure_channel2(
                fut_sc2,
                tick_sender.clone(),
                output_sender2,
            ))
            .unwrap();

        assert_eq!(test_executor.run(output_receiver1).output(), Some(Ok(true)));
        assert_eq!(test_executor.run(output_receiver2).output(), Some(Ok(true)));

        // assert_eq!(true, LocalPool::new().run_until(output_receiver1).unwrap());
        // assert_eq!(true, LocalPool::new().run_until(output_receiver2).unwrap());
    }
}
