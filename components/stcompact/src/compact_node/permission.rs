use app::conn::AppPermissions;

use crate::compact_node::messages::UserToCompact;

/// Check if an app is allowed to send a certain user request
pub fn check_permission(user_request: &UserToCompact, app_permissions: &AppPermissions) -> bool {
    match user_request {
        UserToCompact::AddRelay(_)
        | UserToCompact::RemoveRelay(_)
        | UserToCompact::AddIndexServer(_)
        | UserToCompact::RemoveIndexServer(_)
        | UserToCompact::AddFriend(_)
        | UserToCompact::SetFriendRelays(_)
        | UserToCompact::SetFriendName(_)
        | UserToCompact::RemoveFriend(_)
        | UserToCompact::EnableFriend(_)
        | UserToCompact::DisableFriend(_)
        | UserToCompact::OpenFriendCurrency(_)
        | UserToCompact::CloseFriendCurrency(_)
        | UserToCompact::SetFriendCurrencyMaxDebt(_)
        | UserToCompact::SetFriendCurrencyRate(_)
        | UserToCompact::RemoveFriendCurrency(_)
        | UserToCompact::ResetFriendChannel(_) => app_permissions.config,
        UserToCompact::InitPayment(_)
        | UserToCompact::ConfirmPaymentFees(_)
        | UserToCompact::CancelPayment(_)
        | UserToCompact::AckPaymentDone(_, _) => app_permissions.buyer,
        UserToCompact::AddInvoice(_)
        | UserToCompact::CancelInvoice(_)
        | UserToCompact::CommitInvoice(_) => app_permissions.seller,
        UserToCompact::RequestVerifyCommit(_) => true,
    }
}
