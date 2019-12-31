// TODO: Could we have the length of the array as a const function?

#[macro_export]
macro_rules! define_fixed_bytes {
    ($name:ident, $len:expr) => {
        #[derive(Clone, Serialize, Deserialize)]
        // #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
        pub struct $name(#[serde(with = "BigArray")] [u8; $len]);

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

            pub const fn len() -> usize {
                $len
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

        impl PartialEq for $name {
            #[inline]
            fn eq(&self, other: &$name) -> bool {
                // TODO: Should we implement constant time comparison here?
                for i in 0..$len {
                    if self.0[i] != other.0[i] {
                        return false;
                    }
                }
                true
            }
        }

        impl Eq for $name {}

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> ::std::fmt::Result {
                std::fmt::Debug::fmt(&self.0[..], f)
            }
        }

        impl std::cmp::PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.0[..].cmp(&other.0[..]))
            }
        }

        impl std::cmp::Ord for $name {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0[..].cmp(&other.0[..])
            }
        }

        impl std::hash::Hash for $name {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.as_ref().hash(state);
            }
        }

        impl std::default::Default for $name {
            fn default() -> Self {
                Self::from(&[0u8; $len])
            }
        }
    };
}
