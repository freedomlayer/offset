
pub fn checked_as_u32(num: usize) -> Option<u32>{
    if num > u32::max_value() as usize {
        None
    }else{
        Some(num as u32)
    }
}
