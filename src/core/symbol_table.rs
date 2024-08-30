use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use crate::core::{CODE_BASE, CODE_MASK, CODE_MAX, fsst_hash, is_escape_code, LEN_BITS};
use crate::core::counter::Counter;
use crate::core::symbol::Symbol;
use crate::util::endian::Endian;

pub trait SymbolTable: SymbolTableClone + Display {
    fn add(&mut self, s: Symbol) -> bool;
    fn find_longest_symbol_code(&self, str_bytes: &[u8]) -> u16;
    fn get_symbol(&self, code: u16) -> &Symbol;
    fn encode_for(&self, target: &Symbol) -> (u8, usize, usize);
    fn len(&self) -> usize;
    fn clear(&mut self);
    fn finalize(&mut self);
    fn dump(&self) -> Vec<u8>;
}

pub trait SymbolTableClone {
    fn clone_box<'a>(&self) -> Box<dyn SymbolTable + 'a>
    where
        Self: 'a;
}

impl<T: Clone + SymbolTable> SymbolTableClone for T {
    fn clone_box<'a>(&self) -> Box<dyn SymbolTable + 'a>
    where
        Self: 'a,
    {
        Box::new(self.clone())
    }
}

#[derive(Clone, Copy)]
struct PerfectHashSymbolTable {
    // lookup table (only used during symbolTable construction, not during normal text compression)
    byte_codes: [u16; CODE_BASE as usize],

    // lookup table using the next two bytes (65536 codes), or just the next single byte
    short_codes: [u16; 65536],

    hash_table: [Symbol; PerfectHashSymbolTable::TABLE_SIZE],
    symbols: [Symbol; CODE_MAX as usize],
    len_histo: [u8; Symbol::MAX_LEN],
    symbol_num: u16,
    finalized: bool,
}

impl PerfectHashSymbolTable {
    const TABLE_SIZE: usize = 4096;

    pub fn new() -> PerfectHashSymbolTable {
        let unused = Symbol::from_byte_code(0, CODE_MASK);
        let mut symbols = [unused; CODE_MAX as usize];
        let mut byte_codes = [0u16; CODE_BASE as usize];
        for i in 0..CODE_BASE {
            let byte_code = (1 << LEN_BITS) | i;
            byte_codes[i as usize] = byte_code;
            symbols[i as usize] = Symbol::from_byte_code(i as u8, byte_code);
        }

        let mut short_codes = [0u16; 65536];
        for i in 0..short_codes.len() {
            short_codes[i] = (1 << LEN_BITS) | ((i as u16) & 0xff);
        }

        let len_histo = [0u8; Symbol::MAX_LEN];
        let hash_table = [Symbol::free(); PerfectHashSymbolTable::TABLE_SIZE];
        PerfectHashSymbolTable {
            byte_codes,
            short_codes,
            hash_table,
            symbols,
            len_histo,
            symbol_num: 0,
            finalized: false,
        }
    }

    fn hash_insert(&mut self, s: &Symbol) -> bool {
        let src_symbol = self.get_hash_symbol_mut(s.hash());
        if src_symbol.taken() {
            return false;
        }

        src_symbol.update_to(s);
        return true;
    }

    fn get_hash_symbol_mut(&mut self, hash_value: usize) -> &mut Symbol {
        &mut self.hash_table[Self::hash_idx(hash_value)]
    }

    fn get_hash_symbol(&self, hash_value: usize) -> &Symbol {
        &self.hash_table[Self::hash_idx(hash_value)]
    }

    fn hash_idx(hash_value: usize) -> usize {
        hash_value & (PerfectHashSymbolTable::TABLE_SIZE - 1)
    }
}

impl SymbolTable for PerfectHashSymbolTable {
    fn add(&mut self, mut s: Symbol) -> bool {
        let len = s.length();
        let code = CODE_BASE + self.symbol_num;
        s.set_code_len(code, len);
        if len == 1 {
            self.byte_codes[s.first()] = code | (1 << LEN_BITS); // len=1 (<<FSST_LEN_BITS)
        } else if len == 2 {
            self.short_codes[s.first2()] = code | (2 << LEN_BITS); // len=2 (<<FSST_LEN_BITS)
        } else if !self.hash_insert(&s) {
            return false;
        }

        self.symbols[code as usize] = s;
        self.symbol_num += 1;
        self.len_histo[len - 1] += 1;
        return true;
    }

    fn find_longest_symbol_code(&self, str_bytes: &[u8]) -> u16 {
        let target_symbol = Symbol::from_str_bytes(str_bytes);
        let src_symbol = self.get_hash_symbol(target_symbol.hash());
        if target_symbol.prefix_match(src_symbol) {
            return src_symbol.code();
        }

        if target_symbol.length() >= 2 {
            let code = self.short_codes[target_symbol.first2()] & CODE_MASK;
            if code >= CODE_BASE {
                return code;
            }
        }

        self.byte_codes[target_symbol.first()] & CODE_MASK
    }

