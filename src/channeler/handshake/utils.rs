//! Handshake-related Utilities and helpers.

use std::convert::TryFrom;

use ring::aead::{CHACHA20_POLY1305, OpeningKey, SealingKey};

use crypto::{
    dh::{DhPrivateKey, DhPublicKey, Salt, DH_PUBLIC_KEY_LEN, SALT_LEN},
    hash::sha_512_256,
    identity::PublicKey,
    rand_values::RandValue,
};
use proto::channeler::{ChannelId, CHANNEL_ID_LEN};

use super::{ChannelMetadata, HandshakeError};

pub fn finish_handshake(
    remote_public_key: PublicKey,
    sent_rand_nonce: RandValue,
    recv_rand_nonce: RandValue,
    sent_key_salt: Salt,
    recv_key_salt: Salt,
    remote_dh_public_key: DhPublicKey,
    local_dh_private_key: DhPrivateKey,
) -> Result<ChannelMetadata, HandshakeError> {
    let local_dh_public_key = local_dh_private_key
        .compute_public_key()
        .expect("failed to compute local public key");

    let (tx_cid, rx_cid) = derive_channel_id(
        &sent_rand_nonce,
        &recv_rand_nonce,
        &local_dh_public_key,
        &remote_dh_public_key,
    );

    let (tx_key, rx_key) = derive_channel_key(
        local_dh_private_key,
        remote_dh_public_key,
        sent_key_salt,
        recv_key_salt,
    )?;

    Ok(ChannelMetadata {
        remote_public_key,
        tx_cid,
        tx_key,
        rx_cid,
        rx_key,
    })
}

fn derive_channel_id(
    sent_rand_nonce: &RandValue,
    recv_rand_nonce: &RandValue,
    sent_dh_public_key: &DhPublicKey,
    recv_dh_public_key: &DhPublicKey,
) -> (ChannelId, ChannelId) {
    let tx_cid = {
        let mut b: Vec<u8> = Vec::with_capacity(2 * (DH_PUBLIC_KEY_LEN + SALT_LEN));

        b.extend_from_slice(&sent_dh_public_key);
        b.extend_from_slice(&recv_dh_public_key);
        b.extend_from_slice(&sent_rand_nonce);
        b.extend_from_slice(&recv_rand_nonce);

        ChannelId::try_from(&sha_512_256(&b)[..CHANNEL_ID_LEN])
            .expect("hash result length MUST greater or equal CHANNEL_ID_LEN")
    };

    let rx_cid = {
        let mut b: Vec<u8> = Vec::with_capacity(2 * (DH_PUBLIC_KEY_LEN + SALT_LEN));

        b.extend_from_slice(&recv_dh_public_key);
        b.extend_from_slice(&sent_dh_public_key);
        b.extend_from_slice(&recv_rand_nonce);
        b.extend_from_slice(&sent_rand_nonce);

        ChannelId::try_from(&sha_512_256(&b)[..CHANNEL_ID_LEN])
            .expect("hash result length MUST greater or equal CHANNEL_ID_LEN")
    };

    (tx_cid, rx_cid)
}

fn derive_channel_key(
    local_dh_private_key: DhPrivateKey,
    remote_dh_public_key: DhPublicKey,
    sent_key_salt: Salt,
    recv_key_salt: Salt,
) -> Result<(SealingKey, OpeningKey), HandshakeError> {
    let (tx_sym_key, rx_sym_key) = local_dh_private_key
        .derive_symmetric_key(remote_dh_public_key, sent_key_salt, recv_key_salt)
        .map_err(|_| HandshakeError::DeriveSymmetricKeyFailed)?;

    let tx_key = SealingKey::new(&CHACHA20_POLY1305, &tx_sym_key)
        .map_err(|_| HandshakeError::ConvertToSealingKeyFailed)?;
    let rx_key = OpeningKey::new(&CHACHA20_POLY1305, &rx_sym_key)
        .map_err(|_| HandshakeError::ConvertToOpeningKeyFailed)?;

    Ok((tx_key, rx_key))
}
