use futures::channel::mpsc;
use futures::SinkExt;
use futures::task::{Spawn, SpawnExt};
use crate::conn::{Listener};

#[allow(unused)]
pub struct ListenRequest<CONN,CONF,AR> {
    conn_sender: mpsc::Sender<CONN>,
    config_receiver: mpsc::Receiver<CONF>,
    arg: AR,
}

/// A test util: A mock Listener.
#[derive(Clone)]
pub struct DummyListener<S,CONN,CONF,AR> {
    req_sender: mpsc::Sender<ListenRequest<CONN,CONF,AR>>,
    spawner: S,
}

impl<S,CONN,CONF,AR> DummyListener<S,CONN,CONF,AR> 
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    pub fn new(req_sender: mpsc::Sender<ListenRequest<CONN,CONF,AR>>, 
               spawner: S) -> DummyListener<S,CONN,CONF,AR> {

        DummyListener {
            req_sender,
            spawner,
        }
    }
}

impl<S,CONN,CONF,AR> Listener for DummyListener<S,CONN,CONF,AR> 
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    type Connection = CONN;
    type Config = CONF;
    type Arg = AR;

    fn listen(self, arg: AR) -> (mpsc::Sender<CONF>, 
                             mpsc::Receiver<CONN>) {
        let (conn_sender, conn_receiver) = mpsc::channel(0);
        let (config_sender, config_receiver) = mpsc::channel(0);

        let listen_request = ListenRequest {
            conn_sender,
            config_receiver,
            arg,
        };

        let DummyListener {mut spawner, mut req_sender} = self;

        spawner.spawn(async move {
            let res = await!(req_sender.send(listen_request));
            if let Err(_) = res {
                println!("Error sending listen_request");
            }
        }).unwrap();

        (config_sender, conn_receiver)
    }
}
