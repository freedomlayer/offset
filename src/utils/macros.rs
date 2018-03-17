//! Utility macros

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TryFromBytesError;

impl ::std::fmt::Display for TryFromBytesError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.write_str("could not convert byte slice to a fixed byte type")
    }
}

impl ::std::error::Error for TryFromBytesError {
    #[inline]
    fn description(&self) -> &str {
        "could not convert byte slice to a fixed byte type"
    }

    #[inline]
    fn cause(&self) -> Option<&::std::error::Error> { None }
}

#[macro_export]
macro_rules! define_fixed_bytes {
    ($name:ident, $len:expr) => {
        #[derive(Default, Debug, Clone, Eq, PartialEq, Hash)]
        pub struct $name([u8; $len]);

        impl AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }
        impl ::std::ops::Deref for $name {
            type Target = [u8];
            #[inline]
            fn deref(&self) -> &[u8] {
                &self.0
            }
        }
        impl ::std::ops::DerefMut for $name {
            #[inline]
            fn deref_mut(&mut self) -> &mut [u8] {
                &mut self.0
            }
        }
        impl<'a> ::std::convert::From<&'a [u8; $len]> for $name {
            #[inline]
            fn from(src: &'a [u8; $len]) -> $name {
                let mut inner = [0x00u8; $len];
                inner.copy_from_slice(&src[..$len]);
                $name(inner)
            }
        }
        impl<'a> ::std::convert::TryFrom<&'a [u8]> for $name {
            type Error = ::utils::TryFromBytesError;

            #[inline]
            fn try_from(src: &'a [u8]) -> Result<$name, ::utils::TryFromBytesError> {
                if src.len() < $len {
                    Err(::utils::TryFromBytesError)
                } else {
                    let mut inner = [0x00u8; $len];
                    inner.copy_from_slice(&src[..$len]);
                    Ok($name(inner))
                }
            }
        }
        impl<'a> ::std::convert::TryFrom<&'a ::bytes::Bytes> for $name {
            type Error = ::utils::TryFromBytesError;

            #[inline]
            fn try_from(src: &'a ::bytes::Bytes) -> Result<$name, ::utils::TryFromBytesError> {
                if src.len() < $len {
                    Err(::utils::TryFromBytesError)
                } else {
                    let mut inner = [0x00u8; $len];
                    inner.copy_from_slice(&src[..$len]);
                    Ok($name(inner))
                }
            }
        }
    };
}
