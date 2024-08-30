#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Criterion, criterion_group};

use fsst_rust::{encode_all_strings, read_string_lines};

fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("ps_comment_compress");
    let group_test_data_path = "assets/test_data/ps_comment";
    let data_setup = || {
        let mut strings = read_string_lines(group_test_data_path).unwrap();
        strings.truncate(1000);
        strings
    };

    group.bench_with_input("fsst", group_test_data_path, |b, _| {
        b.iter_batched(
            data_setup,
            |s| {
                encode_all_strings(&s);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_with_input("zstd", group_test_data_path, |b, _| {
        b.iter_batched(
            data_setup,
            |s| {
                for str in s {
                    zstd::encode_all(str.as_bytes(), 3).unwrap();
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_compress);
criterion_main!(benches);