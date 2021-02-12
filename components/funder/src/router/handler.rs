use database::transaction::Transaction;

use crate::token_channel::TcDbClient;

use crate::router::types::{RouterControl, RouterDbClient, RouterError, RouterInfo, RouterOp};
use crate::router::{handle_config, handle_friend, handle_liveness, handle_relays, handle_route};

pub async fn handle_router_op(
    control: &mut impl RouterControl,
    info: &RouterInfo,
    router_op: RouterOp,
) -> Result<(), RouterError> {
    match router_op {
        RouterOp::AddCurrency(friend_public_key, currency) => {
            handle_config::add_currency(control, info, friend_public_key, currency).await
        }
        RouterOp::SetRemoveCurrency(friend_public_key, currency) => {
            handle_config::set_remove_currency(control, info, friend_public_key, currency).await
        }
        RouterOp::UnsetRemoveCurrency(friend_public_key, currency) => {
            handle_config::unset_remove_currency(control, info, friend_public_key, currency).await
        }
        RouterOp::SetRemoteMaxDebt(friend_public_key, currency, remote_max_debt) => {
            handle_config::set_remote_max_debt(
                control,
                info,
                friend_public_key,
                currency,
                remote_max_debt,
            )
            .await
        }
        RouterOp::SetLocalMaxDebt(friend_public_key, currency, local_max_debt) => {
            handle_config::set_local_max_debt(
                control,
                info,
                friend_public_key,
                currency,
                local_max_debt,
            )
            .await
        }
        RouterOp::OpenCurrency(friend_public_key, currency) => {
            handle_config::open_currency(control, friend_public_key, currency).await
        }
        RouterOp::CloseCurrency(friend_public_key, currency) => {
            handle_config::close_currency(control, friend_public_key, currency).await
        }
        RouterOp::FriendMessage(friend_public_key, friend_message) => {
            handle_friend::incoming_friend_message(control, info, friend_public_key, friend_message)
                .await
        }
        RouterOp::SetFriendOnline(friend_public_key) => {
            handle_liveness::set_friend_online(control, info, friend_public_key).await
        }
        RouterOp::SetFriendOffline(friend_public_key) => {
            handle_liveness::set_friend_offline(control, friend_public_key).await
        }
        RouterOp::UpdateFriendLocalRelays(friend_public_key, friend_local_relays) => todo!(),
        RouterOp::UpdateLocalRelays(local_relays) => todo!(),
        RouterOp::SendRequest(currency, mc_request) => {
            handle_route::send_request(control, info, currency, mc_request).await
        }
        RouterOp::SendResponse(mc_response) => {
            handle_route::send_response(control, info, mc_response).await
        }
        RouterOp::SendCancel(mc_cancel) => handle_route::send_cancel(control, mc_cancel).await,
        // TODO: Should also handle add/remove friend?
    }?;

    // TODO: flush all pending send here
    todo!();
}
