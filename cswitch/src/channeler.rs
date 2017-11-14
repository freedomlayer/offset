
extern crate futures;

use std::collections::HashMap;
use self::futures::future::{Future, LoopFn, Loop, loop_fn, IntoFuture};
use self::futures::sync::mpsc;
use self::futures::sync::oneshot;

use ::identity::PublicKey;
use ::inner_messages::{FromTimer, ChannelerToNetworker,
    NetworkerToChanneler, ToSecurityModule, FromSecurityModule,
    ChannelerNeighborInfo};

// TODO: Create a generic close handler, and use it for SecurityModule too.
struct ChannelerHandle {
    handle_close_sender: oneshot::Sender<()>,
    handle_close_receiver: oneshot::Receiver<()>,   // Closing is complete
}


enum ChannelerError {
}

struct ChannelerConnection {
}

enum ChannelerState {
    Reading,
}

struct Channeler {
    timer_receiver: mpsc::Receiver<FromTimer>, 
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    security_module_sender: mpsc::Sender<ToSecurityModule>,
    security_module_receiver: mpsc::Receiver<FromSecurityModule>,

    unverified_connections: Vec<ChannelerConnection>,
    connections: HashMap<PublicKey, Vec<ChannelerConnection>>,
    neighbors_info: HashMap<PublicKey, ChannelerNeighborInfo>,
}



impl Channeler {
    fn create(timer_receiver: mpsc::Receiver<FromTimer>, 
            networker_sender: mpsc::Sender<ChannelerToNetworker>,
            networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
            security_module_sender: mpsc::Sender<ToSecurityModule>,
            security_module_receiver: mpsc::Receiver<FromSecurityModule>)
                -> Self {

        Channeler {
            timer_receiver,
            networker_sender,
            networker_receiver,
            security_module_sender,
            security_module_receiver,
            unverified_connections: Vec::new(),
            connections: HashMap::new(),
            neighbors_info: HashMap::new(),
        }
    }

    fn loop_future(self) -> impl Future<Item=(),Error=ChannelerError> {
        loop_fn(self, |channeler| {
            Ok(Loop::Break(()))
        })
    }

}

