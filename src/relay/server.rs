#![allow(unused)]
use std::collections::HashMap;
use std::marker::PhantomData;
use futures::{stream, Stream, Sink};
use futures::sync::mpsc;
use futures::prelude::{async, await, async_stream}; 
use crypto::identity::PublicKey;
use timer::TimerClient;

use super::messages::{TunnelMessage, RelayListenIn, RelayListenOut};

struct ConnPair<M,K> {
    receiver: M,
    sender: K,
}

struct HalfTunnel<MT,KT> {
    conn_pair: ConnPair<MT,KT>,
    ticks_to_close: usize,
}

struct Listener<M,K,MT,KT> {
    half_tunnel: HashMap<PublicKey, HalfTunnel<MT,KT>>,
    conn_pair: ConnPair<M,K>,
    ticks_to_close: usize,
    ticks_to_send_keepalive: usize,
}

pub struct RelayServer<M,K,MT,KT> {
    listeners: HashMap<PublicKey, Listener<M,K,MT,KT>>,
    timer_client: TimerClient,
    keepalive_ticks: usize,
}

pub struct IncomingListen {
    receiver: Box<Stream<Item=RelayListenIn, Error=()>>,
    sender: Box<Sink<SinkItem=RelayListenOut, SinkError=()>>,
}

pub struct IncomingAccept {
    receiver: Box<Stream<Item=RelayListenIn, Error=()>>,
    sender: Box<Sink<SinkItem=RelayListenOut, SinkError=()>>,
    accepted_public_key: PublicKey,
}

pub struct IncomingConnect {
    receiver: Box<Stream<Item=RelayListenIn, Error=()>>,
    sender: Box<Sink<SinkItem=RelayListenOut, SinkError=()>>,
    dest_public_key: PublicKey,
}

enum IncomingConnInner {
    Listen(IncomingListen),
    Accept(IncomingAccept),
    Connect(IncomingConnect),
}

struct IncomingConn {
    conn_public_key: PublicKey,
    inner: IncomingConnInner,
}

struct ConnClosed {
    initiator: PublicKey,
    listener: PublicKey,
}


enum RelayServerEvent {
    IncomingConn(IncomingConn),
    ConnClosed(ConnClosed),
}

enum RelayServerError {
}

#[async_stream(item=IncomingConn)]
fn conn_processor<T,M,K>(timer_client: TimerClient,
                    incoming_conns: T,
                    keepalive_ticks: usize) -> Result<(), RelayServerError>
where
    T: Stream<Item=(M, K, PublicKey), Error=()>,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{

    /*
    match await!(incoming_conns.into_future()) {
        Ok((opt_reader_message, ret_reader)) => {
            match opt_reader_message {
                Some(reader_message) => Ok((reader_message, ret_reader)),
                None => return Err(SecureChannelError::ReaderClosed),
            }
        },
        Err(_) => return Err(SecureChannelError::ReaderError),
    }
    */

    unimplemented!();
}


 
#[async]
fn relay_server<T,M,K>(timer_client: TimerClient, 
                incoming_conns: T,
                keepalive_ticks: usize) -> Result<!, RelayServerError> 
where
    T: Stream<Item=(M, K, PublicKey), Error=()>,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    // TODO:
    // check for any event:
    // - Incoming connection 
    //      (sender, receiver) pair an a public key
    //      - Convert the connection into one of three: Listen, Accept or Connect.
    //          (Should be done using a .map() adapter on the Stream).
    //          
    // - A connection was closed
    //      - Remove from data structures
    // - Time tick
    //      - Possibly timeout: Listening conn
    //      - Send keepalive if required to a listening conn.
    unimplemented!();
}
