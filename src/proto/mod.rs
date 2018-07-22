//! Pieces pertaining to the `CSwitch` protocol.
//!
//! This module define `CSwitch` messages's format.

// #![deny(warnings)]

use std::{io, mem};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSendPrice<T> {
    pub base: T,
    pub multiplier: T,
}

impl<T:PartialOrd> LinearSendPrice<T> {
    pub fn bytes_count() -> usize {
        mem::size_of::<T>() * 2
    }

    pub fn smaller_than(&self, other: &Self) -> bool {
        if self.base < other.base {
            self.multiplier <= other.multiplier
        } else if self.base == other.base {
            self.multiplier < other.multiplier
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_send_price_ord() {
        let lsp1 = LinearSendPrice {
            base: 3,
            multiplier: 3,
        };

        let lsp2 = LinearSendPrice {
            base: 5,
            multiplier: 5,
        };

        let lsp3 = LinearSendPrice {
            base: 4,
            multiplier: 6,
        };

        let lsp4 = LinearSendPrice {
            base: 4,
            multiplier: 7,
        };

        let lsp5 = LinearSendPrice {
            base: 5,
            multiplier: 6,
        };

        assert!(lsp1.smaller_than(&lsp2));
        assert!(lsp1.smaller_than(&lsp3));

        assert!(!(lsp2.smaller_than(&lsp3)));
        assert!(!(lsp3.smaller_than(&lsp2)));

        assert!(lsp3.smaller_than(&lsp4));
        assert!(lsp3.smaller_than(&lsp5));
    }
}