    fn get_symbol(&self, code: u16) -> &Symbol {
        &self.symbols[code as usize]
    }

    fn encode_for(&self, target: &Symbol) -> (u8, usize, usize) {
        let src_symbol = self.get_hash_symbol(target.hash());
        if target.prefix_match(src_symbol) {
            return (src_symbol.code() as u8, src_symbol.length(), 1);
        }

        let code = self.short_codes[target.first2()];
        let s_len = (code >> LEN_BITS) as usize;
        let out_len = (1 + ((code & CODE_BASE) >> 8)) as usize;
        (code as u8, s_len, out_len)
    }

    fn len(&self) -> usize {
        self.symbol_num as usize
    }

    fn clear(&mut self) {
        for i in CODE_BASE..CODE_BASE + self.symbol_num {
            let s = self.get_symbol(i);
            match s.length() {
                1 => {
                    let v = s.first();
                    self.byte_codes[v] = (v as u16 & 0xff) | (1 << LEN_BITS)
                }
                2 => {
                    let v = s.first2();
                    self.short_codes[v] = (v as u16 & 0xff) | (1 << LEN_BITS)
                }
                _ => {
                    let src = self.get_hash_symbol_mut(s.hash());
                    src.reset();
                }
            }
        }
        self.len_histo.fill(0);
        self.symbol_num = 0;
    }

    fn finalize(&mut self) {
        // compute running sum of code lengths (starting offsets for each length)
        let mut rsum = [0u8; Symbol::MAX_LEN];
        for i in 0..rsum.len() - 1 {
            rsum[i + 1] = rsum[i] + self.len_histo[i];
        }

        let mut new_codes = [0u8; CODE_BASE as usize];
        for i in CODE_BASE..CODE_BASE + self.symbol_num {
            let mut s = self.symbols[i as usize];
            let len = s.length();
            new_codes[(i - CODE_BASE) as usize] = rsum[len - 1];
            rsum[len - 1] += 1;
            let new_code = new_codes[(i - CODE_BASE) as usize];
            s.set_code_len(new_code as u16, len);
            self.symbols[new_code as usize] = s;
        }

        for i in 0..CODE_BASE as usize {
            if (self.byte_codes[i] & CODE_MASK) >= CODE_BASE {
                let idx = (self.byte_codes[i] & 0xff) as usize;
                self.byte_codes[i] = new_codes[idx] as u16 | (1 << LEN_BITS);
            } else {
                self.byte_codes[i] = CODE_MASK | (1 << LEN_BITS);
            }
        }

        for i in 0..self.short_codes.len() {
            if (self.short_codes[i] & CODE_MASK) >= CODE_BASE {
                let idx = (self.short_codes[i] & 0xff) as usize;
                self.short_codes[i] = new_codes[idx] as u16 | (self.short_codes[i] & (0xf << LEN_BITS));
            } else {
                self.short_codes[i] = self.byte_codes[i & 0xff];
            }
        }

        for i in 0..self.hash_table.len() {
            if self.hash_table[i].taken() {
                let idx = (self.hash_table[i].code() & 0xff) as usize;
                self.hash_table[i] = self.symbols[new_codes[idx] as usize];
            }
        }
        self.finalized = true;
    }

    fn dump(&self) -> Vec<u8> {
        let mut total_size = 9usize;
        for i in 0..self.len_histo.len() {
            total_size += self.len_histo[i] as usize * (i + 1);
        }
        let mut buf = Vec::with_capacity(total_size);
        buf.push(Endian::get_native_endian().into());
        self.len_histo.iter().for_each(|l| buf.push(*l));
        for i in 0..self.symbol_num {
            let s = self.get_symbol(i);
            let mut num = s.as_u64();
            for _ in 0..s.length() {
                buf.push(num as u8);
                num >>= 8;
            }
        }
        buf
    }
}

impl Display for PerfectHashSymbolTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (start, end) = if self.finalized {
            (0usize, self.symbol_num as usize)
        } else {
            (CODE_BASE as usize, (CODE_BASE + self.symbol_num) as usize)
        };
        let symbols_str = &self.symbols[start..end].iter()
            .map(|&x| x.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        write!(f, "[{}]", symbols_str)
    }
}

pub struct SymbolTableBuilder {
    counter: Counter,
    count_frac: u32,
}

impl SymbolTableBuilder {
    pub fn build_from(s: &str) -> Box<dyn SymbolTable> {
        let str = String::from(s);
        let sample = vec![&str];
        SymbolTableBuilder {
            counter: Counter::new(),
            count_frac: 0,
        }.build(&sample)
    }

    pub fn build_from_samples(samples: &Vec<&String>) -> Box<dyn SymbolTable> {
        SymbolTableBuilder {
            counter: Counter::new(),
            count_frac: 5,
        }.build(samples)
    }

