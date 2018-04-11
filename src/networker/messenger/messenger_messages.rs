use bytes::Bytes;
use bytes::BytesMut;
use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use utils::signed_message::SignedMessage;
use utils::signed_message;
use std::mem;
use utils::convert_int;
use crypto::hash;
use crypto::hash::HashResult;
use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use proto::indexer::NeighborsRoute;
use proto::common::SendFundsReceipt;
use proto::funder::InvoiceId;
use super::credit_calculator;
use super::pending_neighbor_request::PendingNeighborRequest;


/// Messages that a `Networker` sends and receives.
pub enum NetworkerTCMessage {
    SetRemoteMaxDebt(u64),
    SetInvoiceId(InvoiceId),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage(RequestSendMessage),
    ResponseSendMessage(ResponseSendMessage),
    FailedSendMessage(FailedSendMessage),
    // new_balanace
    // ResetChannel(i64),
}


#[derive(Debug, Eq, PartialEq)]
pub struct ResponseSendMessage {
    request_id: Uid,
    rand_nonce: RandValue,
    processing_fee_collected: u64,
    response_content: Vec<u8>,
    signature: Signature,
}

impl SignedMessage for ResponseSendMessage{
    fn signature(&self) -> &Signature{
        &self.signature
    }

    fn set_signature(&mut self, signature: Signature){
        self.signature = signature
    }

    fn as_bytes(&self) -> Bytes{
        let mut message = Vec::new();
        message.extend_from_slice(&self.request_id);
        message.extend_from_slice(&self.rand_nonce);
        // Serialize the processing_fee_collected:
        message.write_u64::<LittleEndian>(self.processing_fee_collected);
        message.extend_from_slice(&self.response_content);

        signed_message::ref_to_bytes(message.as_ref())
    }
}

impl ResponseSendMessage{

    pub fn bytes_count(&self) -> usize{
        mem::size_of_val(&self.request_id) +
        mem::size_of_val(&self.rand_nonce) +
        mem::size_of_val(&self.processing_fee_collected) +
        self.response_content.len() + // number of bytes that represent the array
        mem::size_of_val(&self.signature)
    }

    pub fn response_length(&self) -> usize{
        self.bytes_count()
    }

    pub fn request_id(&self) -> &Uid{
        &self.request_id
    }

    pub fn processing_fee(&self) -> u64{
        self.processing_fee_collected
    }

}

#[derive(Debug, PartialEq, Eq)]
pub struct RequestSendMessage {
    request_id: Uid,
    route: NeighborsRoute,
    request_content: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u64,
}

impl RequestSendMessage {
    pub fn bytes_count(&self) -> usize {
        // We count the bytes count here and not before deserialization,
        // because we actually charge for the amount of bytes we send, and not for the
        // amount of bytes we receive (Those could possibly be encoded in some strange way)
        mem::size_of::<Uid>() +
            mem::size_of::<PublicKey>() * self.route.public_keys.len() +
            self.request_content.len() + // number of bytes that represent the array
            mem::size_of_val(&self.max_response_len) +
            mem::size_of_val(&self.processing_fee_proposal) +
            mem::size_of_val(&self.credits_per_byte_proposal)
    }

    fn nodes_to_dest(&self, sender_public_key: &PublicKey) -> Option<usize> {
        let destination = self.route.get_destination_public_key()?;
        let distance = self.route.distance_between_nodes(sender_public_key, &destination)?;
        Some(distance)
    }

    pub fn request_id(&self) -> &Uid {
        &self.request_id
    }

    pub fn create_pending_request(&self, sender_public_key: &PublicKey) -> Option<PendingNeighborRequest> {
        Some(PendingNeighborRequest {
            request_id: self.request_id,
            route: self.route.clone(),
            request_bytes_count: convert_int::checked_as_u32(self.bytes_count())?,
            request_content_hash: hash::sha_512_256(self.request_content.as_ref()),
            max_response_length: self.max_response_len,
            processing_fee_proposal: self.processing_fee_proposal,
            credits_per_byte_proposal: self.credits_per_byte_proposal,
            nodes_to_dest: self.nodes_to_dest(sender_public_key)?,
        })
    }

    pub fn route(&self) -> &NeighborsRoute{
        &self.route
    }

    pub fn credits_to_freeze_on_destination(&self) -> Option<u64>{
        credit_calculator::credits_to_freeze(self.processing_fee_proposal,
                            convert_int::checked_as_u32(self.bytes_count())?,
                                              self.credits_per_byte_proposal,
                                              self.max_response_len, 1)
    }
}


#[derive(Debug, Eq, PartialEq)]
pub struct FailedSendMessage {
    request_id: Uid,
    reporting_public_key: PublicKey,
    rand_nonce: RandValue,
    signature: Signature,
}

impl SignedMessage for FailedSendMessage{
    fn signature(&self) -> &Signature{
        &self.signature
    }

    fn set_signature(&mut self, signature: Signature){
        self.signature = signature
    }

    fn as_bytes(&self) -> Bytes {
        let mut message = Vec::new();
        message.extend_from_slice(self.request_id.as_ref());
        message.extend_from_slice(self.reporting_public_key.as_ref());
        message.extend_from_slice(self.rand_nonce.as_ref());
        signed_message::ref_to_bytes(message.as_ref())
    }
}

impl FailedSendMessage{
    pub fn nodes_to_reporting(&self, receiver_public_key: &PublicKey, route: &NeighborsRoute) -> Option<usize> {
        let distance = route.distance_between_nodes(receiver_public_key, &self.reporting_public_key)?;
        Some(distance)
    }

    pub fn verify_reporter_position(&self, receiver_public_key: &PublicKey, route: &NeighborsRoute) -> bool{
        match route.get_destination_public_key(){
            None=>false,
            Some(destination_key)=> {
                if destination_key == self.reporting_public_key {
                    false
                } else {
                    match self.nodes_to_reporting(receiver_public_key, route) {
                        Some(0) | None => false,
//                        Some(0) => false,
                        _ => true,
                    }
                }
            },
        }
    }

    pub fn verify_failed_message_signature(&self, request_data: &[u8]) -> bool{
        self.verify_signature(&self.reporting_public_key, &[])
    }

    pub fn request_id(&self) -> &Uid{
        &self.request_id
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use ring::test::rand::FixedByteRandom;
    use crypto::uid::Uid;
    use proto::indexer::NeighborsRoute;


    #[test]
    fn test_request_send_msg_bytes_count() {
        let rng1 = FixedByteRandom { byte: 0x03 };

        assert_eq!(mem::size_of::<PublicKey>(), 32);

        let rsm = RequestSendMessage {
            request_id: Uid::new(&rng1),
            route: NeighborsRoute {
                public_keys: vec![
                    PublicKey::from(&[0u8; 32]),
                    PublicKey::from(&[0u8; 32]),
                ],
            },
            request_content: vec![1,2,3,4,5],
            max_response_len: 0x200,
            processing_fee_proposal: 1,
            credits_per_byte_proposal: 2,
        };

        let expected = 16 + 32 + 32 + 5 + 4 + 8 + 8;
        assert_eq!(rsm.bytes_count(), expected);
    }
}
