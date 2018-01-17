//! Pieces pertaining to the `CSwitch` protocol.
//!
//! This module define `CSwitch` messages's format.

// #![deny(warnings)]

use std::io;

use bytes::Bytes;

pub enum ProtoError {
    /// The error occurred in encoding/decoding capnp message.
    Capnp(::capnp::Error),

    /// The error occurred in converting some types from bytes.
    InvalidLength,
}

/// The convenient methods used to encode/decode capnp messages.
///
// TODO: Can the generic associated types allow us provide default
// implementation of `decode` and `encode` here?
pub trait Schema<'a>: Sized {
    type Reader: 'a;
    type Writer: 'a;

    fn decode(buffer: Bytes) -> Result<Self, SchemaError>;
    fn encode(&self) -> Result<Bytes, SchemaError>;
    fn read(from: &Self::Reader) -> Result<Self, SchemaError>;
    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError>;
}

pub mod schema_impl;

pub mod channeler;
pub mod indexer;
pub mod networker;

#[derive(Debug)]
pub enum SchemaError {
    Io(io::Error),
    Capnp(::capnp::Error),
    Invalid,
    NotInSchema,
}

impl From<io::Error> for SchemaError {
    #[inline]
    fn from(e: io::Error) -> SchemaError {
        SchemaError::Io(e)
    }
}

impl From<::capnp::Error> for SchemaError {
    #[inline]
    fn from(e: ::capnp::Error) -> SchemaError {
        SchemaError::Capnp(e)
    }
}

impl From<::capnp::NotInSchema> for SchemaError {
    #[inline]
    fn from(_: ::capnp::NotInSchema) -> SchemaError {
        SchemaError::NotInSchema
    }
}
