extern crate futures;
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;

// use self::futures::sync::mpsc::{channel, Sender, Receiver};
// use self::futures::sync::mpsc::{channel, Sender, Receiver};

use ::inner_messages::{ToSecurityModule, FromSecurityModule};
use ::identity::{Identity};

struct SecurityModuleHandle {
    close_sender: oneshot::Sender<()>,
}

/// This handle is used to send a closing notification to SecurityModuleHandle,
/// even after it was consumed.
impl SecurityModuleHandle {
    /// Send a close message to SecurityModuleHandle
    fn close(self) {
        self.close_sender.send(());
    }
}

struct SecurityModule<I> {
    identity: I,
    client_endpoints: Vec<(mpsc::Sender<FromSecurityModule>, mpsc::Receiver<ToSecurityModule>)>,
    close_receiver: oneshot::Receiver<()>,
}

impl<I: Identity> SecurityModule<I> {
    /// Create a security module together with a security module handle.
    /// The handle can be used to notify the security module to close, remotely.
    fn create(identity: I) -> (SecurityModuleHandle, Self) {

        // Create a oneshot channel between the handle and the security module:
        let (sender, receiver) = oneshot::channel();

        let sm_handle = SecurityModuleHandle {
            close_sender: sender,
        };

        let sm = SecurityModule {
            identity,
            client_endpoints: Vec::new(),
            close_receiver: receiver,
        };

        (sm_handle, sm)
    }


    /// Create a new client for the security module.
    /// Remote side is given a sender side of a channel, and a receiver side of a channel.
    fn new_client(&mut self) -> (mpsc::Sender<ToSecurityModule>, mpsc::Receiver<FromSecurityModule>) {
       let (sm_sender, client_receiver) = mpsc::channel(0);
       let (client_sender, sm_receiver) = mpsc::channel(0);

       self.client_endpoints.push((sm_sender, sm_receiver));
       (client_sender, client_receiver)

    }
}
