#[macro_export]
macro_rules! define_fixed_bytes {
    ($name:ident, $len:expr) => {
        #[derive(
            Default, Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name([u8; $len]);

        impl $name {
            #[allow(unused)]
            #[inline]
            pub fn as_array_ref(&self) -> &[u8; $len] {
                &self.0
            }

            /// Formatting for `Debug` and `Display`.
            fn format(&self) -> String {
                let upper_hex = self
                    .as_ref()
                    .iter()
                    .map(|byte| format!("{:02X}", byte))
                    .collect::<Vec<_>>();

                upper_hex.join("")
            }
        }
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
            type Error = ();

            #[inline]
            fn try_from(src: &'a [u8]) -> Result<$name, ()> {
                if src.len() < $len {
                    Err(())
                } else {
                    let mut inner = [0x00u8; $len];
                    inner.copy_from_slice(&src[..$len]);
                    Ok($name(inner))
                }
            }
        }
        impl<'a> ::std::convert::TryFrom<&'a ::bytes::Bytes> for $name {
            type Error = ();

            #[inline]
            fn try_from(src: &'a ::bytes::Bytes) -> Result<$name, ()> {
                if src.len() < $len {
                    Err(())
                } else {
                    let mut inner = [0x00u8; $len];
                    inner.copy_from_slice(&src[..$len]);
                    Ok($name(inner))
                }
            }
        }
    };
}
