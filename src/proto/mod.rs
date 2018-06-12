//! Pieces pertaining to the `CSwitch` protocol.
//!
//! This module define `CSwitch` messages's format.

// #![deny(warnings)]

use std::io;

use bytes::Bytes;

/// The convenient methods used to encode/decode capnp messages.
///
// TODO: Can the generic associated types allow us provide default
// implementation of `decode` and `encode` here?
pub trait Proto<'a>: Sized {
    type Reader: 'a;
    type Writer: 'a;

    fn decode<B: AsRef<[u8]>>(buffer: B) -> Result<Self, ProtoError>;
    fn encode(&self) -> Result<Bytes, ProtoError>;
    fn read(from: &Self::Reader) -> Result<Self, ProtoError>;
    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError>;
}

pub mod proto_impl;

pub mod common;
pub mod channeler;
pub mod funder;
pub mod networker;

#[derive(Debug)]
pub enum ProtoError {
    Io(io::Error),
    Capnp(::capnp::Error),
    Invalid,
    NotInSchema,
}

impl From<io::Error> for ProtoError {
    #[inline]
    fn from(e: io::Error) -> ProtoError {
        ProtoError::Io(e)
    }
}

impl From<::capnp::Error> for ProtoError {
    #[inline]
    fn from(e: ::capnp::Error) -> ProtoError {
        ProtoError::Capnp(e)
    }
}

impl From<::capnp::NotInSchema> for ProtoError {
    #[inline]
    fn from(_: ::capnp::NotInSchema) -> ProtoError {
        ProtoError::NotInSchema
    }
}
