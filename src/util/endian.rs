#[derive(PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn from_u8(b: u8) -> Endian {
        match b {
            0u8 => Endian::Little,
            1u8 => Endian::Big,
            _ => panic!("unknown endian byte {}", b)
        }
    }

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

impl Into<u8> for Endian {
    fn into(self) -> u8 {
        match self {
            Endian::Little => 0u8,
            Endian::Big => 1u8
        }
    }
}