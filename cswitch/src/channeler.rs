extern crate futures;
// extern crate rand;
extern crate tokio_core;
extern crate tokio_io;
extern crate ring;


use std::collections::{HashMap};
use std::mem;
use std::cell::RefCell;

// use self::rand::Rng;

use self::futures::{Stream, Poll, Async, AsyncSink, StartSend};
use self::futures::future::{Future, loop_fn, Loop, LoopFn};
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;
use self::tokio_core::net::TcpStream;
use self::tokio_core::reactor::Handle;
use self::tokio_io::AsyncRead;
use self::ring::rand::SecureRandom;


use ::crypto::identity::PublicKey;
use ::inner_messages::{FromTimer, ChannelerToNetworker,
    NetworkerToChanneler, ToSecurityModule, FromSecurityModule,
    ChannelerNeighborInfo, ServerType};
use ::close_handle::{CloseHandle, create_close_handle};
use ::crypto::rand_values::{RandValuesStore, RandValue};
use ::prefix_frame_codec::PrefixFrameCodec;

const NUM_RAND_VALUES: usize = 16;
const RAND_VALUE_TICKS: usize = 20;


const KEEP_ALIVE_TICKS: usize = 15;
const CONN_ATTEMPT_TICKS: usize = 120;



enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed,
    NetworkerClosed, // TODO: We should probably start closing too.
    NetworkerPollError,
    TimerClosed, // TODO: We should probably start closing too.
    TimerPollError,
}


struct Channel {
    ticks_to_receive_keep_alive: usize,
    ticks_to_send_keep_alive: usize,
    // TODO:
    // - Sender
    // - Receiver
}

struct ChannelerNeighbor {
    info: ChannelerNeighborInfo,
    last_remote_rand_value: Option<RandValue>,
    channels: Vec<Channel>,
    ticks_to_next_conn_attempt: usize,
    num_pending_out_conn: usize,
}


/*
enum ChannelerState {
    ReadClose,
    HandleClose,
    ReadTimer,
    ReadNetworker,
    HandleNetworker(NetworkerToChanneler),
    ReadSecurityModule,
    PollPendingConnection,
    ReadConnectionMessage(usize),
    HandleConnectionMessage(usize),
    // ReadListenSocket,
    Closed,
}
*/

struct InnerChanneler<'a,R:'a> {
    handle: Handle,
    timer_receiver: mpsc::Receiver<FromTimer>, 
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    security_module_sender: mpsc::Sender<ToSecurityModule>,
    security_module_receiver: mpsc::Receiver<FromSecurityModule>,
    crypt_rng: &'a R,

    close_sender_opt: Option<oneshot::Sender<()>>,
    close_receiver: Option<oneshot::Receiver<()>>,

    rand_values_store: RandValuesStore,

    neighbors: HashMap<PublicKey, ChannelerNeighbor>,
    server_type: ServerType,

    // state: ChannelerState,
}


struct Channeler<'a, R:'a> {
    inner_channeler: RefCell<InnerChanneler<'a,R>>,
}


fn create_channeler_future<'a,R: SecureRandom>(handle: &Handle, 
            timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>,
            crypt_rng: &'a R,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> 
                impl Future<Item=(), Error=ChannelerError> + 'a {

    let rand_values_store = RandValuesStore::new(
        crypt_rng, RAND_VALUE_TICKS, NUM_RAND_VALUES);

    let inner_channeler = RefCell::new(InnerChanneler {
        handle: handle.clone(),
        timer_receiver,
        networker_sender,
        networker_receiver,
        security_module_sender,
        security_module_receiver,
        crypt_rng,
        close_sender_opt: Some(close_sender),
        close_receiver: Some(close_receiver),
        rand_values_store,
        neighbors: HashMap::new(),
        server_type: ServerType::PrivateServer,
    });

    loop_fn(inner_channeler, |inner_channeler| {
        // TODO: Start all the tasks here:
        //
        
        // Wait for closing:
        inner_channeler.borrow_mut().close_receiver.take().unwrap()
            .map_err(|oneshot::Canceled| {
                warn!("Remote closing handle was canceled!");
                ChannelerError::CloseReceiverCanceled
            })
            .and_then(|()| {
                // TODO: Send close requests to all tasks here?
                Ok(Loop::Break(()))
            })
    })
}

