@0xa13c661ee4a5c8d7;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".Receipt;

# Token channel messages
# ----------------------

struct NeighborMoveToken {
        tokenChannelIndex @0: UInt8;
        union {
                transactions @1: List(NeighborTransaction);
                resetChannel @2: Int64;
        }
        oldToken @3: CustomUInt256;
        randNonce @4: CustomUInt128;
}

struct NeighborInconsistencyError {
        tokenChannelIndex @0: UInt8;
        currentToken @1: CustomUInt256;
        balanceForReset @2: Int64;
}


# Token Transactions
# ------------------


struct SetRemoteMaxDebtTran {
        remoteMaxDebt @0: UInt64;
}


struct SetInvoiceIdTran {
        invoiceId @0: CustomUInt256;
}

struct LoadFundsTran {
        receipt @0: Receipt;
}



struct NeighborsRoute {
        publicKeys @0: List(CustomUInt256);
}


struct RequestSendMessageTran {
        requestId @0: CustomUInt128;
        route @1: NeighborsRoute;
        requestContent @2: Data;
        maxResponseLength @3: UInt32;
        processingFeeProposal @4: UInt64;
        creditsPerByteProposal @5: UInt64;
}


struct ResponseSendMessageTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        processingFeeCollected @2: UInt64;
        # The amount of credit actually collected from the proposed
        # processingFee. This value is at most request.processingFeeProposal.
        responseContent @3: Data;
        signature @4: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "REQUEST_SUCCESS" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   processingFeeCollected ||
        #   sha512/256(responseContent) ||
        #   randNonce)
}



struct FailedSendMessageTran {
        requestId @0: CustomUInt128;
        reportingPublicKeyIndex @1: UInt16;
        # Index on the route of the public key reporting this failure message.
        # The destination node should not be able to issue this message.
        randNonceSignatures @2: List(RandNonceSignature);
        # Signature{key=reportingNodePublicKey}(
        #   "REQUEST_FAILURE" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   prev randNonceSignatures ||
        #   randNonce)
}


struct NeighborTransaction {
        union {
                setRemoteMaxDebt @0: SetRemoteMaxDebtTran;
                setInvoiceIdTran @1: SetInvoiceIdTran;
                loadFunds @2: LoadFundsTran;
                requestSendMessage @3: RequestSendMessageTran;
                responseSendMessage @4: ResponseSendMessageTran;
                failedSendMessage @5: FailedSendMessageTran;
        }
}


