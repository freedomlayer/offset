use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;
use futures::{StreamExt, SinkExt, FutureExt};

use common::conn::ConnPairString;

use crate::server_loop::ConnPairCompactServer;
use crate::messages::UserToServerAck;


#[derive(Debug)]
pub enum SerializeConnError {
    SpawnError,
}

/// Serialize a strings communication into ConnPairCompactServer
pub fn serialize_conn_pair<S>(conn_pair: ConnPairString, spawner: &S) -> Result<ConnPairCompactServer, SerializeConnError>
where
    S: Spawn,
{
    let (mut user_sender, mut user_receiver) = conn_pair.split();

    let (server_sender, mut receiver) = mpsc::channel(1);
    let (mut sender, server_receiver) = mpsc::channel(1);

    let send_fut = async move {
        while let Some(server_to_user_ack) = receiver.next().await {
            // Serialize:
            let ser_str = serde_json::to_string(&server_to_user_ack)
                .expect("Serialization error!");

            user_sender.send(ser_str).await.ok()?;
        }
        Some(())
    };
    spawner.spawn(send_fut.map(|_: Option<()>| ())).map_err(|_| SerializeConnError::SpawnError)?;

    let recv_fut = async move {
        while let Some(line) = user_receiver.next().await {
            // Deserialize:
            let user_to_server_ack: UserToServerAck = serde_json::from_str(&line).ok()?;

            // Forward to user:
            sender.send(user_to_server_ack).await.ok()?;
        }
        Some(())
    };
    spawner.spawn(recv_fut.map(|_: Option<()>| ())).map_err(|_| SerializeConnError::SpawnError)?;

    Ok(ConnPairCompactServer::from_raw(server_sender, server_receiver))
}
