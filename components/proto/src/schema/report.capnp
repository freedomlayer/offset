@0x8bc829b5200f3c7f;

using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;

using import "common.capnp".RelayAddress;
using import "common.capnp".IndexServerAddress;

## Report related structs
#########################

struct MoveTokenHashed {
        operationsHash @0: Hash;
        oldToken @1: Signature;
        inconsistencyCounter @2: UInt64;
        moveTokenCounter @3: CustomUInt128;
        balance @4: CustomInt128;
        localPendingDebt @5: CustomUInt128;
        remotePendingDebt @6: CustomUInt128;
        randNonce @7: RandNonce;
        newToken @8: Signature;
}


enum FriendStatus {
        disabled @0;
        enabled @1;
}

enum RequestsStatus {
        closed @0;
        open @1;
}

enum FriendLivenessReport {
        offline @0;
        online @1;
}

enum DirectionReport {
        incoming @0;
        outgoing @1;
}

struct McRequestsStatus {
        local @0: RequestsStatus;
        remote @1: RequestsStatus;
}

struct TcReport {
        direction @0: DirectionReport;
        balance @1: CustomInt128;
        requestsStatus @2: McRequestsStatus;
        numLocalPendingRequests @3: UInt64;
        numRemotePendingRequests @4: UInt64;
}

struct ChannelInconsistentReport {
        localResetTermsBalance @0: CustomInt128;
        optRocalResetTermsBalance: union {
                remoteResetTerms @1: CustomInt128;
                empty @2: Void;
        }
}


struct ChannelStatusReport {
        union {
                inconsistent @0: ChannelInconsistentReport;
                consistenet @1: TcReport;
        }
}

struct OptLastIncomingMoveToken {
        union {
                moveTokenHashed @0: MoveTokenHashed;
                empty @1: Void;
        }
}


struct FriendReport {
        address @0: RelayAddress;
        name @1: Text;
        optLastIncomingMoveToken @2: OptLastIncomingMoveToken;
        liveness @3: FriendLivenessReport;
        channelStatus @4: ChannelStatusReport;
        wantedRemoteMaxDebt @5: CustomUInt128;
        wantedLocalRequestsStatus @6: RequestsStatus;
        numPendingRequests @7: UInt64;
        numPendingResponses @8: UInt64;
        status @9: FriendStatus;
        numPendingUserRequests @10: UInt64;
}

# A full report. Contains a full summary of the current state.
# This will usually be sent only once, and then ReportMutations will be sent.
struct FunderReport {
        localPublicKey @0: PublicKey;
        optAddress: union {
                address @1: RelayAddress;
                empty @2: Void;
        }
        friends @3: List(FriendReport);
        numReadyReceipts @4: UInt64;
}


############################################################################
############################################################################


struct SetAddressReport {
    union {
        address @0: RelayAddress;
        empty @1: Void;
    }
}

struct AddFriendReport {
        friendPublicKey @0: PublicKey;
        address @1: RelayAddress;
        name @2: Text;
        balance @3: CustomInt128;
        optLastIncomingMoveToken @4: OptLastIncomingMoveToken;
        channelStatus @5: ChannelStatusReport;
}

struct RelayAddressName {
        address @0: RelayAddress;
        name @1: Text;
}

struct FriendReportMutation {
        union {
                setFriendInfo @0: RelayAddressName;
                setChannelStatus @1: ChannelStatusReport;
                setWantedRemoteMaxDebt @2: CustomUInt128;
                setWantedLocalRequestsStatus @3: RequestsStatus;
                setNumPendingRequests @4: UInt64;
                setNumPendingResponses @5: UInt64;
                setFriendStatus @6: FriendStatus;
                setNumPendingUserRequests @7: UInt64;
                setOptLastIncomingMoveToken @8: OptLastIncomingMoveToken;
                setLiveness @9: FriendLivenessReport;
        }
}

struct PkFriendReportMutation {
        friendPublicKey @0: PublicKey;
        friendReportMutation @1: FriendReportMutation;
}

# A FunderReportMutation. Could be applied over a FunderReport to make small changes.
struct FunderReportMutation {
        union {
                setAddress @0: SetAddressReport;
                addFriend @1: AddFriendReport;
                removeFriend @2: PublicKey;
                pkFriendReportMutation @3: PkFriendReportMutation;
                setNumReadyReceipts @4: UInt64;
        }
}


############################################################################
##### IndexClient report
############################################################################

struct IndexClientReport {
        indexServers @0: List(IndexServerAddress);
        optConnectedServer: union {
                indexServerAddress @1: IndexServerAddress;
                empty @2: Void;
        }
}

struct IndexClientReportMutation {
        union {
                addIndexServer @0: IndexServerAddress;
                removeIndexServer @1: IndexServerAddress;
                setConnectedServer: union {
                        indexServerAddress @2: IndexServerAddress;
                        empty @3: Void;
                }
        }
}


############################################################################
##### Node report
############################################################################

struct NodeReport {
        funderReport @0: FunderReport;
        indexClientReport @1: IndexClientReport;
}

struct NodeReportMutation {
        union {
                funder @0: FunderReportMutation;
                indexClient @1: IndexClientReportMutation;
        }
}
