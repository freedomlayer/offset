use app::conn::AppPermissions;

use crate::compact_node::messages::UserRequest;

/// Check if an app is allowed to send a certain user request
pub fn check_permission(user_request: &UserRequest, app_permissions: &AppPermissions) -> bool {
    match user_request {
        UserRequest::AddRelay(_)
        | UserRequest::RemoveRelay(_)
        | UserRequest::AddIndexServer(_)
        | UserRequest::RemoveIndexServer(_)
        | UserRequest::AddFriend(_)
        | UserRequest::SetFriendRelays(_)
        | UserRequest::SetFriendName(_)
        | UserRequest::RemoveFriend(_)
        | UserRequest::EnableFriend(_)
        | UserRequest::DisableFriend(_)
        | UserRequest::OpenFriendCurrency(_)
        | UserRequest::CloseFriendCurrency(_)
        | UserRequest::SetFriendCurrencyMaxDebt(_)
        | UserRequest::SetFriendCurrencyRate(_)
        | UserRequest::RemoveFriendCurrency(_)
        | UserRequest::ResetFriendChannel(_) => app_permissions.config,
        UserRequest::InitPayment(_)
        | UserRequest::ConfirmPaymentFees(_)
        | UserRequest::CancelPayment(_)
        | UserRequest::AckPaymentDone(_, _) => app_permissions.buyer,
        UserRequest::AddInvoice(_)
        | UserRequest::CancelInvoice(_)
        | UserRequest::RequestCommitInvoice(_) => app_permissions.seller,
    }
}
