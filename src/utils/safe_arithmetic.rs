
#[allow(unused)]
/// Add u32 to i32. In case of an overflow, return None.
fn checked_add_i32_u32(a: i32, b: u32) -> Option<i32> {
    let b_half = (b / 2) as i32;
    let b_rem = (b % 2) as i32;

    Some(a.checked_add(b_half)?.checked_add(b_half)?
        .checked_add(b_rem)?)
}

#[allow(unused)]
/// Add u64 to i64. In case of an overflow, return None.
fn checked_add_i64_u64(a: i64, b: u64) -> Option<i64> {
    let b_half = (b / 2) as i64;
    let b_rem = (b % 2) as i64;

    Some(a.checked_add(b_half)?.checked_add(b_half)?
        .checked_add(b_rem)?)
}

#[allow(unused)]
/// Add u128 to i128. In case of an overflow, return None.
fn checked_add_i128_u128(a: i128, b: u128) -> Option<i128> {
    let b_half = (b / 2) as i128;
    let b_rem = (b % 2) as i128;


    Some(a.checked_add(b_half)?.checked_add(b_half)?
        .checked_add(b_rem)?)

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checked_add_i32_u32() {
        assert_eq!(checked_add_i32_u32(1,1), Some(2));
        assert_eq!(checked_add_i32_u32(0x7fffffff,1), None);
        assert_eq!(checked_add_i32_u32(0x7ffffffe,2), None);
        assert_eq!(checked_add_i32_u32(0x7ffffffe,3), None);
        assert_eq!(checked_add_i32_u32(0x7ffffffe,1), Some(0x7fffffff));
        assert_eq!(checked_add_i32_u32(-1,3), Some(2));
        assert_eq!(checked_add_i32_u32(-5,3), Some(-2));
    }

    #[test]
    fn test_checked_add_i64_u64() {
        assert_eq!(checked_add_i64_u64(1,1), Some(2));
        assert_eq!(checked_add_i64_u64(0x7fff_ffff_ffff_ffff,1), None);
        assert_eq!(checked_add_i64_u64(-1,3), Some(2));
        assert_eq!(checked_add_i64_u64(-5,3), Some(-2));
    }

    #[test]
    fn test_checked_add_i128_u128() {
        assert_eq!(checked_add_i128_u128(1,1), Some(2));
        assert_eq!(checked_add_i128_u128(0x7fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff,1), None);
        assert_eq!(checked_add_i128_u128(-1,3), Some(2));
        assert_eq!(checked_add_i128_u128(-5,3), Some(-2));
    }
}
