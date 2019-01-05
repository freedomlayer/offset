use std::fmt::Debug;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use crate::friend::{FriendMutation, ChannelStatus};
use crate::state::{FunderMutation};

use common::canonical_serialize::CanonicalSerialize;
use proto::funder::messages::{FriendStatus, UserRequestSendFunds,
    SetFriendRemoteMaxDebt, ResetFriendChannel, SetFriendAddress, SetFriendName, 
    AddFriend, RemoveFriend, SetFriendStatus, SetRequestsStatus,
    ReceiptAck, FunderIncomingControl, ResponseReceived, 
    FunderOutgoingControl, ResponseSendFundsResult};

use super::MutableFunderHandler;
use crate::types::{ChannelerConfig, FunderOutgoingComm,
    ChannelerAddFriend};

// TODO: Should be an argument of the Funder:
const MAX_PENDING_USER_REQUESTS: usize = 0x10;

#[derive(Debug)]
pub enum HandleControlError {
    FriendDoesNotExist,
    TokenChannelDoesNotExist,
    NotInvitedToReset,
    ResetTokenMismatch,
    NotFirstInRoute,
    InvalidRoute,
    RequestAlreadyInProgress,
    PendingUserRequestsFull,
    ReceiptDoesNotExist,
    ReceiptSignatureMismatch,
    UserRequestInvalid,
    FriendNotReady,
    BlockedByFreezeGuard,
}


