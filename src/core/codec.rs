use crate::core::{bulk_load_u32, CODE_ESCAPE, U64_SIZE};
use crate::core::symbol::Symbol;
use crate::core::symbol_table::SymbolTable;
use crate::util::endian::Endian;

pub struct Encoder<'a> {
    symbol_table: &'a Box<dyn SymbolTable>,
}

impl Encoder<'_> {
    pub fn from_table(table: &Box<dyn SymbolTable>) -> Encoder {
        Encoder { symbol_table: table }
    }

    pub fn encode_str(&self, str: &str) -> Vec<u8> {
        let mut buf = vec![0; str.len() << 1];
        let (mut pos_in, mut pos_out) = (0, 0);
        while pos_in < str.len() {
            let target = Symbol::from_str(&str[pos_in..]);
            buf[pos_out + 1] = target.first() as u8;
            let (code, s_len, out_len) = self.symbol_table.encode_for(&target);
            buf[pos_out] = code;
            pos_out += out_len;
            pos_in += s_len;
        }
        buf.truncate(pos_out);
        buf
    }

    pub fn encode(&self, str: &str, include_table: bool) -> Vec<u8> {
        let mut buf = self.encode_str(str);
        if include_table {
            let mut table_buf = self.symbol_table.dump();
            let mut buf_with_tab = Vec::with_capacity(table_buf.len() + table_buf.len());
            buf_with_tab.append(&mut table_buf);
            buf_with_tab.append(&mut buf);
            buf_with_tab
        } else {
            buf
        }
    }
}

pub struct Decoder {
    symbols: [u64; CODE_ESCAPE as usize],
    lens: [u8; CODE_ESCAPE as usize],
}

impl Decoder {
    pub fn from_table(table: &Box<dyn SymbolTable>) -> Decoder {
        let mut symbols = [0u64; CODE_ESCAPE as usize];
        let mut lens = [0u8; CODE_ESCAPE as usize];
        for i in 0..table.len() {
            let s = table.get_symbol(i as u16);
            symbols[i] = s.as_u64();
            lens[i] = s.length() as u8;
        }
        Decoder { symbols, lens }
    }

    pub fn from_table_bytes(buf: &Vec<u8>) -> (usize, Decoder) {
        let mut symbols = [0u64; CODE_ESCAPE as usize];
        let mut lens = [0u8; CODE_ESCAPE as usize];
        let encode_endian = Endian::from_u8(*buf.get(0).unwrap());
        let len_histo = &buf[1..9];
        let (mut pos, mut code) = (9, 0usize);
        for len in 1..=Symbol::MAX_LEN {
            for _ in 0..len_histo[len - 1] {
                let mut num = 0u64;
                if Endian::get_native_endian() != encode_endian {
                    num |= *buf.get(pos).unwrap() as u64;
                    for i in 1..len {
                        num <<= 8;
                        num |= *buf.get(pos + i).unwrap() as u64;
                    }
                } else {
                    num |= *buf.get(pos + len - 1).unwrap() as u64;
                    for i in (0..len - 1).rev() {
                        num <<= 8;
                        num |= *buf.get(pos + i).unwrap() as u64;
                    }
                }
                symbols[code] = num;
                lens[code] = len as u8;
                code += 1;
                pos += len;
            }
        }
        (pos, Decoder { symbols, lens })
    }

    /// safe decode method
    pub fn decode_with_tab(table: &Box<dyn SymbolTable>, buf: &Vec<u8>) -> String {
        let mut str = String::with_capacity(buf.len() * 4);
        let mut pos = 0;
        while pos < buf.len() {
            let b = buf.get(pos).unwrap();
            pos += 1;
            if *b == 255 {
                str.push(*buf.get(pos).unwrap() as char);
                pos += 1;
            } else {
                str.push_str(&table.get_symbol(*b as u16).to_string());
            }
        }
        str
    }

    /// decode method that uses the unsafe method
    pub fn decode(&self, str_buf: &Vec<u8>) -> String {
        let (mut pos_in, mut pos_out) = (0, 0);
        let mut decode_buf = vec![0u8; str_buf.len() * Symbol::MAX_LEN];
        unsafe {
            let out = decode_buf.as_mut_ptr();
            while pos_in + 4 < str_buf.len() {
                let next_block = bulk_load_u32(&str_buf[pos_in..pos_in + 4]);
                let escape_mask = (next_block & 0x80808080) & ((((!next_block) & 0x7F7F7F7F) + 0x7F7F7F7F) ^ 0x80808080);
                if escape_mask == 0 {
                    self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                    self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                    self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                    self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                } else {
                    let mut first_escape_pos = escape_mask.trailing_zeros() >> 3;
                    while first_escape_pos > 0 {
                        self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                        first_escape_pos -= 1;
                    }
                    decode_buf[pos_out] = str_buf[pos_in + 1];
                    pos_in += 2;
                    pos_out += 1;
                }
            }
            while pos_in < str_buf.len() {
                if str_buf[pos_in] != CODE_ESCAPE {
                    self.unaligned_store(&mut pos_in, &mut pos_out, &str_buf, out);
                } else {
                    decode_buf[pos_out] = str_buf[pos_in + 1];
                    pos_in += 2;
                    pos_out += 1;
                }
            }
            decode_buf.truncate(pos_out);
            String::from_utf8_unchecked(decode_buf)
        }
    }

    #[inline(always)]
    unsafe fn unaligned_store(&self, pos_in: &mut usize, pos_out: &mut usize, str_in: &Vec<u8>, out: *mut u8) {
        let code = str_in[*pos_in] as usize;
        std::ptr::copy_nonoverlapping(self.symbols[code].to_ne_bytes().as_ptr(), out.add(*pos_out), U64_SIZE);
        *pos_in += 1;
        *pos_out += self.lens[code] as usize;
    }
}

#[cfg(test)]
mod test {
    use crate::core::codec::{Decoder, Encoder};
    use crate::core::symbol_table::SymbolTableBuilder;

    #[test]
    pub fn test_decode_with_dump_table() {
        let test_str = "paqvawflxucgajxfzxwooypirnzkahobfvxzhrerdwzkerwwolqfbafwslwhsvuitbtgkvnjrdr";
        let symbol_table = SymbolTableBuilder::build_from(test_str);
        let encoder = Encoder::from_table(&symbol_table);
        let buf = symbol_table.dump();
        let (table_end_pos, decoder) = Decoder::from_table_bytes(&buf);
        assert_eq!(buf.len(), table_end_pos);
        let encode_buf = encoder.encode(test_str, false);
        let decode_str = decoder.decode(&encode_buf);
        assert_eq!(test_str, decode_str);
    }
}