    fn build(&mut self, samples: &Vec<&String>) -> Box<dyn SymbolTable> {
        let mut symbol_table: Box<dyn SymbolTable> = Box::new(PerfectHashSymbolTable::new());
        let mut best_table = symbol_table.clone_box();
        let mut best_gain = i64::MIN;
        let mut best_single = [0u8; Counter::ENTRY_SIZE * 2];
        let mut sample_frac = 8;
        loop {
            let gain = self.compute_freq(samples, sample_frac, &symbol_table);
            if gain > best_gain {
                best_gain = gain;
                best_single = self.counter.backup_single();
                best_table = symbol_table.clone_box();
            }
            if sample_frac >= 128 {
                break;
            }
            self.make_table(sample_frac, &mut symbol_table);
            self.counter.reset();
            sample_frac += 30;
        }
        self.counter.restore_single(best_single);
        self.make_table(sample_frac, &mut best_table);
        best_table.finalize();
        best_table
    }

    fn compute_freq(&mut self, samples: &Vec<&String>, sample_frac: u32, symbol_table: &Box<dyn SymbolTable>) -> i64 {
        let mut gain = 0i64;
        for i in 0..samples.len() {
            if samples.len() > 128 && sample_frac < 128 {
                let rand = 1 + ((fsst_hash(1 + i) * sample_frac as usize) & 127);
                if rand > sample_frac as usize {
                    continue;
                }
            }
            gain += self.count_line(samples[i].as_bytes(), sample_frac, symbol_table);
        }
        gain
    }

    fn count_line(&mut self, str_bytes: &[u8], sample_frac: u32, symbol_table: &Box<dyn SymbolTable>) -> i64 {
        let mut gain = 0i64;
        let mut pos = 0;
        let mut code1 = symbol_table.find_longest_symbol_code(&str_bytes);
        let mut s1 = symbol_table.get_symbol(code1);
        loop {
            self.counter.inc_single(code1 as usize);
            if s1.length() > 1 {
                self.counter.inc_single(str_bytes[pos] as usize);
            }
            gain += s1.length() as i64 - (1 + is_escape_code(code1) as i64);
            pos += s1.length();
            if pos >= str_bytes.len() {
                break;
            }

            let code2 = symbol_table.find_longest_symbol_code(&str_bytes[pos..]);
            let s2 = symbol_table.get_symbol(code2);
            if sample_frac < 128 {
                self.counter.inc_concat(code1 as usize, code2 as usize);
                if s2.length() > 1 {
                    self.counter.inc_concat(code1 as usize, str_bytes[pos] as usize);
                }
            }
            code1 = code2;
            s1 = s2;
        }
        gain
    }

    fn make_table(&mut self, sample_frac: u32, symbol_table: &mut Box<dyn SymbolTable>) {
        let mut candidates: HashMap<Symbol, u32> = HashMap::with_capacity(CODE_MAX as usize);
        let end = CODE_BASE as usize + symbol_table.len();
        let mut pos1 = 0usize;
        while pos1 < end {
            let cnt1 = self.counter.get_single_and_forward(&mut pos1);
            if cnt1 == 0 {
                pos1 += 1;
                continue;
            }

            let s1 = symbol_table.get_symbol(pos1 as u16);
            let heuristic_cnt = match s1.length() {
                1 => 8 * cnt1,
                _ => cnt1
            };
            self.expand_candidate(&mut candidates, s1.clone(), heuristic_cnt, sample_frac);
            if s1.length() == Symbol::MAX_LEN
                || sample_frac >= 128 {
                pos1 += 1;
                continue;
            }

            let mut pos2 = 0usize;
            while pos2 < end {
                let cnt2 = self.counter.get_concat_and_forward(pos1, &mut pos2);
                if cnt2 > 0 {
                    let s2 = symbol_table.get_symbol(pos2 as u16);
                    let s3 = *s1 + *s2;
                    self.expand_candidate(&mut candidates, s3, cnt2, sample_frac);
                }
                pos2 += 1;
            }
            pos1 += 1;
        }

        let mut sorted_vec: Vec<(Symbol, u32)> = candidates.iter().map(|(k, v)| (*k, *v)).collect();
        sorted_vec.sort_by(|a, b| {
            if a.1 == b.1 {
                b.0.cmp(&a.0)
            } else {
                a.1.cmp(&b.1)
            }
        });
        symbol_table.clear();
        while symbol_table.len() < 255 && !sorted_vec.is_empty() {
            let s = sorted_vec.pop().unwrap();
            symbol_table.add(s.0);
        }
    }

    fn expand_candidate(&self, candidates: &mut HashMap<Symbol, u32>, s: Symbol, cnt: u32, sample_frac: u32) {
        if cnt >= (self.count_frac * sample_frac / 128) {
            let gain = s.length() as u32 * cnt;
            candidates.insert(s, candidates.get(&s).unwrap_or(&0) + gain);
        }
    }
}