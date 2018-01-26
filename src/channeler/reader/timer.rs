use std::{mem, cell::RefCell, rc::Rc};

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use ring::rand::SecureRandom;

use tokio_core::reactor::Handle;

use security_module::client::SecurityModuleClient;
use utils::{CloseHandle, StreamMediator};

use channeler::channel::{Channel, ChannelError};
use channeler::messages::{ChannelerToNetworker, ToChannel};
use channeler::types::{ChannelerNeighbor, NeighborsTable};
use timer::messages::FromTimer;

const CONN_ATTEMPT_TICKS: usize = 120;

#[derive(Debug)]
pub enum TimerReaderError {
    // RecvFromTimerFailed,
    RemoteCloseHandleCanceled,
    // SendToNetworkerFailed,
}

// TODO CR: Maybe we should produce the TimerReaderError::RemoteCloseHandleClosed at the right
// place, instead of having it here?
// I could understand this kind of error conversion if it meant something generic, but it seems
// like this conversion means something local to a specific place in the code. It will probably be
// easier to understand what happens here if this code will be moved to the relevant place.
//
// As an example, what if we had more than one oneshots used inside the TimerReader in the future?
// For one of those oneshots we will have TimerReaderError::RemoteOneshot1Closed, and for the
// other one we will have TimerReaderError::RemoteOneshot2Closed. We won't be able to use the impl
// From<oneshot::Canceled> for TimerReaderError> in that case.
impl From<oneshot::Canceled> for TimerReaderError {
    fn from(_e: oneshot::Canceled) -> TimerReaderError {
        TimerReaderError::RemoteCloseHandleCanceled
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct TimerReader<SR> {
    handle:    Handle,
    neighbors: Rc<RefCell<NeighborsTable>>,
    sm_client: SecurityModuleClient,

    secure_rng: Rc<SR>,

    timer_receiver:   mpsc::Receiver<FromTimer>,
    timer_dispatcher: StreamMediator<mpsc::Receiver<FromTimer>>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,

    // For reporting the closing event
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

/// Whether needs to retry connection.
fn needs_retry(neighbor: &ChannelerNeighbor) -> bool {
    if neighbor.info.socket_addr.is_some() && neighbor.num_pending == 0 && neighbor.channel.is_none() {
        return neighbor.retry_ticks == 0;
    }

    false
}

impl<SR: SecureRandom + 'static> TimerReader<SR> {
    pub fn new(
        timer_receiver: mpsc::Receiver<FromTimer>,
        handle: &Handle,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client: SecurityModuleClient,
        neighbors: Rc<RefCell<NeighborsTable>>,
        secure_rng: Rc<SR>,
    ) -> (CloseHandle, TimerReader<SR>) {
        let (close_handle, (close_tx, close_rx)) = CloseHandle::new();

        let buffer_size = 10;
        let mut mediator = StreamMediator::new(timer_receiver, buffer_size, handle);

        let timer_reader = TimerReader {
            handle: handle.clone(),
            sm_client,
            neighbors,
            close_tx: Some(close_tx),
            close_rx,
            secure_rng,
            networker_sender,
            timer_receiver: mediator.get_stream(buffer_size).expect("failed to sub the timer"),
            timer_dispatcher: mediator,
        };

        (close_handle, timer_reader)
    }

    /// Spawn a connection attempt to any neighbor for which we are the active side in the
    /// relationship. We attempt to connect to every neighbor every once in a while.
    fn reconnect(&mut self) {
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = Rc::clone(&self.neighbors);
        let sm_client_for_task = self.sm_client.clone();
        let networker_sender_for_task = self.networker_sender.clone();

        // TODO: Extract the following part as a function
        let mut need_retry = Vec::new();
        {
            for (neighbor_public_key, mut neighbor) in &mut self.neighbors.borrow_mut().iter_mut() {
                // TODO: Use a separate function to do this.
                if needs_retry(neighbor) {
                    let addr = neighbor.info.socket_addr.unwrap();
                    need_retry.push((addr, neighbor_public_key.clone(), 0));
                    // TODO CR: Is this the right place to increase num_pending?
                    // Maybe we should do it right before we open the new Channel?
                    // I need to read the rest of the code in this file to be sure about this.
                    neighbor.num_pending += 1;
                }
                if neighbor.retry_ticks == 0 {
                    neighbor.retry_ticks = CONN_ATTEMPT_TICKS;
                } else {
                    neighbor.retry_ticks -= 1;
                }
            }
        }

        for (addr, neighbor_public_key, _channel_index) in need_retry {
            let neighbors = Rc::clone(&neighbors_for_task);
            // let mut networker_sender = networker_sender_for_task.clone();

            let buffer_size = 10;
            let new_channel = Channel::connect(
                &addr,
                &handle_for_task,
                &neighbor_public_key,
                // channel_index,
                // Rc::clone(&neighbors_for_task),
                &networker_sender_for_task,
                &sm_client_for_task,
                self.timer_dispatcher.get_stream(buffer_size).expect("failed to sub timer"),
                Rc::clone(&self.secure_rng),
            ).then(move |conn_result| {
                match neighbors.borrow_mut().get_mut(&neighbor_public_key) {
                    None => Err(ChannelError::Closed("Can't find this neighbor")),
                    Some(neighbor) => {
                        neighbor.num_pending -= 1;

                        let (channel_tx, channel) = conn_result?;
                        neighbor.channel = Some(channel_tx);
                        Ok(channel)
                    },
                }
            });

            handle_for_task.spawn(
                new_channel
                    .map_err(|e| {
                        error!("failed to initialize a new connection: {:?}", e);
                    })
                    .and_then(|channel| {
                        channel.map_err(|e| {
                            warn!("channel closed: {:?}", e);
                        })
                    }),
            );
        }
    }

    fn broadcast_tick(&self) {
        for (_neighbor_public_key, mut neighbor) in &mut self.neighbors.borrow_mut().iter_mut() {
            if let Some(ref mut channel_tx) = neighbor.channel {
                if channel_tx.try_send(ToChannel::TimeTick).is_err() {
                    neighbor.channel = None;
                }
            }
        }
    }

    // TODO: Consume all message before closing actually.
    // TODO CR: What messages do we need to consume here? The buffered messages inside the Streams?
    fn close(&mut self) {
        self.timer_receiver.close();
        match mem::replace(&mut self.close_tx, None) {
            None => {
                error!("call close after close sender consumed, something go wrong");
            },
            Some(close_sender) => {
                if close_sender.send(()).is_err() {
                    error!("remote close handle deallocated, something may go wrong");
                }
            },
        }
    }
}

impl<SR: SecureRandom + 'static> Future for TimerReader<SR> {
    type Item = ();
    type Error = TimerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        // Check if we received a closing request:
        match self.close_rx.poll()? {
            Async::NotReady => (),
            Async::Ready(()) => {
                debug!("received the closing request, closing");

                self.close();

                return Ok(Async::Ready(()));
            },
        }

        loop {
            match self.timer_receiver.poll() {
                Err(_) => {
                    error!("inner receiver error, closing");

                    self.close();

                    return Ok(Async::Ready(()));
                },
                Ok(item) => match item {
                    Async::NotReady => {
                        return Ok(Async::NotReady);
                    },
                    Async::Ready(None) => {
                        error!("inner receiver closed, closing");

                        self.close();

                        return Ok(Async::Ready(()));
                    },
                    Async::Ready(Some(FromTimer::TimeTick)) => {
                        self.reconnect();
                        self.broadcast_tick();
                    },
                },
            }
        }
    }
}
