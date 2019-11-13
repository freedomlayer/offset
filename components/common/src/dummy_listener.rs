use crate::conn::Listener;
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::SinkExt;

#[allow(unused)]
pub struct ListenRequest<CONN, CONF, AR> {
    pub conn_sender: mpsc::Sender<CONN>,
    pub config_receiver: mpsc::Receiver<CONF>,
    pub arg: AR,
}

/// A test util: A mock Listener.
pub struct DummyListener<S, CONN, CONF, AR> {
    req_sender: mpsc::Sender<ListenRequest<CONN, CONF, AR>>,
    spawner: S,
}

// TODO: Why didn't the automatic #[derive(Clone)] works for DummyListener?
// Seemed like it had a problem with having config_receiver inside ListenRequest.
// This is a workaround for this issue:
impl<S, CONN, CONF, AR> Clone for DummyListener<S, CONN, CONF, AR>
where
    S: Clone,
{
    fn clone(&self) -> DummyListener<S, CONN, CONF, AR> {
        DummyListener {
            req_sender: self.req_sender.clone(),
            spawner: self.spawner.clone(),
        }
    }
}

impl<S, CONN, CONF, AR> DummyListener<S, CONN, CONF, AR>
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    pub fn new(
        req_sender: mpsc::Sender<ListenRequest<CONN, CONF, AR>>,
        spawner: S,
    ) -> DummyListener<S, CONN, CONF, AR> {
        DummyListener {
            req_sender,
            spawner,
        }
    }
}

impl<S, CONN, CONF, AR> Listener for DummyListener<S, CONN, CONF, AR>
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    type Connection = CONN;
    type Config = CONF;
    type Arg = AR;

    fn listen(self, arg: AR) -> (mpsc::Sender<CONF>, mpsc::Receiver<CONN>) {
        let (conn_sender, conn_receiver) = mpsc::channel(1);
        let (config_sender, config_receiver) = mpsc::channel(1);

        let listen_request = ListenRequest {
            conn_sender,
            config_receiver,
            arg,
        };

        let DummyListener {
            spawner,
            mut req_sender,
        } = self;

        spawner
            .spawn(async move {
                let res = req_sender.send(listen_request).await;
                if res.is_err() {
                    error!("Error sending listen_request");
                }
            })
            .unwrap();

        (config_sender, conn_receiver)
    }
}
