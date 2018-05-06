
use super::token_channel::ProcessMessageError;
use super::credit_calculator;
use utils::convert_int;
use proto::indexer::NeighborsRoute;
use crypto::hash::HashResult;
use crypto::uid::Uid;
use crypto::identity::PublicKey;
use super::messenger_messages::ResponseSendMessage;
use super::messenger_messages::FailedSendMessage;

#[derive(Clone)]
pub struct PendingNeighborRequest {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content_hash: HashResult,
    pub max_response_length: u32,
    pub processing_fee_proposal: u64,
    pub credits_per_byte_proposal: u64,
    pub request_bytes_count: u32,
    pub nodes_to_dest: usize,
}

/*
impl PendingNeighborRequest{
    /// Returns None if the given public key does not appear in the route, or if
    /// an integer overflow occurs during calculation.
    pub fn credits_to_freeze(&self) -> Option<u64> {
        credit_calculator::credits_to_freeze(self.processing_fee_proposal,
                                             self.request_bytes_count,
                                             self.credits_per_byte_proposal,
                                             self.max_response_length,
                                             self.nodes_to_dest)
    }

    // TODO(a4vision): Write good tests to avoid this, and write large comments to prevent it.
    //                  We should be aware that an attacker might try to make this calculation
    //                  fail (or the credits_on_failure calculation fail). We need to defend
    //                  against such failures. It is important to note that they might be
    //                  less predictable if the calculation methods get complex.
    pub fn credits_on_success(&self, response_length: usize) -> Option<u64> {
        credit_calculator::credits_on_success(self.processing_fee_proposal,
                                             self.request_bytes_count,
                                             self.credits_per_byte_proposal,
                                             self.max_response_length,
            convert_int::checked_as_u32(response_length)?,
            self.nodes_to_dest)
    }

    pub fn credits_on_failure(&self, nodes_to_reporting: usize) -> Option<u64> {
        credit_calculator::credits_on_failure(
                                             self.request_bytes_count,
                                             nodes_to_reporting)
    }

    pub fn verify_response_message(&self, response_send_msg: &ResponseSendMessage)
        -> Result<(), ProcessMessageError>{
        let destination_key = self.route.get_destination_public_key().
            ok_or(ProcessMessageError::InnerBug)?;
        if !response_send_msg.verify_signature(&destination_key, &self.request_content_hash){
            return Err(ProcessMessageError::InvalidResponseSignature);
        }

        let response_length = convert_int::checked_as_u32(response_send_msg.response_length()).
            ok_or(ProcessMessageError::TooLongMessage)?;

        if response_length > self.max_response_length{
            return Err(ProcessMessageError::TooLongMessage);
        }
        if self.processing_fee_proposal < response_send_msg.get_processing_fee(){
            return Err(ProcessMessageError::TooMuchFeeCollected);
        }
        Ok(())
    }

    pub fn verify_failed_message(&self, receiver_public_key: &PublicKey,
                                          failed_message: &FailedSendMessage)
        -> Result<(), ProcessMessageError>{
        if !failed_message.verify_reporter_position(receiver_public_key, &self.route){
            return Err(ProcessMessageError::InvalidFailureReporter)
        }
        if !failed_message.verify_signature(&self.request_content_hash){
            return Err(ProcessMessageError::InvalidFailedSignature)
        }
        Ok(())
    }

    pub fn get_route(&self) -> &NeighborsRoute{
        &self.route
    }
}
*/
