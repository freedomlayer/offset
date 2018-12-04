use std::iter;
use std::marker::Unpin;
use core::pin::Pin;

use keepalive::keepalive_channel;
use futures::{future, Future, FutureExt, stream, 
    Stream, StreamExt, Sink, SinkExt,
    select};
use futures::task::Spawn;

use crypto::identity::PublicKey;
use timer::{TimerTick, TimerClient};
use timer::utils::future_timeout;
use utils::int_convert::usize_to_u64;

use proto::relay::messages::{InitConnection, RejectConnection, IncomingConnection};
use super::types::{IncomingConn, IncomingConnInner, 
    IncomingListen, IncomingAccept, IncomingConnect};
use proto::relay::serialize::{serialize_incoming_connection, deserialize_reject_connection,
                             deserialize_init_connection};



fn dispatch_conn<M,K,KE,TS>(receiver: M, sender: K, 
                         public_key: PublicKey, 
                         first_msg: Vec<u8>,
                         timer_stream: TS,
                         keepalive_ticks: usize,
                         spawner: impl Spawn) 
    -> Option<IncomingConn<impl Stream<Item=RejectConnection> + Unpin,
                              impl Sink<SinkItem=IncomingConnection,SinkError=()> + Unpin,
                              impl Stream<Item=Vec<u8>> + Unpin,
                              impl Sink<SinkItem=Vec<u8>,SinkError=()> + Unpin,
                              impl Stream<Item=Vec<u8>> + Unpin,
                              impl Sink<SinkItem=Vec<u8>,SinkError=()> + Unpin>>
where
    M: Stream<Item=Vec<u8>> + Unpin + Send + 'static,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin + Send + 'static,
    TS: Stream<Item=TimerTick> + Unpin + Send + 'static,
{

    let (sender, receiver) = keepalive_channel(sender,
                      receiver,
                      timer_stream,
                      keepalive_ticks,
                      spawner);

    let sender = sender.sink_map_err(|_| ());
    let inner = match deserialize_init_connection(&first_msg).ok()? {
        InitConnection::Listen => {
            IncomingConnInner::Listen(IncomingListen {
                receiver: receiver.map(|data| deserialize_reject_connection(&data))
                    .take_while(|res| future::ready(res.is_ok()))
                    .map(|res| res.unwrap()),
                sender: sender.with(|msg| future::ready(Ok(serialize_incoming_connection(&msg)))),
            })
        },
        InitConnection::Accept(accept_public_key) => 
            IncomingConnInner::Accept(IncomingAccept {
                receiver,
                sender,
                accept_public_key,
            }),
        InitConnection::Connect(connect_public_key) => 
            IncomingConnInner::Connect(IncomingConnect {
                receiver,
                sender,
                connect_public_key,
            }),
    };

    Some(IncomingConn {
        public_key,
        inner,
    })
}

async fn process_conn<M,K,KE>(mut receiver: M, 
                sender: K, 
                public_key: PublicKey,
                mut timer_client: TimerClient,
                keepalive_ticks: usize,
                conn_timeout_ticks: usize,
                spawner: impl Spawn) -> Option<
                             IncomingConn<impl Stream<Item=RejectConnection> + Unpin,
                                          impl Sink<SinkItem=IncomingConnection,SinkError=()> + Unpin,
                                          impl Stream<Item=Vec<u8>> + Unpin,
                                          impl Sink<SinkItem=Vec<u8>,SinkError=()> + Unpin,
                                          impl Stream<Item=Vec<u8>> + Unpin,
                                          impl Sink<SinkItem=Vec<u8>,SinkError=()> + Unpin>>

where
    M: Stream<Item=Vec<u8>> + Unpin + Send + 'static,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin + Send + 'static,
{

    let mut c_timer_client = timer_client.clone();
    let mut fut_receiver = Box::pinned(async move {
        if let Some(first_msg) = await!(receiver.next()) {
            let timer_stream = await!(c_timer_client.request_timer_stream()).unwrap();
            dispatch_conn(receiver, sender, public_key, first_msg, 
                         timer_stream, keepalive_ticks, spawner)
        } else {
            None
        }
    });

    let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
    await!(future_timeout(fut_receiver, timer_stream, conn_timeout_ticks))?
}



