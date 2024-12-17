#[derive(PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn get_native_endian() -> Endian {
        #[cfg(target_endian = "little")]
        {
            Endian::Little
        }
        #[cfg(target_endian = "big")]
        {
            Endian::Big
        }
    }
}

impl From<Endian> for u8 {
    fn from(val: Endian) -> Self {
        match val {
            Endian::Little => 0u8,
            Endian::Big => 1u8
        }
    }
}

impl From<u8> for Endian {
    fn from(value: u8) -> Self {
        match value {
            0u8 => Endian::Little,
            1u8 => Endian::Big,
            _ => panic!("unknown endian byte {}", value)
        }
    }
}