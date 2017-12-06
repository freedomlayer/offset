use std::io;

use bytes::{BigEndian, Bytes, BytesMut, Buf, BufMut};

pub mod channeler_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/channeler_capnp.rs"));
}

use channeler_capnp::{custom_u_int128, custom_u_int256, custom_u_int512};

/// Fill the components of `CustomUInt128` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `src`.
#[inline]
pub fn write_custom_u_int128(dest: &mut custom_u_int128::Builder, src: &Bytes)
    -> Result<(), io::Error> {
    let mut rdr = io::Cursor::new(src);

    dest.set_x0(rdr.get_u64::<BigEndian>());
    dest.set_x1(rdr.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt128` reader.
///
/// # Panics
///
/// This function panics if there is not enough remaining capacity in `dest`.
#[inline]
pub fn read_custom_u_int128(dest: &mut BytesMut, src: &custom_u_int128::Reader)
    -> Result<(), io::Error> {

    dest.put_u64::<BigEndian>(src.get_x0());
    dest.put_u64::<BigEndian>(src.get_x1());

    Ok(())
}

/// Fill the components of `CustomUInt256` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `src`.
#[inline]
pub fn write_custom_u_int256(dest: &mut custom_u_int256::Builder, src: &Bytes)
    -> Result<(), io::Error> {
    let mut rdr = io::Cursor::new(src);

    dest.set_x0(rdr.get_u64::<BigEndian>());
    dest.set_x1(rdr.get_u64::<BigEndian>());
    dest.set_x2(rdr.get_u64::<BigEndian>());
    dest.set_x3(rdr.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt256` reader.
///
/// # Panics
///
/// This function panics if there is not enough remaining capacity in `dest`.
#[inline]
pub fn read_custom_u_int256(dest: &mut BytesMut, src: &custom_u_int256::Reader)
    -> Result<(), io::Error> {

    dest.put_u64::<BigEndian>(src.get_x0());
    dest.put_u64::<BigEndian>(src.get_x1());
    dest.put_u64::<BigEndian>(src.get_x2());
    dest.put_u64::<BigEndian>(src.get_x3());

    Ok(())
}

/// Fill the components of `CustomUInt512` from given bytes.
///
/// # Panics
///
/// This function panics if there is not enough remaining data in `src`.
#[inline]
pub fn write_custom_u_int512(dest: &mut custom_u_int512::Builder, src: &Bytes)
                             -> Result<(), io::Error> {
    let mut rdr = io::Cursor::new(src);

    dest.set_x0(rdr.get_u64::<BigEndian>());
    dest.set_x1(rdr.get_u64::<BigEndian>());
    dest.set_x2(rdr.get_u64::<BigEndian>());
    dest.set_x3(rdr.get_u64::<BigEndian>());
    dest.set_x4(rdr.get_u64::<BigEndian>());
    dest.set_x5(rdr.get_u64::<BigEndian>());
    dest.set_x6(rdr.get_u64::<BigEndian>());
    dest.set_x7(rdr.get_u64::<BigEndian>());

    Ok(())
}

/// Read the underlying bytes from given `CustomUInt512` reader.
///
/// # Panics
///
/// This function panics if there is not enough remaining capacity in `dest`.
#[inline]
pub fn read_custom_u_int512(dest: &mut BytesMut, src: &custom_u_int512::Reader)
    -> Result<(), io::Error> {

    dest.put_u64::<BigEndian>(src.get_x0());
    dest.put_u64::<BigEndian>(src.get_x1());
    dest.put_u64::<BigEndian>(src.get_x2());
    dest.put_u64::<BigEndian>(src.get_x3());
    dest.put_u64::<BigEndian>(src.get_x4());
    dest.put_u64::<BigEndian>(src.get_x5());
    dest.put_u64::<BigEndian>(src.get_x6());
    dest.put_u64::<BigEndian>(src.get_x7());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Bytes, BytesMut};
    use super::{custom_u_int128, custom_u_int256, custom_u_int512};
    use super::{read_custom_u_int128, write_custom_u_int128,
                read_custom_u_int256, write_custom_u_int256,
                read_custom_u_int512, write_custom_u_int512};
    #[test]
    fn test_custom_u_int128() {
        let mut message = ::capnp::message::Builder::new_default();

        let mut buf_src = Bytes::from_static(
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
              0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f]);

        assert_eq!(buf_src.len(), 16);

        let mut num_u128 = message.init_root::<custom_u_int128::Builder>();
        write_custom_u_int128(&mut num_u128, &buf_src).unwrap();

        let mut buf_dest = BytesMut::with_capacity(16usize);
        read_custom_u_int128(&mut buf_dest, &num_u128.borrow_as_reader()).unwrap();

        assert_eq!(&buf_src, &buf_dest);
    }

    #[test]
    fn test_custom_u_int256() {
        let mut message = ::capnp::message::Builder::new_default();

        let mut buf_src = Bytes::from_static(
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
              0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
              0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
              0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f]);
        assert_eq!(buf_src.len(), 32);

        let mut num_u256 = message.init_root::<custom_u_int256::Builder>();
        write_custom_u_int256(&mut num_u256, &buf_src).unwrap();

        let mut buf_dest = BytesMut::with_capacity(32usize);
        read_custom_u_int256(&mut buf_dest, &num_u256.borrow_as_reader()).unwrap();

        assert_eq!(&buf_src, &buf_dest);
    }

    #[test]
    fn test_custom_u_int512() {
        let mut message = ::capnp::message::Builder::new_default();

        let mut buf_src = Bytes::from_static(
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
              0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
              0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
              0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
              0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
              0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
              0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
              0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f]);
        assert_eq!(buf_src.len(), 64);

        let mut num_u512 = message.init_root::<custom_u_int512::Builder>();
        write_custom_u_int512(&mut num_u512, &buf_src).unwrap();

        let mut buf_dest = BytesMut::with_capacity(64usize);
        read_custom_u_int512(&mut buf_dest, &num_u512.borrow_as_reader()).unwrap();

        assert_eq!(&buf_src, &buf_dest);
    }
}

