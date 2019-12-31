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

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use proto::crypto::{Uid, PrivateKey, PublicKey};
    use proto::net::messages::NetAddress;

    use crate::messages::{ServerToUserAck, UserToServerAck, UserToServer, 
        RequestCreateNode, NodeName, CreateNodeRemote, ServerToUser, NodeId};
    use crate::compact_node::{CompactToUser, PaymentDone};

    #[test]
    fn test_ser_deser_server_to_user_ack1() {
        let msg = ServerToUserAck::Ack(Uid::from(&[21; Uid::len()]));
        let ser_str = serde_json::to_string(&msg).unwrap();
        let msg2 = serde_json::from_str(&ser_str).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_ser_deser_server_to_user_ack2() {
        let payment_done = PaymentDone::Failure(Uid::from(&[1; Uid::len()]));
        let compact_to_user = CompactToUser::PaymentDone(payment_done);
        let server_to_user = ServerToUser::Node(NodeId(0x100u64), compact_to_user);
        let msg = ServerToUserAck::ServerToUser(server_to_user);
        let ser_str = serde_json::to_string(&msg).unwrap();
        println!("{}", ser_str);
        let msg2 = serde_json::from_str(&ser_str).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_ser_deser_user_to_server_ack() {
        let create_node_remote = CreateNodeRemote {
            node_name: NodeName::new("node_name".to_owned()),
            app_private_key: PrivateKey::from(&[0xaa; PrivateKey::len()]),
            node_public_key: PublicKey::from(&[0xbb; PublicKey::len()]),
            node_address: NetAddress::try_from("net_address".to_owned()).unwrap(),
        };
        let request_create_node = RequestCreateNode::CreateNodeRemote(create_node_remote);
        let msg = UserToServerAck {
            request_id: Uid::from(&[1; Uid::len()]),
            inner: UserToServer::RequestCreateNode(request_create_node),
        };
        let ser_str = serde_json::to_string(&msg).unwrap();
        // println!("{}", ser_str);
        let msg2 = serde_json::from_str(&ser_str).unwrap();
        assert_eq!(msg, msg2);
    }

}