#[allow(unused)]
impl<A, R> MutableFunderHandler<A,R> 
where
    A: CanonicalSerialize + Clone + Debug + 'static + PartialEq + Eq,
    R: CryptoRandom + 'static,
{

    fn control_set_friend_remote_max_debt(&mut self, 
                                            set_friend_remote_max_debt: SetFriendRemoteMaxDebt) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let friend = self.get_friend(&set_friend_remote_max_debt.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        if friend.wanted_remote_max_debt == set_friend_remote_max_debt.remote_max_debt {
            // Wanted remote max debt is already set to this value. Nothing to do here.
            return Ok(())
        }

        // We only set the wanted remote max debt here. The actual remote max debt will be changed
        // only when we manage to send a move token message containing the SetRemoteMaxDebt
        // operation.
        let friend_mutation = FriendMutation::SetWantedRemoteMaxDebt(set_friend_remote_max_debt.remote_max_debt);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_remote_max_debt.friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(m_mutation);

        self.send_commands.set_try_send(&set_friend_remote_max_debt.friend_public_key);
        Ok(())
    }

    fn control_reset_friend_channel(&mut self, 
                                    reset_friend_channel: ResetFriendChannel) 
        -> Result<(), HandleControlError> {

        let friend = self.get_friend(&reset_friend_channel.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let remote_reset_terms = match &friend.channel_status {
            ChannelStatus::Consistent(_) => Err(HandleControlError::NotInvitedToReset),
            ChannelStatus::Inconsistent(channel_inconsistent) => {
                match &channel_inconsistent.opt_remote_reset_terms {
                    None => Err(HandleControlError::NotInvitedToReset),
                    Some(remote_reset_terms) => {
                        if (remote_reset_terms.reset_token != reset_friend_channel.current_token)  {
                            Err(HandleControlError::ResetTokenMismatch)
                        } else {
                            Ok(remote_reset_terms)
                        }
                    },
                }
            },
        }?;

        // We don't have the ability to sign here, therefore we defer the creation
        // of the local reset outgoing move token to the sender.
        self.send_commands.set_local_reset(&reset_friend_channel.friend_public_key);

        Ok(())
    }

    fn enable_friend(&mut self, 
                     friend_public_key: &PublicKey,
                     friend_address: &A) {

        let friend = self.get_friend(friend_public_key).unwrap();

        // Notify Channeler:
        let channeler_add_friend = ChannelerAddFriend {
            friend_public_key: friend_public_key.clone(),
            friend_address: friend_address.clone(),
            local_addresses: friend.sent_local_address.to_vec(),
        };
        let channeler_config = ChannelerConfig::AddFriend(channeler_add_friend);
        self.add_outgoing_comm(FunderOutgoingComm::ChannelerConfig(channeler_config));

    }

    fn disable_friend<'a>(&'a mut self, 
                     friend_public_key: &'a PublicKey) {

        // Cancel all pending requests to this friend:
        self.cancel_pending_requests(friend_public_key);
        self.cancel_pending_user_requests(friend_public_key);

        // Notify Channeler:
        let channeler_config = ChannelerConfig::RemoveFriend(
            friend_public_key.clone());
        self.add_outgoing_comm(FunderOutgoingComm::ChannelerConfig(channeler_config));

    }

    fn control_set_address(&mut self, opt_address: Option<A>) {
        let m_mutation = FunderMutation::SetAddress(opt_address.clone());
        self.apply_funder_mutation(m_mutation);

        // Notify Channeler about relay address change:
        let channeler_config = ChannelerConfig::SetAddress(opt_address.clone());
        self.add_outgoing_comm(FunderOutgoingComm::ChannelerConfig(channeler_config));

        // We might need to update all friends about the address change:
        let friend_public_keys = self.state.friends.keys()
            .cloned()
            .collect::<Vec<_>>();
        
        for friend_public_key in &friend_public_keys {
            self.send_commands.set_try_send(friend_public_key);
        }
    }

    fn control_add_friend(&mut self, add_friend: AddFriend<A>) 
        -> Result<(), HandleControlError> {

        let m_mutation = FunderMutation::AddFriend(add_friend.clone());
        self.apply_funder_mutation(m_mutation);

        Ok(())
    }

    /// This is a violent operation, as it removes all the known state with the remote friend.  
    /// An inconsistency will occur if the friend is added again.
    fn control_remove_friend(&mut self, remove_friend: RemoveFriend) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&remove_friend.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        self.disable_friend(&remove_friend.friend_public_key);

        self.cancel_local_pending_requests(
            &remove_friend.friend_public_key);
        self.cancel_pending_requests(
                &remove_friend.friend_public_key);
        self.cancel_pending_user_requests(
                &remove_friend.friend_public_key);

        let m_mutation = FunderMutation::RemoveFriend(
                remove_friend.friend_public_key.clone());

        self.apply_funder_mutation(m_mutation);
        Ok(())
    }

    fn control_set_friend_status(&mut self, set_friend_status: SetFriendStatus) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _ = self.get_friend(&set_friend_status.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_mutation = FriendMutation::SetStatus(set_friend_status.status.clone());
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_status.friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(m_mutation);

        let friend = self.get_friend(&set_friend_status.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_public_key = &set_friend_status.friend_public_key;
        let friend_address = friend.remote_address.clone();

        match set_friend_status.status {
            FriendStatus::Enabled => self.enable_friend(friend_public_key, &friend_address),
            FriendStatus::Disabled => self.disable_friend(&friend_public_key),
        };

        Ok(())
    }

    fn control_set_requests_status(&mut self, set_requests_status: SetRequestsStatus) 
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let _friend = self.get_friend(&set_requests_status.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        let friend_mutation = FriendMutation::SetWantedLocalRequestsStatus(set_requests_status.status);
        let m_mutation = FunderMutation::FriendMutation(
            (set_requests_status.friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(m_mutation);

        self.send_commands.set_try_send(&set_requests_status.friend_public_key);
        Ok(())
    }

    fn control_set_friend_address(&mut self, set_friend_address: SetFriendAddress<A>)
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let friend = self.get_friend(&set_friend_address.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        // If the newly proposed address is the same as the old one,
        // we do nothing:
        if set_friend_address.address == friend.remote_address {
            return Ok(())
        }

        let friend_mutation = FriendMutation::SetRemoteAddress(set_friend_address.address.clone());
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_address.friend_public_key.clone(), friend_mutation));

        self.apply_funder_mutation(m_mutation);

        // Notify Channeler to change the friend's address:
        self.disable_friend(&set_friend_address.friend_public_key);
        self.enable_friend(&set_friend_address.friend_public_key, 
                           &set_friend_address.address);

        Ok(())
    }

    fn control_set_friend_name(&mut self, set_friend_name: SetFriendName)
        -> Result<(), HandleControlError> {

        // Make sure that friend exists:
        let friend = self.get_friend(&set_friend_name.friend_public_key)
            .ok_or(HandleControlError::FriendDoesNotExist)?;

        // If the newly proposed name is the same as the old one, we do nothing:
        if friend.name == set_friend_name.name {
            return Ok(())
        }

        let friend_mutation = FriendMutation::SetName(set_friend_name.name);
        let m_mutation = FunderMutation::FriendMutation(
            (set_friend_name.friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(m_mutation);

        Ok(())
    }

    fn check_user_request_valid(&self, 
                                user_request_send_funds: &UserRequestSendFunds) 
                                -> Option<()> {

        if !user_request_send_funds.route.is_valid() {
            return None;
        }
        Some(())
    }

    fn control_request_send_funds_inner(&mut self, user_request_send_funds: UserRequestSendFunds)
        -> Result<(), HandleControlError> {

        self.check_user_request_valid(&user_request_send_funds)
            .ok_or(HandleControlError::UserRequestInvalid)?;

        // If we already have a receipt for this request, we return the receipt immediately and
        // exit. Note that we don't erase the receipt yet. This will only be done when a receipt
        // ack is received.
        if let Some(receipt) = self.state.ready_receipts.get(&user_request_send_funds.request_id) {
            let response_received = ResponseReceived {
                request_id: user_request_send_funds.request_id,
                result: ResponseSendFundsResult::Success(receipt.clone()),
            };
            self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(response_received));
            return Ok(());
        }

        let route = &user_request_send_funds.route;

        // We have to be the first on the route:
        match route.public_keys.first() {
            Some(first) if *first == self.state.local_public_key => Ok(()),
            _ => Err(HandleControlError::NotFirstInRoute),
        }?;

        // We want to have at least two public keys on the route (source and destination).
        // We also want that the public keys on the route are unique.
        if !route.is_valid() {
            return Err(HandleControlError::InvalidRoute);
        }
        let friend_public_key = route.public_keys[1].clone();

        let friend = match self.get_friend(&friend_public_key) {
            Some(friend) => Ok(friend),
            None => Err(HandleControlError::FriendDoesNotExist),
        }?;

        if !self.is_friend_ready(&friend_public_key) {
            return Err(HandleControlError::FriendNotReady);
        }

        // If request is already in progress, we do nothing:
        // Check if there is already a pending user request with the same request_id:
        for user_request in &friend.pending_user_requests {
            if user_request_send_funds.request_id == user_request.request_id {
                return Err(HandleControlError::RequestAlreadyInProgress);
            }
        }

        let token_channel = match &friend.channel_status {
            ChannelStatus::Inconsistent(_) => unreachable!(),
            ChannelStatus::Consistent(token_channel) => token_channel
        };

        // Check if there is an onging request with the same request_id with this specific friend:
        if token_channel
            .get_mutual_credit()
            .state()
            .pending_requests
            .pending_local_requests
            .contains_key(&user_request_send_funds.request_id) {
                return Err(HandleControlError::RequestAlreadyInProgress);
        }

        // Check if we have room to push this message:
        if friend.pending_user_requests.len() >= MAX_PENDING_USER_REQUESTS {
            return Err(HandleControlError::PendingUserRequestsFull);
        }

        let mut request_send_funds = user_request_send_funds.to_request();
        self.add_local_freezing_link(&mut request_send_funds);

        let verify_res = self.ephemeral
            .freeze_guard
            .verify_freezing_links(&request_send_funds.route,
                                   request_send_funds.dest_payment,
                                   &request_send_funds.freeze_links);

        if verify_res.is_none() {
            return Err(HandleControlError::BlockedByFreezeGuard);
        }


        let friend_mutation = FriendMutation::PushBackPendingUserRequest(request_send_funds);
        let funder_mutation = FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
        self.apply_funder_mutation(funder_mutation);
        self.send_commands.set_try_send(&friend_public_key);

        Ok(())
    }


    fn control_request_send_funds(&mut self, user_request_send_funds: UserRequestSendFunds) 
        -> Result<(), HandleControlError> {
        
        // If we managed to push the message, we return an Ok(()).
        // Otherwise, we return the internal error and return a response failure message.
        self.control_request_send_funds_inner(user_request_send_funds.clone())
            .map_err(|e| {
                let response_received = ResponseReceived {
                    request_id: user_request_send_funds.request_id,
                    result: ResponseSendFundsResult::Failure(self.state.local_public_key.clone()),
                };

                self.add_outgoing_control(FunderOutgoingControl::ResponseReceived(response_received));
                e
            })
    }

    /// Handle an incoming receipt ack message
    fn control_receipt_ack(&mut self, receipt_ack: ReceiptAck) 
        -> Result<(), HandleControlError> {

        let receipt = self.state.ready_receipts.get(&receipt_ack.request_id)
            .ok_or(HandleControlError::ReceiptDoesNotExist)?;

        // Make sure that the provided signature matches the one we have at the ready receipt.
        // We do this to make sure the user doesn't send a receipt ack before he actually got the
        // receipt (The user can not predit the receipt_signature ahead of time)
        if receipt_ack.receipt_signature != receipt.signature {
            return Err(HandleControlError::ReceiptSignatureMismatch);
        }

        let m_mutation = FunderMutation::RemoveReceipt(receipt_ack.request_id);
        self.apply_funder_mutation(m_mutation);

        Ok(())
    }


    pub fn handle_control_message(&mut self, 
                                  funder_config: FunderIncomingControl<A>) 
        -> Result<(), HandleControlError> {

        match funder_config {
            FunderIncomingControl::SetFriendRemoteMaxDebt(set_friend_remote_max_debt) => {
                self.control_set_friend_remote_max_debt(set_friend_remote_max_debt);
            },
            FunderIncomingControl::ResetFriendChannel(reset_friend_channel) => {
                self.control_reset_friend_channel(reset_friend_channel)?;
            },
            FunderIncomingControl::SetAddress(opt_address) => {
                self.control_set_address(opt_address);
            },
            FunderIncomingControl::AddFriend(add_friend) => {
                self.control_add_friend(add_friend);
            },
            FunderIncomingControl::RemoveFriend(remove_friend) => {
                self.control_remove_friend(remove_friend)?;
            },
            FunderIncomingControl::SetFriendStatus(set_friend_status) => {
                self.control_set_friend_status(set_friend_status);
            },
            FunderIncomingControl::SetRequestsStatus(set_requests_status) => {
                self.control_set_requests_status(set_requests_status);
            },
            FunderIncomingControl::SetFriendAddress(set_friend_address) => {
                self.control_set_friend_address(set_friend_address);
            },
            FunderIncomingControl::SetFriendName(set_friend_name) => {
                self.control_set_friend_name(set_friend_name);
            },
            FunderIncomingControl::RequestSendFunds(user_request_send_funds) => {
                self.control_request_send_funds(user_request_send_funds);
            },
            FunderIncomingControl::ReceiptAck(receipt_ack) => {
                self.control_receipt_ack(receipt_ack);
            }
        };
        Ok(())
    }
}
