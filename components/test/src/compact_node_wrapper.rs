use futures::sink::SinkExt;
use futures::stream::StreamExt;

use common::conn::ConnPair;

use app::gen::gen_uid;
use stcompact::compact_node::messages::{CompactToUserAck, UserToCompact, UserToCompactAck};

#[derive(Debug)]
pub struct CompactNodeWrapperError;

/// Send a request and wait until the request is acked
pub async fn send_request(
    conn_pair: &mut ConnPair<UserToCompactAck, CompactToUserAck>,
    user_to_compact: UserToCompact,
) -> Result<(), CompactNodeWrapperError> {
    let user_request_id = gen_uid();
    let user_to_compact_ack = UserToCompactAck {
        user_request_id: user_request_id.clone(),
        inner: user_to_compact,
    };
    conn_pair.sender.send(user_to_compact_ack).await.unwrap();

    // Wait until we get an ack for our request:
    while let Some(compact_to_user_ack) = conn_pair.receiver.next().await {
        match compact_to_user_ack {
            CompactToUserAck::Ack(request_id) => {
                if request_id == user_request_id {
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
