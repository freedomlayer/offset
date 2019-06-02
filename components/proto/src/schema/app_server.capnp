@0xcd5fc5928aa22c39;

using import "funder.capnp".FriendsRoute;
using import "common.capnp".Uid;
using import "common.capnp".InvoiceId;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;
using import "common.capnp".PaymentId;
using import "common.capnp".Rate;

using import "common.capnp".Receipt;
using import "common.capnp".Commit;
using import "common.capnp".MultiCommit;
using import "common.capnp".RelayAddress;
using import "common.capnp".NamedRelayAddress;
using import "common.capnp".NetAddress;
using import "common.capnp".NamedIndexServerAddress;

using import "report.capnp".NodeReport;
using import "report.capnp".NodeReportMutation;

using import "index.capnp".RequestRoutes;
using import "index.capnp".MultiRoute;


# Interface between AppServer and an Application
################################################

struct UserRequestSendFunds {
        requestId @0: Uid;
        route @1: FriendsRoute;
        invoiceId @2: InvoiceId;
        destPayment @3: CustomUInt128;
}

struct ResponseReceived {
        requestId @0: Uid;
        result: union {
                success @1: Receipt;
                failure @2: PublicKey; # Reporting public key
        }
}

struct ReceiptAck {
        requestId @0: Uid;
        receiptSignature @1: Signature;
}

# Application -> AppServer
struct AddFriend {
        friendPublicKey @0: PublicKey;
        relays @1: List(RelayAddress);
        name @2: Text;
        balance @3: CustomInt128;
}

# Application -> AppServer
struct SetFriendName {
        friendPublicKey @0: PublicKey;
        name @1: Text;
}

struct SetFriendRelays {
        friendPublicKey @0: PublicKey;
        relays @1: List(RelayAddress);
}

# Application -> AppServer
struct SetFriendRemoteMaxDebt {
        friendPublicKey @0: PublicKey;
        remoteMaxDebt @1: CustomUInt128;
}

struct SetFriendRate {
        friendPublicKey @0: PublicKey;
        rate @1: Rate;
}

# Application -> AppServer
struct ResetFriendChannel {
        friendPublicKey @0: PublicKey;
        resetToken @1: Signature;
}

struct ResponseRoutesResult {
        union {
                success @0: List(MultiRoute);
                failure @1: Void;
        }
}

struct ClientResponseRoutes {
        requestId @0: Uid;
        result @1: ResponseRoutesResult;
}

struct CreatePayment {
        paymentId @0: PaymentId;
        invoiceId @1: InvoiceId;
        totalDestPayment @2: CustomUInt128;
        destPublicKey @3: PublicKey;
}

struct CreateTransaction {
        paymentId @0: PaymentId;
        requestId @1: Uid;
        route @2: FriendsRoute;
        destPayment @3: CustomUInt128;
        fees @4: CustomUInt128;
}

struct AckClosePayment {
        paymentId @0: PaymentId;
        ackUid @1: Uid;
}

struct AddInvoice {
        invoiceId @0: InvoiceId;
        totalDestPayment @1: CustomUInt128;
}

#####################################################################

struct AppPermissions {
        routes @0: Bool;
        # Can request for routes
        buyer @1: Bool;
        # Can buy (Send credits)
        seller @2: Bool;
        # Can sell (Receive credits)
        config @3: Bool;
        # Can configure friends
}


struct ReportMutations {
        optAppRequestId: union {
                appRequestId @0: Uid;
                # Mutations were caused by an application request.
                empty @1: Void;
                # Mutations were caused for some other reason.
        }
        mutations @2: List(NodeReportMutation);
        # A list of mutations
}

struct RequestResult {
        union {
                success @0: Commit;
                failure @1: Void;
        }
}

struct TransactionResult {
        requestId @0: Uid;
        result @1: RequestResult;
}

struct PaymentStatusSuccess {
        receipt @0: Receipt;
        ackUid @1: Uid;
}

struct PaymentStatus {
        union {
                paymentNotFound @0: Void;
                inProgress @1: Void;
                success @2: PaymentStatusSuccess;
                canceled @3: Uid;
        }
}

struct ResponseClosePayment {
        paymentId @0: PaymentId;
        status @1: PaymentStatus;
}


struct AppServerToApp {
    union {
        # Funds
        transactionResult @0: TransactionResult;
        responseClosePayment @1: ResponseClosePayment;

        # Reports about current state:
        report @2: NodeReport;
        reportMutations @3: ReportMutations;

        # Routes:
        responseRoutes @4: ClientResponseRoutes;

    }
}

struct AppRequest {
    union {
        # Set relay address to be used locally
        addRelay @0: NamedRelayAddress;
        removeRelay @1: PublicKey;

        # Buyer (Sending Funds):
        createPayment @2: CreatePayment;
        createTransaction @3: CreateTransaction;
        requestClosePayment @4: PaymentId;
        ackClosePayment @5: AckClosePayment;

        # Seller (Receiving funds):
        addInvoice @6: AddInvoice;
        cancelInvoice @7: InvoiceId;
        commitInvoice @8: MultiCommit;

        # Friends management
        addFriend @9: AddFriend;
        setFriendRelays @10: SetFriendRelays;
        setFriendName @11: SetFriendName;
        removeFriend @12: PublicKey;
        enableFriend @13: PublicKey;
        disableFriend @14: PublicKey;
        openFriend @15: PublicKey;
        closeFriend @16: PublicKey;
        setFriendRemoteMaxDebt @17: SetFriendRemoteMaxDebt;
        setFriendRate @18: SetFriendRate;
        resetFriendChannel @19: ResetFriendChannel;

        # Routes:
        requestRoutes @20: RequestRoutes;

        # Index servers management:
        addIndexServer @21: NamedIndexServerAddress;
        removeIndexServer @22: PublicKey;
    }
}


struct AppToAppServer {
        appRequestId @0: Uid;
        appRequest @1: AppRequest;
}

