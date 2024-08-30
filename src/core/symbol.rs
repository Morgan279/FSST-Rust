use std::cmp::{min, Ordering};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Add;

use crate::core::{CODE_MASK, fsst_hash, U64_SIZE, U64Bytes};

#[derive(Clone, Copy)]
pub struct Symbol {
    num: u64,
    icl: u64,
}

impl Symbol {
    pub(crate) const MAX_LEN: usize = U64_SIZE;
    const FREE_ICL: u64 = ((15 << 28) | ((CODE_MASK as u32) << 16)) as u64;

    pub fn from_str(str: &str) -> Symbol {
        Self::from_str_bytes(str.as_bytes())
    }

    pub fn from_str_bytes(bytes: &[u8]) -> Symbol {
        let len = min(bytes.len(), Self::MAX_LEN);
        let mut str_bytes = [0u8; Self::MAX_LEN];
        str_bytes[..len].copy_from_slice(&bytes[..len]);
        Symbol {
            num: Self::bytes_to_u64(str_bytes),
            icl: Self::compute_icl(CODE_MASK as u32, len as u32),
        }
    }

    pub fn from_byte_code(b: u8, code: u16) -> Symbol {
        Symbol {
            num: b as u64,
            icl: Self::compute_icl(code as u32, 1u32),
        }
    }

    pub fn from_str_num(num: u64, len: usize) -> Symbol {
        Symbol {
            num,
            icl: Self::compute_icl(CODE_MASK as u32, len as u32),
        }
    }

    pub fn free() -> Symbol {
        Symbol {
            num: 0,
            icl: Self::FREE_ICL,
        }
    }

    pub fn length(&self) -> usize {
        (self.icl >> 28) as usize
    }

    pub fn code(&self) -> u16 {
        ((self.icl >> 16) & CODE_MASK as u64) as u16
    }

    pub fn compute_icl(code: u32, len: u32) -> u64 {
        ((len << 28) | (code << 16) | ((8 - len) * 8)) as u64
    }

    pub fn set_code_len(&mut self, code: u16, len: usize) {
        self.icl = Self::compute_icl(code as u32, len as u32);
    }

    pub fn update_to(&mut self, target: &Symbol) {
        self.num = target.num;
        self.icl = target.icl;
    }

    pub fn reset(&mut self) {
        self.num = 0;
        self.icl = Self::FREE_ICL;
    }

    pub fn taken(&self) -> bool {
        self.icl < Self::FREE_ICL
    }

    pub fn prefix_match(&self, rhs: &Symbol) -> bool {
        self.icl >= rhs.icl && rhs.num == (self.num & (0xffffffffffffffff >> (rhs.icl as u8)))
    }

    pub fn word_match(&self, word: u64) -> bool {
        self.taken() && self.num == (word & (0xffffffffffffffff >> (self.icl as u8)))
    }

    pub fn as_u64(&self) -> u64 {
        self.num
    }

    pub fn first(&self) -> usize {
        (self.num & 0xff) as usize
    }

    pub fn first2(&self) -> usize {
        (self.num & 0xffff) as usize
    }

    pub fn hash(&self) -> usize {
        fsst_hash((self.num & 0xffffff) as usize)
    }

    fn bytes_to_u64(str_bytes: U64Bytes) -> u64 {
        unsafe {
            std::mem::transmute::<U64Bytes, u64>(str_bytes)
        }
    }

    fn u64_to_bytes(value: u64) -> U64Bytes {
        unsafe {
            std::mem::transmute::<u64, U64Bytes>(value)
        }
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = Self::u64_to_bytes(self.num.clone());
        write!(f, "{}", String::from_utf8_lossy(&s[0..self.length()]))
    }
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.num.hash(state);
        self.length().hash(state);
    }
}

impl Add for Symbol {
    type Output = Symbol;

    fn add(self, rhs: Self) -> Self::Output {
        let this_len = self.length();
        let concat_len = min(this_len + rhs.length(), Symbol::MAX_LEN);
        Symbol {
            num: (&rhs.num << (8 * this_len)) | &self.num,
            icl: Self::compute_icl(CODE_MASK as u32, concat_len as u32),
        }
    }
}

impl PartialEq<Self> for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.num == other.num && self.length() == other.length()
    }
}

impl Eq for Symbol {}

impl PartialOrd<Self> for Symbol {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.num.partial_cmp(&other.num)
    }
}

impl Ord for Symbol {
    fn cmp(&self, other: &Self) -> Ordering {
        self.num.cmp(&other.num)
    }
}

#[cfg(test)]
mod test {
    use crate::core::symbol::Symbol;

    #[test]
    pub fn test_symbol_add() {
        let s1 = Symbol::from_str("1234");
        assert_eq!("1234", s1.clone().to_string());
        let s2 = Symbol::from_str("567");
        assert_eq!("1234567", (s1.clone() + s2).to_string());
        let s3 = Symbol::from_str("56789");
        assert_ne!("123456789", (s1.clone() + s3.clone()).to_string());
        assert_eq!("12345678", (s1 + s3).to_string());
    }
}