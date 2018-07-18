@0xc9191c4889ae128d;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# Application -> AppManager
struct AppRandNonce {
        appRandNonce @0: CustomUInt128;
        appPublicKey @1: CustomUInt256;
}

# AppManager -> Application
struct AppManagerRandNonce {
        appManagerRandNonce @0: CustomUInt128;
        appManagerPublicKey @1: CustomUInt256;
}


# Application -> AppManager
struct AppDh {
        appDhPublicKey @0: CustomUInt256;
        appManagerRandNonce @1: CustomUInt128;
        appKeySalt @2: CustomUInt256;
        signature @3: CustomUInt512;
}

# AppManager -> Application
struct AppManagerDh {
        appManagerDhPublicKey @0: CustomUInt256;
        appRandNonce @1: CustomUInt128;
        appManagerKeySalt @2: CustomUInt256;
        signature @3: CustomUInt512;
}
