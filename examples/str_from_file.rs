use fsst_rust::{decode_all_strings, encode_all_strings, read_string_lines};

fn main() {
    let compress_file_path = "assets/test_data/ps_comment".to_string();
    let strings = read_string_lines(compress_file_path).unwrap();

    let mut start_time = std::time::Instant::now();
    let (symbol_table, encodings) = encode_all_strings(&strings);
    let compress_time = start_time.elapsed();

    start_time = std::time::Instant::now();
    let decode_strings = decode_all_strings(&symbol_table, &encodings);
    let decompress_time = start_time.elapsed();

    let mut encoding_size = symbol_table.dump().len();
    let mut total_size = 0;
    for i in 0..strings.len() {
        total_size += strings[i].len();
        encoding_size += encodings[i].len();
        assert_eq!(strings[i], decode_strings[i]);
    }

    let compress_factor = total_size as f64 / encoding_size as f64;
    println!("compression factor: {:.4}", compress_factor);
    println!("compression cost time: {}ms", compress_time.as_millis());
    println!("decompression cost time: {}ms", decompress_time.as_millis());
}