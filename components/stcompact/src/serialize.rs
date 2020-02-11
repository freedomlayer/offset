use futures::task::{Spawn, SpawnExt};

use futures::channel::mpsc;
use futures::{FutureExt, SinkExt, StreamExt};

use common::conn::ConnPairString;

use crate::messages::UserToServerAck;
use crate::server_loop::ConnPairCompactServer;

#[derive(Debug)]
pub enum SerializeConnError {
    SpawnError,
}

/// Serialize a strings communication into ConnPairCompactServer
pub fn serialize_conn_pair<S>(
    conn_pair: ConnPairString,
    spawner: &S,
) -> Result<ConnPairCompactServer, SerializeConnError>
where
    S: Spawn,
{
    let (mut user_sender, mut user_receiver) = conn_pair.split();

    let (server_sender, mut receiver) = mpsc::channel(1);
    let (mut sender, server_receiver) = mpsc::channel(1);

    let send_fut = async move {
        while let Some(server_to_user_ack) = receiver.next().await {
            // Serialize:
            let ser_str = serde_json::to_string(&server_to_user_ack).expect("Serialization error!");

            user_sender.send(ser_str).await.ok()?;
        }
        Some(())
    };
    spawner
        .spawn(send_fut.map(|_: Option<()>| ()))
        .map_err(|_| SerializeConnError::SpawnError)?;

    let recv_fut = async move {
        while let Some(line) = user_receiver.next().await {
            // Deserialize:
            let user_to_server_ack: UserToServerAck = serde_json::from_str(&line).ok()?;

            // Forward to user:
            sender.send(user_to_server_ack).await.ok()?;
        }
        Some(())
    };
    spawner
        .spawn(recv_fut.map(|_: Option<()>| ()))
        .map_err(|_| SerializeConnError::SpawnError)?;

    Ok(ConnPairCompactServer::from_raw(
        server_sender,
        server_receiver,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    use serde::{Deserialize, Serialize};

    use quickcheck::QuickCheck;

    #[allow(unused)]
    use proto::crypto::{InvoiceId, PrivateKey, PublicKey, Uid};
    use proto::funder::messages::Currency;
    use proto::net::messages::NetAddress;

    use crate::compact_node::messages::*;
    use crate::messages::*;

    #[test]
    fn test_ser_deser_server_to_user_ack1() {
        let msg = ServerToUserAck::Ack(Uid::from(&[21; Uid::len()]));
        let ser_str = serde_json::to_string(&msg).unwrap();
        let msg2 = serde_json::from_str(&ser_str).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_ser_deser_server_to_user_ack2() {
        let open_invoice = OpenInvoice {
            currency: Currency::try_from("FST".to_owned()).unwrap(),
            total_dest_payment: 0x1234u128,
            description: "description".to_owned(),
            is_commited: false,
            generation: Generation(5),
        };
        let mut open_invoices = HashMap::new();
        open_invoices.insert(InvoiceId::from(&[0x11; InvoiceId::len()]), open_invoice);
        let compact_report = CompactReport {
            local_public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
            index_servers: Vec::new(),
            opt_connected_index_server: None,
            relays: Vec::new(),
            friends: HashMap::new(),
            open_invoices,
            open_payments: HashMap::new(),
        };
        let compact_to_user = CompactToUser::Report(compact_report);
        let server_to_user = ServerToUser::Node(NodeId(0x100u64), compact_to_user);
        let msg = ServerToUserAck::ServerToUser(server_to_user);
        let ser_str = serde_json::to_string(&msg).unwrap();
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
        let request_create_node = CreateNode::CreateNodeRemote(create_node_remote);
        let msg = UserToServerAck {
            request_id: Uid::from(&[1; Uid::len()]),
            inner: UserToServer::CreateNode(request_create_node),
        };
        let ser_str = serde_json::to_string(&msg).unwrap();
        let msg2 = serde_json::from_str(&ser_str).unwrap();
        assert_eq!(msg, msg2);
    }

    #[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ExampleStruct {
        ex_num: i32,
        ex_string: String,
    }

    #[quickcheck]
    fn qc_example_struct(msg: ExampleStruct) -> bool {
        let ser_str = serde_json::to_string(&msg).unwrap();
        let msg2: ExampleStruct = serde_json::from_str(&ser_str).unwrap();
        msg2 == msg
    }

    #[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    enum ExampleEnum {
        Hello,
        World(Vec<u8>),
        MyString(String),
    }

    #[quickcheck]
    fn qc_example_enum(msg: ExampleEnum) -> bool {
        let ser_str = serde_json::to_string(&msg).unwrap();
        let msg2: ExampleEnum = serde_json::from_str(&ser_str).unwrap();
        msg2 == msg
    }

    #[test]
    fn qc_mc_info_json() {
        fn ser_de(msg: McInfo) -> bool {
            let ser_str = serde_json::to_string(&msg).unwrap();
            let msg2: McInfo = serde_json::from_str(&ser_str).unwrap();
            msg2 == msg
        }
        QuickCheck::new()
            .max_tests(100)
            .quickcheck(ser_de as fn(McInfo) -> bool);
    }

    #[test]
    fn qc_server_to_user_ack_json() {
        fn ser_de(msg: ServerToUserAck) -> bool {
            let ser_str = serde_json::to_string(&msg).unwrap();
            let msg2: ServerToUserAck = serde_json::from_str(&ser_str).unwrap();
            msg2 == msg
        }
        QuickCheck::new()
            .max_tests(100)
            .quickcheck(ser_de as fn(ServerToUserAck) -> bool);
    }

    #[test]
    fn qc_user_to_server_ack_json() {
        fn ser_de(msg: UserToServerAck) -> bool {
            let ser_str = serde_json::to_string(&msg).unwrap();
            let msg2: UserToServerAck = serde_json::from_str(&ser_str).unwrap();
            msg2 == msg
        }
        QuickCheck::new()
            .max_tests(100)
            .quickcheck(ser_de as fn(UserToServerAck) -> bool);
    }
}
