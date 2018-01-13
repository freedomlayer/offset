
use ::inner_messages::{NeighborsRoute};
use ::crypto::uid::Uid;


// TODO: Make sure we do the right thing when hashing this / signing over this:
enum SendMessageNTranContent {
    CommMeans,
    Encrypted(Vec<u8>),
}

struct RequestSendMessageNTran {
    request_id: Uid,
    route: NeighborsRoute,
    request_content: SendMessageNTranContent,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u64,
}

/*


struct ResponseSendMessageTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        responseData @2: Data;
        signature @3: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "MESSAGE_SUCCESS" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   sha512/256(responseContent)
        #   randNonce)
}

struct FailedSendMessageTran {
        requestId @0: CustomUInt128;
        reportingPublicKey @1: CustomUInt256;
        randNonce @2: CustomUInt128;
        signature @3: CustomUInt512;
        # Signature{key=reportingNodePublicKey}(
        #   "MESSAGE_FAILURE" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   randNonce)
}


struct ResetChannelTran {
        newBalance @0: UInt64;
}
*/



pub struct Transactions {
}
