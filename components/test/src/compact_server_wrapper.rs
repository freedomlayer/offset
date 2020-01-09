use futures::sink::SinkExt;
use futures::stream::StreamExt;

use common::conn::ConnPair;

use app::gen::gen_uid;
use stcompact::messages::{ServerToUserAck, UserToServer, UserToServerAck};

#[derive(Debug)]
pub struct CompactServerWrapperError;

/// Send a request and wait until the request is acked
pub async fn send_request(
    conn_pair: &mut ConnPair<UserToServerAck, ServerToUserAck>,
    user_to_server: UserToServer,
) -> Result<(), CompactServerWrapperError> {
    let request_id = gen_uid();
    let user_to_server_ack = UserToServerAck {
        request_id: request_id.clone(),
        inner: user_to_server,
    };
    conn_pair.sender.send(user_to_server_ack).await.unwrap();

    // Wait until we get an ack for our request:
    while let Some(server_to_user_ack) = conn_pair.receiver.next().await {
        match server_to_user_ack {
            ServerToUserAck::Ack(in_request_id) => {
                if request_id == in_request_id {
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
