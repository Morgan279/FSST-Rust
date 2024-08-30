use std::cmp::{max, min};

mod symbol;
mod counter;
pub mod symbol_table;
pub mod codec;

const U64_SIZE: usize = size_of::<u64>();
const CODE_MAX: u16 = 1 << 9;
const CODE_MASK: u16 = CODE_MAX - 1;
const CODE_BASE: u16 = 256;
const CODE_ESCAPE: u8 = 255;
const LEN_BITS: u16 = 12;
const HASH_SHIFT: usize = 15;
const HASH_PRIME: usize = 2971215073;
const SAMPLE_TARGET: usize = 1 << 16;
const SMALL_STR_THRESHOLD: usize = 1 << 14;

type U64Bytes = [u8; U64_SIZE];

pub fn is_escape_code(code: u16) -> bool {
    code < CODE_BASE
}

fn fsst_hash(v: usize) -> usize {
    let prime = v * HASH_PRIME;
    prime ^ (prime >> HASH_SHIFT)
}

fn bulk_load(s: &[u8]) -> u64 {
    let mut v = [0u8; U64_SIZE];
    v[..s.len()].copy_from_slice(s);
    unsafe {
        std::mem::transmute::<U64Bytes, u64>(v)
    }
}

fn bulk_load_u32(s: &[u8]) -> u32 {
    let mut v = [0u8; 4];
    v[..s.len()].copy_from_slice(s);
    unsafe {
        std::mem::transmute::<[u8; 4], u32>(v)
    }
}

pub fn take_sample(sample_space: &Vec<String>) -> Vec<&String> {
    let total_size = sample_space.iter().map(|s| s.len()).sum::<usize>();
    let (mut sample_size, mut sample_prob, mut sample_target) = (0usize, 256usize, SAMPLE_TARGET);
    if total_size > sample_target {
        sample_prob = max(4, 256 * sample_target / total_size);
    } else {
        sample_target = total_size;
    }
    let mut sample = Vec::with_capacity(sample_space.len() * (sample_target / total_size));

    let sample_rand = 1;
    while sample_size < sample_target {
        for str in sample_space {
            if (sample_rand & 255) < sample_prob {
                sample.push(str);
                sample_size += str.len();
                if sample_size >= sample_target {
                    break;
                }
            }
        }
        sample_prob <<= 2;
    }

    sample
}

pub fn take_sample_from_bytes(sample_space: &[u8]) -> Vec<u8> {
    if sample_space.len() < SMALL_STR_THRESHOLD {
        return Vec::from(sample_space);
    }

    let sample_size = min(sample_space.len() >> 3, SAMPLE_TARGET);
    let sample_seg_size = sample_size / 10;
    let mut sample = vec![0; sample_size];
    let gap = (sample_space.len() - sample.len()) / 10;
    let (mut pos_in, mut pos_out) = (0, 0);
    loop {
        if pos_in + sample_seg_size >= sample_space.len()
            || pos_out + sample_seg_size >= sample_size {
            break;
        }
        sample[pos_out..pos_out + sample_seg_size]
            .copy_from_slice(&sample_space[pos_in..pos_in + sample_seg_size]);
        pos_out += sample_seg_size;
        pos_in += sample_seg_size + gap;
    }
    sample.truncate(pos_out);
    sample
}
