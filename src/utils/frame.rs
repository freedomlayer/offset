use app_manager_capnp::frame;
use std::io;

/// Read a frame, return data in that frame. Block thread until reading is done.
pub fn read_frame<R: io::Read>(r: &mut R) -> Result<Vec<u8>, ::capnp::Error> {
    let message = ::capnp::serialize::read_message(r, Default::default())?;
    let frame = message.get_root::<frame::Reader>()?;
    Ok(frame.get_data()?.to_vec())
}

/// Write `buf` as a frame, Block thread until writing is done.
pub fn write_frame<W: io::Write>(w: &mut W, buf: &[u8]) -> Result<(), io::Error> {
    let mut message = ::capnp::message::Builder::new_default();
    message.init_root::<frame::Builder>().set_data(buf);
    ::capnp::serialize::write_message(w, &message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_frame() {
        let data = [1, 2, 3, 4, 5];

        let mut writer = Vec::new();
        super::write_frame(&mut writer, &data).unwrap();

        let mut reader = writer.as_slice();
        let vec = super::read_frame(&mut reader).unwrap();

        assert_eq!(data, vec.as_slice());
    }
}
