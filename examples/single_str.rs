use fsst_rust::{decode_string, encode_string};
use fsst_rust::core::codec::Decoder;

fn main() {
    let str = "tumcwitumvldb";
    let (symbol_table, encoding) = encode_string(str, false);
    println!("built symbol table: {}", symbol_table.to_string()); // [b, t, w, tumc, witumvld]
    assert_eq!(str, decode_string(&symbol_table, &encoding));

    let table_bytes = symbol_table.dump();
    let (_, decoder) = Decoder::from_table_bytes(&table_bytes);
    assert_eq!(str, decoder.decode(&encoding));

    let compress_factor = str.len() as f64 / encoding.len() as f64;
    println!("compression factor: {:.4}", compress_factor);
}