use std::iter;
use std::marker::Unpin;
use core::pin::Pin;

use futures::{future, Future, FutureExt, stream, 
    Stream, StreamExt, Sink, SinkExt,
    select};

use crypto::identity::PublicKey;
use timer::{TimerTick, TimerClient};
use utils::int_convert::usize_to_u64;

use proto::relay::messages::{InitConnection, TunnelMessage, 
    RelayListenIn, RelayListenOut};
use super::types::{IncomingConn, IncomingConnInner, 
    IncomingListen, IncomingAccept, IncomingConnect};
use proto::relay::serialize::{deserialize_init_connection, deserialize_relay_listen_in,
                        serialize_relay_listen_out, serialize_tunnel_message,
                        deserialize_tunnel_message};



fn dispatch_conn<M,K,KE>(receiver: M, sender: K, public_key: PublicKey, first_msg: Vec<u8>) 
    -> Option<IncomingConn<impl Stream<Item=RelayListenIn> + Unpin,
                              impl Sink<SinkItem=RelayListenOut,SinkError=()> + Unpin,
                              impl Stream<Item=TunnelMessage> + Unpin,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()> + Unpin,
                              impl Stream<Item=TunnelMessage> + Unpin,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()> + Unpin>>
where
    M: Stream<Item=Vec<u8>> + Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin,
{
    let sender = sender.sink_map_err(|_| ());
    let inner = match deserialize_init_connection(&first_msg).ok()? {
        InitConnection::Listen => {
            IncomingConnInner::Listen(IncomingListen {
                receiver: receiver.map(|data| deserialize_relay_listen_in(&data))
                    .take_while(|res| future::ready(res.is_ok()))
                    .map(|res| res.unwrap()),
                sender: sender.with(|msg| future::ready(Ok(serialize_relay_listen_out(&msg)))),
            })
        },
        InitConnection::Accept(accept_public_key) => 
            IncomingConnInner::Accept(IncomingAccept {
                receiver: receiver.map(|data| deserialize_tunnel_message(&data))
                    .take_while(|res| future::ready(res.is_ok()))
                    .map(|res| res.unwrap()),
                sender: sender.with(|msg| future::ready(Ok(serialize_tunnel_message(&msg)))),
                accept_public_key,
            }),
        InitConnection::Connect(connect_public_key) => 
            IncomingConnInner::Connect(IncomingConnect {
                receiver: receiver.map(|data| deserialize_tunnel_message(&data))
                    .take_while(|res| future::ready(res.is_ok()))
                    .map(|res| res.unwrap()),
                sender: sender.with(|msg| future::ready(Ok(serialize_tunnel_message(&msg)))),
                connect_public_key,
            }),
    };

    Some(IncomingConn {
        public_key,
        inner,
    })
}

/*
pub async fn select<T, F1, F2>(mut fut1: F1, mut fut2: F2) -> T
where
    F1: Future<Output=T> + Unpin,
    F2: Future<Output=T> + Unpin,
{
    select! {
        fut1 => fut1,
        fut2 => fut2,
    }
}
*/

fn process_conn<M,K,KE,TS>(receiver: M, 
                sender: K, 
                public_key: PublicKey,
                timer_stream: TS,
                conn_timeout_ticks: usize) -> impl Future<Output=Option<
                             IncomingConn<impl Stream<Item=RelayListenIn> + Unpin,
                                          impl Sink<SinkItem=RelayListenOut,SinkError=()> + Unpin,
                                          impl Stream<Item=TunnelMessage> + Unpin,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()> + Unpin,
                                          impl Stream<Item=TunnelMessage> + Unpin,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()> + Unpin>>>

where
    M: Stream<Item=Vec<u8>> + Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin,
    TS: Stream<Item=TimerTick> + Unpin,
{
    let conn_timeout_ticks = usize_to_u64(conn_timeout_ticks).unwrap();
    let mut fut_receiver = 
        receiver
        .into_future()
        .then(|(opt_first_msg, receiver)| {
            future::ready(match opt_first_msg {
                Some(first_msg) => dispatch_conn(receiver, sender, public_key, first_msg),
                None => None,
            })
        });

    let mut fut_time = timer_stream
        .take(conn_timeout_ticks)
        .for_each(|_| {
            future::ready(())
        })
        .map(|_| {
            None
        });

    // select(fut_receiver, fut_time)
    // NOTE: This select is probably not Unpin. Maybe we need to implement our own?
    async move {
        select! {
            fut_receiver => fut_receiver,
            fut_time => fut_time,
        }
    }
}



/// Process incoming connections
/// For each connection obtain the first message, and prepare the correct type according to this
/// first messages.
/// If waiting for the first message takes too long, discard the connection.
pub fn conn_processor<T,M,K,KE>(timer_client: TimerClient,
                    incoming_conns: T,
                    conn_timeout_ticks: usize) -> impl Stream<
                        Item=IncomingConn<impl Stream<Item=RelayListenIn>,
                                          impl Sink<SinkItem=RelayListenOut,SinkError=()>,
                                          impl Stream<Item=TunnelMessage>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>,
                                          impl Stream<Item=TunnelMessage>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>>>
where
    T: Stream<Item=(M, K, PublicKey)> + Unpin,
    M: Stream<Item=Vec<u8>> + Unpin,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE> + Unpin,
{

    let timer_streams = stream::iter::<_>(iter::repeat(()))
        .then(move |()| timer_client.clone().request_timer_stream())
        .map(|res| res.unwrap());


    incoming_conns
    .zip(timer_streams)
    .map(move |((receiver, sender, public_key), timer_stream)| {
        process_conn(receiver, sender, public_key, 
                     timer_stream, conn_timeout_ticks)
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

    #[test]
    fn test_dispatch_conn_basic() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
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
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
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
        let incoming_conn = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Connect(incoming_connect) => 
                assert_eq!(incoming_connect.connect_public_key, connect_public_key),
            _ => panic!("Wrong IncomingConnInner"),
        };
    }

    #[test]
    fn test_dispatch_conn_invalid_first_msg() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let ser_first_msg = b"This is an invalid message".to_vec();
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let res = dispatch_conn(receiver,
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg);
        assert!(res.is_none());
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
        let processed_conns = conn_processor(timer_client, 
                       incoming_conns, 
                       conn_timeout_ticks);

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

    async fn task_process_conn_timeout(mut spawner: impl Spawn + Clone + 'static) -> Result<(),()> {

        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let (local_sender, mut remote_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut remote_sender, local_receiver) = mpsc::channel::<Vec<u8>>(0);

        let conn_timeout_ticks = 16;
        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();

        let (res_sender, res_receiver) = oneshot::channel();
        let fut_incoming_conn = process_conn(local_receiver, 
                                             local_sender, 
                                             public_key.clone(),
                                             timer_stream,
                                             conn_timeout_ticks);
        spawner.spawn(fut_incoming_conn
            .then(|res| {
                res_sender.send(res).ok().unwrap();
                future::ready(())
            }));

        for _ in 0 .. 16usize {
            await!(tick_sender.send(())).unwrap();
        }

        assert!(await!(remote_receiver.next()).is_none());
        assert!(await!(res_receiver).unwrap().is_none());

        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        let res = await!(remote_sender.send(ser_first_msg));
        assert!(res.is_err());
        Ok(())
    }

    #[test]
    fn test_process_conn_timeout() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_process_conn_timeout(thread_pool.clone())).unwrap();
    }
}
