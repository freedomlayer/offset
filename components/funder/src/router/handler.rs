use crate::router::types::{RouterControl, RouterDbClient, RouterOp};
use crate::router::{handle_config, handle_friend, handle_liveness, handle_relays, handle_route};

pub async fn handle_router_op<RC>(router_control: &mut RouterControl<'_, RC>, router_op: RouterOp)
where
    RC: RouterDbClient,
{
    match router_op {
        RouterOp::AddCurrency(friend_public_key, currency) => todo!(),
        RouterOp::RemoveCurrency(friend_public_key, currency) => todo!(),
        RouterOp::SetRemoteMaxDebt(friend_public_key, currency, remote_max_debt) => todo!(),
        RouterOp::SetLocalMaxDebt(friend_public_key, currency, remote_max_debt) => todo!(),
        RouterOp::OpenCurrency(friend_public_key, currency) => todo!(),
        RouterOp::CloseCurrency(friend_public_key, currency) => todo!(),
        RouterOp::FriendMessage(friend_public_key, friend_message) => todo!(),
        RouterOp::SetFriendOnline(friend_public_key) => todo!(),
        RouterOp::SetFriendOffline(friend_public_key) => todo!(),
        RouterOp::UpdateFriendLocalRelays(friend_public_key, friend_local_relays) => todo!(),
        RouterOp::UpdateLocalRelays(local_relays) => todo!(),
        RouterOp::SendRequest(currency, mc_request) => todo!(),
        RouterOp::SendResponse(mc_response) => todo!(),
        RouterOp::SendCancel(mc_cancel) => todo!(),
    }
    todo!();
}
