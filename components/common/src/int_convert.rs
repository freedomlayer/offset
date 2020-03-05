use std::u32;

#[cfg(any(
    target_pointer_width = "8",
    target_pointer_width = "16",
    target_pointer_width = "32"
))]
pub fn usize_to_u32(num: usize) -> Option<u32> {
    Some(num as u32)
}

#[cfg(target_pointer_width = "64")]
pub fn usize_to_u32(num: usize) -> Option<u32> {
    if num > u32::MAX as usize {
        None
    } else {
        Some(num as u32)
    }
}

// TODO: Possibly make this function constant?
// TODO: Possibly replace Option<u64> with u64 in the future?
#[cfg(any(
    target_pointer_width = "8",
    target_pointer_width = "16",
    target_pointer_width = "32",
    target_pointer_width = "64"
))]
pub fn usize_to_u64(num: usize) -> Option<u64> {
    Some(num as u64)
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
pub fn u32_to_usize(num: u32) -> Option<usize> {
    Some(num as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usize_to_u32() {
        assert_eq!(usize_to_u32(0_usize), Some(0u32));
        assert_eq!(usize_to_u32(1_usize), Some(1u32));
        assert_eq!(usize_to_u32(0xffff_ffff_usize), Some(0xffff_ffffu32));
        assert_eq!(usize_to_u32(0x1_0000_0000_usize), None);
    }

    #[test]
    fn test_usize_to_u64() {
        assert_eq!(usize_to_u64(0usize), Some(0u64));
        assert_eq!(usize_to_u64(1usize), Some(1u64));
        assert_eq!(
            usize_to_u64(0xffff_ffff_ffff_ffff_usize),
            Some(0xffff_ffff_ffff_ffffu64)
        );
    }

    #[test]
    fn test_u32_to_usize() {
        assert_eq!(u32_to_usize(0u32), Some(0usize));
        assert_eq!(u32_to_usize(1u32), Some(1usize));
        assert_eq!(u32_to_usize(0xffff_ffff_u32), Some(0xffff_ffff_usize));
    }
}
