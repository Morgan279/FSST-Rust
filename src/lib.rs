use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;

use crate::core::codec::{Decoder, Encoder};
use crate::core::symbol_table::{SymbolTable, SymbolTableBuilder};
use crate::core::take_sample;

pub mod core;
mod util;

/// build symbol table by sampling the given strings
/// symbol table can be used to build `Encoder` and `Decoder`
/// # Example
///
/// ```
/// use fsst_rust::build_table_by_sampling;
/// use fsst_rust::core::codec::{Decoder, Encoder};
/// let strings = vec!["abcd".to_string(), "efgh".to_string()];
/// let symbol_table = build_table_by_sampling(&strings);
/// let encoder = Encoder::from_table(&symbol_table);
/// let str = "abc";
/// let encoding = encoder.encode(&str, false);
/// let decoder = Decoder::from_table(&symbol_table);
/// let decode_str = decoder.decode(&encoding);
/// assert_eq!(str, decode_str);
/// ```
pub fn build_table_by_sampling(strings: &Vec<String>) -> Box<dyn SymbolTable> {
    let sample = take_sample(&strings);
    SymbolTableBuilder::build_from_samples(&sample)
}

/// encode all given strings
/// it will sample the given strings and build a symbol table which will be returned in a tuple
pub fn encode_all_strings(strings: &Vec<String>) -> (Box<dyn SymbolTable>, Vec<Vec<u8>>) {
    let symbol_table = build_table_by_sampling(strings);
    let encoder = Encoder::from_table(&symbol_table);
    let mut encodings = Vec::with_capacity(strings.len());
    for str in strings {
        encodings.push(encoder.encode_str(str));
    }
    (symbol_table, encodings)
}

/// encode a single string
/// if including_table is true, it will encode the symbol table to bytes
/// and add it the encoding bytes header, i.e., | symbol table bytes | string encoding bytes |
/// # Example
///
/// ```
/// use fsst_rust::core::codec::Decoder;
/// use fsst_rust::encode_string;
/// let str = "hello world".to_string();
/// let (_, encoding) = encode_string(&str, true);
/// let (table_end_pos, decoder) = Decoder::from_table_bytes(&encoding);
/// let decode_str = decoder.decode(&encoding[table_end_pos..].to_vec());
/// assert_eq!(str, decode_str);
/// ```
pub fn encode_string(str: &str, including_table: bool) -> (Box<dyn SymbolTable>, Vec<u8>) {
    let symbol_table = SymbolTableBuilder::build_from(str);
    let encoder = Encoder::from_table(&symbol_table);
    let encoding = encoder.encode(str, including_table);
    (symbol_table, encoding)
}

/// decode bytes to string according to the give symbol table
pub fn decode_string(table: &Box<dyn SymbolTable>, encoding: &Vec<u8>) -> String {
    Decoder::from_table(table).decode(encoding)
}

/// decode all string encodings by the given symbol table
pub fn decode_all_strings(table: &Box<dyn SymbolTable>, encodings: &Vec<Vec<u8>>) -> Vec<String> {
    let mut strings = Vec::with_capacity(encodings.len());
    let decoder = Decoder::from_table(table);
    for encoding in encodings {
        strings.push(decoder.decode(encoding))
    }
    strings
}

pub fn encode_all_strings_from_file<P: AsRef<Path>>(filename: P) -> io::Result<(Box<dyn SymbolTable>, Vec<Vec<u8>>)> {
    let strings = read_string_lines(filename)?;
    Ok(encode_all_strings(&strings))
}

pub fn read_string_lines<P>(filename: P) -> io::Result<Vec<String>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    let strings: Vec<String> = io::BufReader::new(file)
        .lines()
        .map(|l| l.expect("read string failed"))
        .collect();
    Ok(strings)
}

#[cfg(test)]
mod test {
    use crate::{decode_all_strings, encode_all_strings, read_string_lines};

    #[test]
    pub fn test_codec() {
        let group_test_data_path = "assets/test_data/c_name";
        let mut strings = read_string_lines(group_test_data_path).unwrap();
        strings.truncate(1000);
        let (table, encodings) = encode_all_strings(&strings);
        let decode_strings = decode_all_strings(&table, &encodings);
        for i in 0..strings.len() {
            assert_eq!(strings[i], decode_strings[i]);
        }
    }
}