struct NetworkerReader;
struct TimerReader;
struct SecurityModuleClient;


fn create_timer_reader_future<'a,R>(inner_channeler: RefCell<InnerChanneler<'a,R>>) 
    -> impl Future<Item=(), Error=ChannelerError> + 'a {

    loop_fn(TimerReader, move |timer_reader| {
        let mut b_inner_channeler = inner_channeler.borrow_mut();

        for (_, mut neighbor) in &mut b_inner_channeler.neighbors {
            let socket_addr = match neighbor.info.neighbor_address.socket_addr {
                None => continue,
                Some(socket_addr) => socket_addr,
            };
            // If there are already some attempts to add connections, 
            // we don't try to add a new connection ourselves.
            if neighbor.num_pending_out_conn > 0 {
                continue;
            }
            if neighbor.channels.len() == 0 {
                // This is an inactive neighbor.
                neighbor.ticks_to_next_conn_attempt -= 1;
                if neighbor.ticks_to_next_conn_attempt == 0 {
                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
                } else {
                    continue;
                }
            }
            /*
            // Attempt a connection:
            TcpStream::connect(&socket_addr, &self.handle)
                .and_then(|stream| {
                    let (sink, stream) = stream.framed(PrefixFrameCodec::new()).split();

                    // TODO: Binary deserializtion of Channeler to Channeler messages.
            */
        }
        Ok(Loop::Break(()))
    })
}


/*
impl<'a,R:SecureRandom> Channeler<'a,R> {
    fn new(handle: &Handle, 
            timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>,
            crypt_rng: &'a R,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> Self {



        let inner_future = loop_fn(inner_channeler, channeler_loop);

        Channeler {
            inner_future,
            inner_channeler,
        }
    }
}

    fn check_should_close(&mut self) -> Result<bool, ChannelerError> {
        match self.close_receiver.poll() {
            Ok(Async::Ready(())) => {
                // We were asked to close
                Ok(true)
            },
            Ok(Async::NotReady) => Ok(false),
            Err(_e) => return Err(ChannelerError::CloseReceiverCanceled),
        }
    }

    /// Tell the handle that we are done closing.
    fn send_close_notification(&mut self) -> Result<(), ChannelerError> {
        let close_sender = match self.close_sender_opt.take() {
            None => panic!("Close notification already sent!"),
            Some(close_sender) => close_sender,
        };

        match close_sender.send(()) {
            Ok(()) => Ok(()),
            Err(_) => Err(ChannelerError::SendCloseNotificationFailed),
        }
    }

    fn handle_networker_message(&mut self, message: NetworkerToChanneler) ->
        StartSend<NetworkerToChanneler, ChannelerError> {

        match message {
            NetworkerToChanneler::SendChannelMessage { 
                neighbor_public_key, message_content } => {
                // TODO: Attempt to send a message to some TCP connections leading to
                // the requested neighbor. The chosen Sink could be not ready.
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::AddNeighborRelation { neighbor_info } => {
                let neighbor_public_key = neighbor_info.neighbor_public_key.clone();
                if self.neighbors.contains_key(&neighbor_public_key) {
                    warn!("Neighbor with public key {:?} already exists", 
                          neighbor_public_key);
                }
                self.neighbors.insert(neighbor_public_key, ChannelerNeighbor {
                    info: neighbor_info,
                    last_remote_rand_value: None,
                    channels: Vec::new(),
                    pending_channels: Vec::new(),
                    pending_out_conn: None,
                    ticks_to_next_conn_attempt: CONN_ATTEMPT_TICKS,
                });
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::RemoveNeighborRelation { neighbor_public_key } => {
                match self.neighbors.remove(&neighbor_public_key) {
                    None => warn!("Attempt to remove a nonexistent neighbor \
                        relation with public key {:?}", neighbor_public_key),
                    _ => {},
                };
                // TODO: Possibly close all connections here.

                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::SetMaxChannels 
                { neighbor_public_key, max_channels } => {
                match self.neighbors.get_mut(&neighbor_public_key) {
                    None => warn!("Attempt to change max_channels for a \
                        nonexistent neighbor relation with public key {:?}",
                        neighbor_public_key),
                    Some(neighbor) => {
                        neighbor.info.max_channels = max_channels;
                    },
                };
                Ok(AsyncSink::Ready)
            },
            NetworkerToChanneler::SetServerType(server_type) => {
                self.server_type = server_type;
                Ok(AsyncSink::Ready)
            },
        }
    }

    /// Handle the passage of time.
    /// We measure time by time ticks.
    fn handle_time_tick(&mut self) -> Result<(), ChannelerError> {
        self.rand_values_store.time_tick(self.crypt_rng);

        // TODO: 
        
        for (_, mut neighbor) in &mut self.neighbors {
            let socket_addr = match neighbor.info.neighbor_address.socket_addr {
                None => continue,
                Some(socket_addr) => socket_addr,
            };
            // If there are already some attempts to add connections, 
            // we don't try to add a new connection ourselves.
            if neighbor.pending_out_conn.is_some() {
                continue;
            }
            if neighbor.pending_channels.len() > 0 {
                continue;
            }
            if neighbor.channels.len() == 0 {
                // This is an inactive neighbor.
                neighbor.ticks_to_next_conn_attempt -= 1;
                if neighbor.ticks_to_next_conn_attempt == 0 {
                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
                } else {
                    continue;
                }
            }
            /*
            // Attempt a connection:
            TcpStream::connect(&socket_addr, &self.handle)
                .and_then(|stream| {
                    let (sink, stream) = stream.framed(PrefixFrameCodec::new()).split();

                    // TODO: Binary deserializtion of Channeler to Channeler messages.
                    // - Write adapters to sink and stream.
                    // - Store connection future at pending_out_conn. It will later resolve to a
                    //      sink and a stream.
                    
                });

            // neighbor.pending_out_conn = Some();
            */
        }
        

        // - If enough time has passed, open a new connection in cases where the
        //  amount of connections is too small or 0.
        // Use TcpStream::connect to create a new connection.




        // - Send connection keepalives?

        // - Close connections that didn't send a keepalive?
        Ok(())
    }
}
*/

