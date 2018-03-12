use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use std::mem;
use utils::convert_int;
use crypto::hash;
use crypto::hash::HashResult;
use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use proto::indexer::NeighborsRoute;
use super::credit_calculator;
use super::pending_neighbor_request::PendingNeighborRequest;


pub struct ResponseSendMessage {
    request_id: Uid,
    // TODO(a4vision): Discuss it: What is this nonce for ? Is it against replay attacks ?
    rand_nonce: RandValue,
    // TODO(a4vision): Discuss it: Is it possible that the final destination will accept a processing
    //                  fee lower than the assigned processing fee ?
    processing_fee_collected: u64,
    response_content: Vec<u8>,
    signature: Signature,
}

impl ResponseSendMessage{
    pub fn verify_signature(&self, public_key: &PublicKey, request_hash: &HashResult) -> bool{
        let mut message = Vec::new();
        message.extend_from_slice(self.request_id.as_bytes());
        message.extend_from_slice(self.rand_nonce.as_bytes());
        // Serialize the processing_fee_collected:
        message.write_u64::<LittleEndian>(self.processing_fee_collected);
        message.extend_from_slice(&self.response_content);
        // TODO(a4vision): Discuss it.
        message.extend(request_hash.as_ref());
        return verify_signature(&message, &public_key, &self.signature);
    }

    pub fn bytes_count(&self) -> usize{
        // TODO(a4vision): Should we check for an overflow here ?
        mem::size_of_val(&self.request_id) +
        mem::size_of_val(&self.rand_nonce) +
        mem::size_of_val(&self.processing_fee_collected) +
        self.response_content.len() * 1 +
        mem::size_of_val(&self.signature)
    }

    pub fn response_length(&self) -> usize{
        // TODO(a4vision): Did we mean bytes_count, or response_send_msg.response_content.len() ?
        self.bytes_count()
    }

    pub fn get_request_id(&self) -> &Uid{
        &self.request_id
    }

    pub fn get_processing_fee(&self) -> u64{
        self.processing_fee_collected
    }

}

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

        // TODO(a4vision): Should we check for an overflow here ?
        mem::size_of::<Uid>() +
            mem::size_of::<PublicKey>() * self.route.public_keys.len() +
            self.request_content.len() * 1 +
            mem::size_of_val(&self.max_response_len) +
            mem::size_of_val(&self.processing_fee_proposal) +
            mem::size_of_val(&self.credits_per_byte_proposal)
    }

    fn nodes_to_dest(&self, sender_public_key: &PublicKey) -> Option<usize> {
        let destination = self.route.get_destination_public_key()?;
        let distance = self.route.distance_between_nodes(&sender_public_key, &destination)?;
        Some(distance)
    }

    pub fn get_request_id(&self) -> &Uid {
        &self.request_id
    }

    pub fn create_pending_request(&self, sender_public_key: &PublicKey) -> Option<PendingNeighborRequest> {
        Some(PendingNeighborRequest {
            request_id: self.request_id.clone(),
            route: self.route.clone(),
            request_bytes_count: convert_int::checked_as_u32(self.bytes_count())?,
            // TODO(a4vision): Discuss it: Should this hash be over the whole request ??? Otherwise,
            //                  some mediators might change parameters along the way.
            request_content_hash: hash::sha_512_256(self.request_content.as_ref()),
            max_response_length: self.max_response_len,
            processing_fee_proposal: self.processing_fee_proposal,
            credits_per_byte_proposal: self.credits_per_byte_proposal,
            nodes_to_dest: self.nodes_to_dest(sender_public_key)?,
        })
    }

    pub fn get_route(&self) -> &NeighborsRoute{
        &self.route
    }

    pub fn credits_to_freeze_on_destination(&self) -> Option<u64>{
        credit_calculator::credits_to_freeze(self.processing_fee_proposal,
                            convert_int::checked_as_u32(self.bytes_count())?,
                                              self.credits_per_byte_proposal,
                                              self.max_response_len, 1)
    }
}


pub struct FailedSendMessage {
    request_id: Uid,
    reporting_public_key: PublicKey,
    rand_nonce: RandValue,
    signature: Signature,
}

impl FailedSendMessage{
    pub fn nodes_to_reporting(&self, receiver_public_key: &PublicKey, route: &NeighborsRoute) -> Option<usize> {
        let distance = route.distance_between_nodes(receiver_public_key, &self.reporting_public_key)?;
        Some(distance)
    }

    pub fn verify_reporter_position(&self, receiver_public_key: &PublicKey, route: &NeighborsRoute) -> bool{
        match route.get_destination_public_key(){
            None=>return false,
            Some(destination_key)=> {
                if destination_key == self.reporting_public_key {
                    false
                } else {
                    match self.nodes_to_reporting(receiver_public_key, route) {
                        None => false,
                        // TODO(a4vision): Should we allow the destination to report a failure ?
                        Some(0) => false,
                        _ => true,
                    }
                }
            },
        }
    }

    pub fn verify_signature(&self, request_hash: &HashResult) -> bool{
        let mut message = Vec::new();
        message.extend_from_slice(self.request_id.as_bytes());
        message.extend_from_slice(self.reporting_public_key.as_bytes());
        message.extend_from_slice(self.rand_nonce.as_bytes());
        // TODO(a4vision): Discuss it.
        message.extend(request_hash.as_ref());
        return verify_signature(&message, &self.reporting_public_key, &self.signature);
    }

    pub fn get_request_id(&self) -> &Uid{
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
                    PublicKey::from_bytes(&vec![0u8; 32]).unwrap(),
                    PublicKey::from_bytes(&vec![0u8; 32]).unwrap(),
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
