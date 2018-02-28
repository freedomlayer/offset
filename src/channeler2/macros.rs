//! Utility macros

macro_rules! define_wrapped_bytes {
    ($name:ident, $len:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
                self.as_ref()
            }
        }
        impl ::std::ops::DerefMut for $name {
            #[inline]
            fn deref_mut(&mut self) -> &mut [u8] {
                &mut self.0
            }
        }
        impl<'a> ::std::convert::TryFrom<&'a [u8]> for $name {
            type Error = ();

            #[inline]
            fn try_from(src: &'a [u8]) -> Result<$name, ()> {
                if src.len() != $len {
                    Err(())
                } else {
                    let mut inner = [0x00u8; $len];
                    inner.copy_from_slice(src);
                    Ok($name(inner))
                }
            }
        }
    };
}
