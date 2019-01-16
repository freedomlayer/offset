use std::collections::VecDeque;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use futures::task::Spawn;

use common::conn::ConnPair;
use crypto::identity::PublicKey;

type RawConn = ConnPair<Vec<u8>,Vec<u8>>;

struct ConnectPoolError;

struct ConnectPool<B> {
    addresses: VecDeque<B>,
}

#[derive(Debug, Clone)]
pub enum CpConfig<B> {
    AddAddress(B),
    RemoveAddress(B),
}

pub struct CpConnectRequest {
    response_sender: oneshot::Sender<RawConn>,
}

pub struct CpConnectClient {
    request_sender: mpsc::Sender<CpConnectRequest>,
}

#[derive(Debug, Clone)]
pub struct CpConfigRequest<B> {
    config: CpConfig<B>,
}

pub struct CpConfigClient<B> {
    request_sender: mpsc::Sender<CpConfigRequest<B>>,
}

impl CpConnectClient {
    pub async fn connect(&mut self) -> Result<RawConn, ConnectPoolError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let connect_request = CpConnectRequest {
            response_sender,
        };
        await!(self.request_sender.send(connect_request))
            .map_err(|_| ConnectPoolError)?;

        await!(response_receiver)
            .map_err(|_| ConnectPoolError)
    }
}

impl<B> CpConfigClient<B> {
    pub async fn config(&mut self, config: CpConfig<B>) -> Result<(), ConnectPoolError> {
        let config_request = CpConfigRequest {
            config,
        };
        await!(self.request_sender.send(config_request))
            .map_err(|_| ConnectPoolError)?;
        Ok(())
    }
}

fn create_connect_pool<B,S>(friend_public_key: PublicKey, 
                         addresses: Vec<B>,
                         spawner: S) -> (CpConfigClient<B>, CpConnectClient) 
where
    S: Spawn,
{
    unimplemented!();
}
