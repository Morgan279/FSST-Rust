#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Criterion, criterion_group};

use fsst_rust::{decode_all_strings, encode_all_strings, read_string_lines};

fn bench_decompress(c: &mut Criterion) {
    let mut group = c.benchmark_group("l_comment_decompress");
    let group_test_data_path = "assets/test_data/l_comment";

    group.bench_with_input("fsst", group_test_data_path, |b, path| {
        b.iter_batched(
            || {
                let mut strings = read_string_lines(path).unwrap();
                strings.truncate(1000);
                encode_all_strings(&strings)
            },
            |s| {
                decode_all_strings(&s.0, &s.1)
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_with_input("zstd", group_test_data_path, |b, path| {
        b.iter_batched(
            || {
                let mut strings = read_string_lines(path).unwrap();
                strings.truncate(1000);
                let mut encodings = Vec::with_capacity(1000);
                for str in strings {
                    encodings.push(zstd::encode_all(str.as_bytes(), 3).unwrap());
                }
                encodings
            },
            |encodings| {
                for encode in encodings {
                    zstd::decode_all(encode.as_slice()).unwrap();
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_decompress);
criterion_main!(benches);