use crate::conn::{BoxFuture, ListenClient, Listener};
use futures::channel::mpsc;
use futures::future;
use futures::task::{Spawn, SpawnExt};
use futures::SinkExt;

#[allow(unused)]
pub struct ListenRequest<CONN, CONF> {
    pub conn_sender: mpsc::Sender<CONN>,
    pub config_receiver: mpsc::Receiver<CONF>,
}

/// A test util: A mock Listener.
pub struct DummyListener<S, CONN, CONF> {
    req_sender: mpsc::Sender<ListenRequest<CONN, CONF>>,
    spawner: S,
}

// TODO: Why didn't the automatic #[derive(Clone)] works for DummyListener?
// Seemed like it had a problem with having config_receiver inside ListenRequest.
// This is a workaround for this issue:
impl<S, CONN, CONF> Clone for DummyListener<S, CONN, CONF>
where
    S: Clone,
{
    fn clone(&self) -> DummyListener<S, CONN, CONF> {
        DummyListener {
            req_sender: self.req_sender.clone(),
            spawner: self.spawner.clone(),
        }
    }
}

impl<S, CONN, CONF> DummyListener<S, CONN, CONF>
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
{
    pub fn new(
        req_sender: mpsc::Sender<ListenRequest<CONN, CONF>>,
        spawner: S,
    ) -> DummyListener<S, CONN, CONF> {
        DummyListener {
            req_sender,
            spawner,
        }
    }
}

#[derive(Debug)]
pub struct DummyListenerError;

impl<S, CONN, CONF> Listener for DummyListener<S, CONN, CONF>
where
    S: Spawn,
    CONN: Send + 'static,
    CONF: Send + 'static,
{
    type Conn = CONN;
    type Config = CONF;
    type Error = DummyListenerError;

    fn listen(
        self,
    ) -> BoxFuture<'static, Result<ListenClient<Self::Config, Self::Conn>, Self::Error>> {
        let (conn_sender, conn_receiver) = mpsc::channel(1);
        let (config_sender, config_receiver) = mpsc::channel(1);

        let listen_request = ListenRequest {
            conn_sender,
            config_receiver,
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

        Box::pin(future::ready(Ok(ListenClient {
            config_sender,
            conn_receiver,
        })))
    }
}