/// Process incoming connections
/// For each connection obtain the first message, and prepare the correct type according to this
/// first messages.
/// If waiting for the first message takes too long, discard the connection.
pub fn conn_processor<T,M,K,KE>(mut timer_client: TimerClient,
                    incoming_conns: T,
                    keepalive_ticks: usize,
                    conn_timeout_ticks: usize,
                    spawner: impl Spawn + Clone) -> impl Stream<
                        Item=IncomingConn<impl Stream<Item=RejectConnection>,
                                          impl Sink<SinkItem=IncomingConnection,SinkError=()>,
                                          impl Stream<Item=Vec<u8>>,
                                          impl Sink<SinkItem=Vec<u8>,SinkError=()>,
                                          impl Stream<Item=Vec<u8>>,
                                          impl Sink<SinkItem=Vec<u8>,SinkError=()>>>
where
    T: Stream<Item=(M, K, PublicKey)> + Unpin,
    M: Stream<Item=Vec<u8>> + Unpin + Send + 'static,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin + Send + 'static,
{

    incoming_conns
        .map(move |(receiver, sender, public_key)| {
            process_conn(receiver, sender, public_key, 
                         timer_client.clone(), keepalive_ticks, 
                         conn_timeout_ticks, spawner.clone())
        })
        .filter_map(|opt_conn| opt_conn)

}


#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::{mpsc, oneshot};
    use futures::Future;
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
    use timer::create_timer_incoming;
    use utils::async_test_utils::{receive, ReceiveError};

    use proto::relay::serialize::serialize_init_connection;

    async fn task_dispatch_conn_basic(spawner: impl Spawn + Clone) {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();
        let keepalive_ticks = 8;

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg,
                                          timer_stream,
                                          keepalive_ticks,
                                          spawner.clone()).unwrap();

        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Listen(_incoming_listen) => {},
            _ => panic!("Wrong IncomingConnInner"),
        };

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let accept_public_key = PublicKey::from(&[0x22; PUBLIC_KEY_LEN]);
        let first_msg = InitConnection::Accept(accept_public_key.clone());
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg,
                                          timer_stream,
                                          keepalive_ticks,
                                          spawner.clone()).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Accept(incoming_accept) => 
                assert_eq!(incoming_accept.accept_public_key, accept_public_key),
            _ => panic!("Wrong IncomingConnInner"),
        };

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let connect_public_key = PublicKey::from(&[0x33; PUBLIC_KEY_LEN]);
        let first_msg = InitConnection::Connect(connect_public_key.clone());
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg,
                                          timer_stream,
                                          keepalive_ticks,
                                          spawner.clone()).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Connect(incoming_connect) => 
                assert_eq!(incoming_connect.connect_public_key, connect_public_key),
            _ => panic!("Wrong IncomingConnInner"),
        };
    }
    
    #[test]
    fn test_dispatch_conn_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_dispatch_conn_basic(thread_pool.clone()));
    }

    async fn task_dispatch_conn_invalid_first_msg(spawner: impl Spawn + Clone) {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();
        let keepalive_ticks = 8;

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let ser_first_msg = b"This is an invalid message".to_vec();
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let res = dispatch_conn(receiver, 
                                sender.sink_map_err(|_| ()), 
                                public_key.clone(), ser_first_msg,
                                timer_stream,
                                keepalive_ticks,
                                spawner);
        assert!(res.is_none());
    }

    #[test]
    fn test_dispatch_conn_invalid_first_msg() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_dispatch_conn_invalid_first_msg(thread_pool.clone()));
    }

    #[test]
    fn test_conn_processor_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();

        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let (local_sender, _remote_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut remote_sender, local_receiver) = mpsc::channel::<Vec<u8>>(0);

        let incoming_conns = stream::iter::<_>(
            vec![(local_receiver, local_sender, public_key.clone())]);

        let conn_timeout_ticks = 16;
        let keepalive_ticks = 8;
        let processed_conns = conn_processor(timer_client, 
                       incoming_conns, 
                       keepalive_ticks,
                       conn_timeout_ticks,
                       thread_pool.clone());


        let processed_conns = Box::pinned(processed_conns);


        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        thread_pool.spawn(
            async move {
                await!(remote_sender
                     .send(ser_first_msg)
                     .map(|res| {
                         match res {
                             Ok(_remote_sender) => (),
                             Err(_) => unreachable!("Sending first message failed!"),
                         }
                     }))
            }
        );


        let (conn, processed_conns) =  thread_pool.run(receive(processed_conns)).unwrap();
        assert_eq!(conn.public_key, public_key);
        match conn.inner {
            IncomingConnInner::Listen(_incoming_listen) => {},
            _ => panic!("Incorrect processed conn"),
        };

        assert!(thread_pool.run(receive(processed_conns)).is_none());
    }

}
