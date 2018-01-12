use utils::crypto::identity::{PublicKey, Signature};

// ===== Internal interfaces =====

pub enum FromSecurityModule {
    ResponseSign { signature: Signature },
    ResponsePublicKey { public_key: PublicKey },
}

pub enum ToSecurityModule {
    RequestSign { message: Vec<u8> },
    RequestPublicKey {},
}
