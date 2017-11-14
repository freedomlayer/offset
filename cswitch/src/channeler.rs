
extern crate futures;

use std::collections::HashMap;
use std::mem;

use self::futures::{Poll, Async};
use self::futures::future::{Future};
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;


use ::identity::PublicKey;
use ::inner_messages::{FromTimer, ChannelerToNetworker,
    NetworkerToChanneler, ToSecurityModule, FromSecurityModule,
    ChannelerNeighborInfo};
use ::close_handle::{CloseHandle, create_close_handle};



enum ChannelerError {
    CloseReceiverCanceled,
    SendCloseNotificationFailed
}

/// A future that resolves when a TCP connection is established.
struct PendingConnection {
}

struct ChannelerConnection {
    // Should contain inner state:
    // - Last random value?
}


enum ChannelerState {
    ReadClose,
    HandleClose,
    ReadTimer,
    ReadNetworker,
    ReadSecurityModule,
    PollPendingConnection,
    ReadConnectionMessage(usize),
    HandleConnectionMessage(usize),
    Closed,
}

struct Channeler {
    timer_receiver: mpsc::Receiver<FromTimer>, 
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    security_module_sender: mpsc::Sender<ToSecurityModule>,
    security_module_receiver: mpsc::Receiver<FromSecurityModule>,

    close_sender_opt: Option<oneshot::Sender<()>>,
    close_receiver: oneshot::Receiver<()>,

    pending_connections: Vec<PendingConnection>,
    unverified_connections: Vec<ChannelerConnection>,
    active_connections: HashMap<PublicKey, Vec<ChannelerConnection>>,
    neighbors_info: HashMap<PublicKey, ChannelerNeighborInfo>,

    state: ChannelerState,
}


impl Channeler {
    fn create(timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>,
            close_sender: oneshot::Sender<()>,
            close_receiver: oneshot::Receiver<()>) -> Self {

        Channeler {
            timer_receiver,
            networker_sender,
            networker_receiver,
            security_module_sender,
            security_module_receiver,
            close_sender_opt: Some(close_sender),
            close_receiver,
            pending_connections: Vec::new(),
            unverified_connections: Vec::new(),
            active_connections: HashMap::new(),
            neighbors_info: HashMap::new(),
            state: ChannelerState::ReadTimer,
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
}

impl Future for Channeler {
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match mem::replace(&mut self.state, ChannelerState::Closed) {
                ChannelerState::Closed => panic!("Invalid state!"),
                ChannelerState::ReadClose => {
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
                    // TODO: 
                    // - If enough time has passed, open a new connection in cases where the
                    //  amount of connections is too small or 0.
                    // - Send connection keepalives?
                    // - Close connections that didn't send a keepalive?
                },

                ChannelerState::ReadNetworker => {},

                ChannelerState::ReadSecurityModule => {},
                ChannelerState::PollPendingConnection => {},
                ChannelerState::ReadConnectionMessage(i) => {},
                ChannelerState::HandleConnectionMessage(i) => {},
            }
        }
    }
}

