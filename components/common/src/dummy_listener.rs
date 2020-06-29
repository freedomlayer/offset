use crate::conn::{BoxFuture, Listener, ListenerClient};
use futures::{channel::mpsc, SinkExt};

pub struct ListenRequest<CONN, CONF, AR> {
    pub conn_sender: mpsc::Sender<CONN>,
    pub config_receiver: mpsc::Receiver<CONF>,
    pub arg: AR,
}

/// A test util: A mock Listener.
pub struct DummyListener<CONN, CONF, AR> {
    req_sender: mpsc::Sender<ListenRequest<CONN, CONF, AR>>,
}

// TODO: Why didn't the automatic #[derive(Clone)] works for DummyListener?
// Seemed like it had a problem with having config_receiver inside ListenRequest.
// This is a workaround for this issue:
impl<CONN, CONF, AR> Clone for DummyListener<CONN, CONF, AR> {
    fn clone(&self) -> DummyListener<CONN, CONF, AR> {
        DummyListener {
            req_sender: self.req_sender.clone(),
        }
    }
}

impl<CONN, CONF, AR> DummyListener<CONN, CONF, AR>
where
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    pub fn new(
        req_sender: mpsc::Sender<ListenRequest<CONN, CONF, AR>>,
    ) -> DummyListener<CONN, CONF, AR> {
        DummyListener { req_sender }
    }
}

#[derive(Debug)]
pub struct DummyListenerError;

impl<CONN, CONF, AR> Listener for DummyListener<CONN, CONF, AR>
where
    CONN: Send + 'static,
    CONF: Send + 'static,
    AR: Send + 'static,
{
    type Connection = CONN;
    type Config = CONF;
    type Arg = AR;
    type Error = DummyListenerError;

    fn listen(
        self,
        arg: Self::Arg,
    ) -> BoxFuture<'static, Result<ListenerClient<Self::Config, Self::Connection>, Self::Error>>
    {
        let (conn_sender, conn_receiver) = mpsc::channel(1);
        let (config_sender, config_receiver) = mpsc::channel(1);

        let listen_request = ListenRequest {
            conn_sender,
            config_receiver,
            arg,
        };

        let DummyListener { mut req_sender } = self;

        Box::pin(async move {
            req_sender
                .send(listen_request)
                .await
                .map_err(|_| DummyListenerError)?;

            Ok(ListenerClient {
                config_sender,
                conn_receiver,
            })
        })
    }
}
