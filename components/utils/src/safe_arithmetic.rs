
pub trait SafeArithmetic: Copy {
    type Unsigned;

    fn checked_add_unsigned(self, u: Self::Unsigned) -> Option<Self>;
    fn checked_sub_unsigned(self, u: Self::Unsigned) -> Option<Self>;
    fn saturating_add_unsigned(self, u: Self::Unsigned) -> Self;
    fn saturating_sub_unsigned(self, u: Self::Unsigned) -> Self;
}

macro_rules! impl_safe_arithmetic {
    ( $i:ty, $u:ty ) => {
        impl SafeArithmetic for $i {
            type Unsigned = $u;
            fn checked_add_unsigned(self, u: $u) -> Option<$i> {
                let u_half = (u / 2) as $i;
                let u_rem = (u % 2) as $i;

                self.checked_add(u_half)?.checked_add(u_half)?
                    .checked_add(u_rem)
            }

            fn checked_sub_unsigned(self, u: $u) -> Option<$i> {
                let u_half = (u / 2) as $i;
                let u_rem = (u % 2) as $i;
                self.checked_sub(u_half)?.checked_sub(u_half)?
                    .checked_sub(u_rem)
            }
            fn saturating_add_unsigned(self, u: $u) -> $i {
                let u_half = (u / 2) as $i;
                let u_rem = (u % 2) as $i;

                self.saturating_add(u_half).saturating_add(u_half)
                    .saturating_add(u_rem)
            }
            fn saturating_sub_unsigned(self, u: $u) -> $i {
                let u_half = (u / 2) as $i;
                let u_rem = (u % 2) as $i;

                self.saturating_sub(u_half).saturating_sub(u_half)
                    .saturating_sub(u_rem)
            }
        }
    }
}

impl_safe_arithmetic!(i8, u8);
impl_safe_arithmetic!(i16, u16);
impl_safe_arithmetic!(i32, u32);
impl_safe_arithmetic!(i64, u64);
impl_safe_arithmetic!(i128, u128);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_arithmetic_u8() {
        assert_eq!(8_i8.checked_add_unsigned(1_u8), Some(9_i8));
        assert_eq!((-3_i8).checked_add_unsigned(1_u8), Some(-2_i8));
        assert_eq!((-3_i8).checked_add_unsigned(5_u8), Some(2_i8));
        assert_eq!(127_i8.checked_add_unsigned(1_u8), None);
        assert_eq!((-2_i8).checked_add_unsigned(255_u8), None);
        assert_eq!(126_i8.checked_add_unsigned(1_u8), Some(127_i8));

        assert_eq!(0_i8.checked_sub_unsigned(1_u8), Some(-1_i8));
        assert_eq!((-1_i8).checked_sub_unsigned(1_u8), Some(-2_i8));
        assert_eq!((-127_i8).checked_sub_unsigned(1_u8), Some(-128_i8));
        assert_eq!((-128_i8).checked_sub_unsigned(1_u8), None);
        assert_eq!(3_i8.checked_sub_unsigned(255_u8), None);

        assert_eq!(1_i8.saturating_add_unsigned(1_u8), 2_i8);
        assert_eq!(3_i8.saturating_add_unsigned(255_u8), 127_i8);
        assert_eq!((-2_i8).saturating_add_unsigned(255_u8), 127_i8);
        assert_eq!((-2_i8).saturating_add_unsigned(255_u8), 127_i8);
        assert_eq!((-3_i8).saturating_add_unsigned(1_u8), -2_i8);

        assert_eq!((-3_i8).saturating_sub_unsigned(1_u8), -4_i8);
        assert_eq!(1_i8.saturating_sub_unsigned(1_u8), 0_i8);
        assert_eq!(1_i8.saturating_sub_unsigned(255_u8), -128_i8);
        assert_eq!(1_i8.saturating_sub_unsigned(128_u8), -127_i8);
    }
}

