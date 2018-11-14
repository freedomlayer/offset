
pub trait SafeSignedArithmetic: Copy {
    type Unsigned;

    fn safe_abs(self) -> Self::Unsigned;
    fn checked_add_unsigned(self, u: Self::Unsigned) -> Option<Self>;
    fn checked_sub_unsigned(self, u: Self::Unsigned) -> Option<Self>;
    fn saturating_add_unsigned(self, u: Self::Unsigned) -> Self;
    fn saturating_sub_unsigned(self, u: Self::Unsigned) -> Self;
}


pub trait SafeUnsignedArithmetic: Copy {
    type Signed;

    fn checked_add_signed(self, u: Self::Signed) -> Option<Self>;
    fn checked_sub_signed(self, u: Self::Signed) -> Option<Self>;
    fn saturating_add_signed(self, u: Self::Signed) -> Self;
    fn saturating_sub_signed(self, u: Self::Signed) -> Self;
}

macro_rules! impl_safe_signed_arithmetic {
    ( $i:ty, $u:ty ) => {
        impl SafeSignedArithmetic for $i {
            type Unsigned = $u;
            fn safe_abs(self) -> $u {
                if self >= 0 {
                    (self as $u)
                } else {
                    let s_half: $i = self / 2;
                    let s_rem = self % 2;
                    ((-s_half) as $u) * 2 + ((-s_rem) as $u)
                }
            }
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

impl_safe_signed_arithmetic!(i8, u8);
impl_safe_signed_arithmetic!(i16, u16);
impl_safe_signed_arithmetic!(i32, u32);
impl_safe_signed_arithmetic!(i64, u64);
impl_safe_signed_arithmetic!(i128, u128);


macro_rules! impl_safe_unsigned_arithmetic {
    ( $u:ty, $i:ty ) => {
        impl SafeUnsignedArithmetic for $u {
            type Signed = $i;
            fn checked_add_signed(self, s: $i) -> Option<$u> {
                if s >= 0 {
                    self.checked_add(s as $u)
                } else {
                    self.checked_sub(s.safe_abs() as $u)
                }
            }

            fn checked_sub_signed(self, s: $i) -> Option<$u> {
                if s >= 0 {
                    self.checked_sub(s as $u)
                } else {
                    self.checked_add(s.safe_abs() as $u)
                }
            }
            fn saturating_add_signed(self, s: $i) -> $u {
                if s >= 0 {
                    self.saturating_add(s as $u)
                } else {
                    self.saturating_sub(s.safe_abs() as $u)
                }
            }
            fn saturating_sub_signed(self, s: $i) -> $u {
                if s >= 0 {
                    self.saturating_sub(s as $u)
                } else {
                    self.saturating_add(s.safe_abs() as $u)
                }
            }
        }
    }
}

impl_safe_unsigned_arithmetic!(u8, i8);
impl_safe_unsigned_arithmetic!(u16, i16);
impl_safe_unsigned_arithmetic!(u32, i32);
impl_safe_unsigned_arithmetic!(u64, i64);
impl_safe_unsigned_arithmetic!(u128, i128);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_unsigned_arithmetic_u8() {
        assert_eq!(9_i8.safe_abs(), 9_u8);
        assert_eq!(0_i8.safe_abs(), 0_u8);
        assert_eq!((-1_i8).safe_abs(), 1_u8);
        assert_eq!((-9_i8).safe_abs(), 9_u8);
        assert_eq!((-9_i8).safe_abs(), 9_u8);
        assert_eq!((-55_i8).safe_abs(), 55_u8); 
        assert_eq!((-128_i8).safe_abs(), 128_u8);   // The difficult case

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

    #[test]
    fn test_safe_signed_arithmetic_u8() {
        assert_eq!(8_u8.checked_add_signed(1_i8), Some(9_u8));
        assert_eq!((3_u8).checked_add_signed(-1_i8), Some(2_u8));
        assert_eq!((0_u8).checked_add_signed(-1_i8), None);
        assert_eq!((254_u8).checked_add_signed(1_i8), Some(255_u8));
        assert_eq!((255_u8).checked_add_signed(1_i8), None);

        assert_eq!(2_u8.checked_sub_signed(1_i8), Some(1_u8));
        assert_eq!(3_u8.checked_sub_signed(4_i8), None);
        assert_eq!(3_u8.checked_sub_signed(-4_i8), Some(7_u8));
        assert_eq!(250_u8.checked_sub_signed(-6_i8), None);
        assert_eq!(250_u8.checked_sub_signed(-128_i8), None);

        assert_eq!(1_u8.saturating_add_signed(1_i8), 2_u8);
        assert_eq!(255_u8.saturating_add_signed(1_i8), 255_u8);
        assert_eq!(250_u8.saturating_add_signed(8_i8), 255_u8);
        assert_eq!(7_u8.saturating_add_signed(-8_i8), 0_u8);
        assert_eq!(7_u8.saturating_add_signed(-6_i8), 1_u8);

        assert_eq!((3_u8).saturating_sub_signed(1_i8), 2_u8);
        assert_eq!((3_u8).saturating_sub_signed(4_i8), 0_u8);
        assert_eq!((254_u8).saturating_sub_signed(-1_i8), 255_u8);
        assert_eq!((254_u8).saturating_sub_signed(-3_i8), 255_u8);
    }
}