/*
impl<'a, R:SecureRandom + 'a> Future for Channeler<'a, R> {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner_future.poll()
        let mut visited_read_close = false;
        loop {
            match mem::replace(&mut self.state, ChannelerState::Closed) {
                ChannelerState::Closed => panic!("Invalid state!"),
                ChannelerState::ReadClose => {
                    let visited_read_close = if visited_read_close {
                        return Ok(Async::NotReady);
                    } else {
                        true
                    };

                    if self.check_should_close()? {
                        self.state = ChannelerState::HandleClose;
                        continue;
                    }
                },
                ChannelerState::HandleClose => {
                    // TODO: Close stuff here.
                    self.send_close_notification()?;
                    self.state = ChannelerState::Closed;
                    return Ok(Async::Ready(()));

                },
                ChannelerState::ReadTimer => {
                    match self.timer_receiver.poll() {
                        Ok(Async::Ready(Some(FromTimer::TimeTick))) => {
                            self.handle_time_tick()?;
                        },
                        Ok(Async::Ready(None)) => return Err(ChannelerError::TimerClosed),
                        Ok(Async::NotReady) => {},
                        Err(()) => return Err(ChannelerError::TimerPollError),
                    };

                    self.state = ChannelerState::ReadNetworker;
                    continue;
                },
                ChannelerState::ReadNetworker => {
                    match self.networker_receiver.poll() {
                        Ok(Async::Ready(Some(msg))) => {
                            self.state = ChannelerState::HandleNetworker(msg);
                            continue;
                        },
                        Ok(Async::Ready(None)) => {
                            return Err(ChannelerError::NetworkerClosed);
                        },
                        Ok(Async::NotReady) => {},
                        Err(()) => return Err(ChannelerError::NetworkerPollError),
                    };

                    self.state = ChannelerState::ReadSecurityModule;
                    continue;
                },

                ChannelerState::HandleNetworker(message) => {
                    self.state = match self.handle_networker_message(message)? {
                        AsyncSink::Ready => ChannelerState::ReadSecurityModule,
                        AsyncSink::NotReady(message) => 
                            ChannelerState::HandleNetworker(message),
                    };
                    continue;
                }


                ChannelerState::ReadSecurityModule => {
                    // TODO
                },
                ChannelerState::PollPendingConnection => {
                    // TODO
                },
                ChannelerState::ReadConnectionMessage(i) => {
                    // TODO
                },
                ChannelerState::HandleConnectionMessage(i) => {
                    // TODO
                },
            }
        }
    }
}
*/

