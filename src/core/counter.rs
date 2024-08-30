use std::cmp::min;

use crate::core::{bulk_load, CODE_MAX, U64_SIZE};

pub(crate) struct Counter {
    single_low: [u8; Counter::ENTRY_SIZE],
    single_high: [u8; Counter::ENTRY_SIZE],
    concat_low: [[u8; Counter::ENTRY_SIZE]; Counter::ENTRY_SIZE],
    concat_high: [[u8; Counter::ENTRY_SIZE >> 1]; Counter::ENTRY_SIZE],
}

impl Counter {
    pub(crate) const ENTRY_SIZE: usize = CODE_MAX as usize;

    pub fn new() -> Counter {
        Counter {
            single_low: [0u8; Counter::ENTRY_SIZE],
            single_high: [0u8; Counter::ENTRY_SIZE],
            concat_low: [[0u8; Counter::ENTRY_SIZE]; Counter::ENTRY_SIZE],
            concat_high: [[0u8; Counter::ENTRY_SIZE >> 1]; Counter::ENTRY_SIZE],
        }
    }

    pub fn inc_single(&mut self, pos: usize) {
        if self.single_low[pos] == 0 {
            // increment high early (when low==0, not when low==255). This means (high > 0) <=> (cnt > 0)
            self.single_high[pos] = self.single_high[pos]
                .checked_add(1)
                .unwrap_or(self.single_high[pos]);
        }
        self.single_low[pos] = self.single_low[pos].wrapping_add(1);
    }

    pub fn inc_concat(&mut self, pos1: usize, pos2: usize) {
        if self.concat_low[pos1][pos2] == 0 {
            // increment high early (when low==0, not when low==255). This means (high > 0) <=> (cnt > 0)
            // inc 4-bits high counter with 1<<0 (1) or 1<<4 (16) -- depending on whether pos2 is even or odd, respectively
            // we take our chances with overflow (4K max val, on a 8K sample)
            let pos_high = pos2 >> 1;
            self.concat_high[pos1][pos_high] = self.concat_high[pos1][pos_high]
                .checked_add(1 << ((pos2 & 1) << 2))
                .unwrap_or(self.concat_high[pos1][pos_high]);
        }
        self.concat_low[pos1][pos2] = self.concat_low[pos1][pos2].wrapping_add(1);
    }

    /// read 16-bits single symbol counter, split into two 8-bits numbers (count1Low, count1High), while skipping over zeros.
    /// it will advance pos1 to the next nonzero counter in register range
    pub fn get_single_and_forward(&mut self, pos: &mut usize) -> u32 {
        let mut high = bulk_load(&self.single_high[*pos..min(*pos + U64_SIZE, Self::ENTRY_SIZE)]);
        let zero = if high > 0 {
            high.trailing_zeros() >> 3
        } else {
            7u32
        };
        high = (high >> (zero << 3)) & 0xff; // advance to nonzero counter
        *pos += zero as usize;
        if (*pos >= Self::ENTRY_SIZE) || high == 0 {
            return 0; // all zero
        }

        let low = self.single_low[*pos] as u64;
        if low > 0 {
            // high is incremented early and low late, so decrement high (unless low==0)
            high -= 1;
        }
        ((high << 8) | low) as u32
    }

    /// read 12-bits pairwise symbol counter, split into low 8-bits and high 4-bits number while skipping over zeros
    /// it will advance pos2 to the next nonzero counter in register range
    pub fn get_concat_and_forward(&mut self, pos1: usize, pos2: &mut usize) -> u32 {
        let start = *pos2 >> 1;
        let end = min(start + U64_SIZE, Self::ENTRY_SIZE >> 1);
        let mut high = bulk_load(&self.concat_high[pos1][start..end]);
        high >>= (*pos2 & 1) << 2; // odd pos2: ignore the lowest 4 bits & we see only 15 counters
        let zero = if high > 0 { // number of zero 4-bits counters
            high.trailing_zeros() >> 2
        } else {
            (15 - (*pos2 & 1)) as u32
        };

        high = (high >> (zero << 2)) & 0xf; // advance to nonzero counter
        *pos2 += zero as usize;
        if (*pos2 >= Self::ENTRY_SIZE) || high == 0 {
            return 0; // all zero
        }

        let low = self.concat_low[pos1][*pos2] as u64;
        if low > 0 {
            // high is incremented early and low late, so decrement high (unless low==0)
            high -= 1;
        }
        ((high << 8) | low) as u32
    }

    pub fn backup_single(&self) -> [u8; Self::ENTRY_SIZE * 2] {
        let mut buf = [0u8; Self::ENTRY_SIZE * 2];
        unsafe {
            std::ptr::copy_nonoverlapping(self.single_low.as_ptr(), buf.as_mut_ptr(), Self::ENTRY_SIZE);
            std::ptr::copy_nonoverlapping(self.single_high.as_ptr(), buf.as_mut_ptr().add(Self::ENTRY_SIZE), Self::ENTRY_SIZE);
        }
        buf
    }

    pub fn restore_single(&mut self, buf: [u8; Self::ENTRY_SIZE * 2]) {
        unsafe {
            std::ptr::copy_nonoverlapping(buf.as_ptr(), self.single_low.as_mut_ptr(), Self::ENTRY_SIZE);
            std::ptr::copy_nonoverlapping(buf.as_ptr().add(Self::ENTRY_SIZE), self.single_high.as_mut_ptr(), Self::ENTRY_SIZE);
        }
    }

    pub fn reset(&mut self) {
        let single_low_prt = self.single_low.as_mut_ptr();
        let single_high_prt = self.single_high.as_mut_ptr();
        let concat_low_prt = self.concat_low.as_mut_ptr();
        let concat_high_prt = self.concat_high.as_mut_ptr();
        unsafe {
            single_low_prt.write_bytes(0, self.single_low.len());
            single_high_prt.write_bytes(0, self.single_high.len());
            concat_low_prt.write_bytes(0, self.concat_low.len());
            concat_high_prt.write_bytes(0, self.concat_high.len());
        }
    }
}

#[cfg(test)]
mod test {
    use crate::core::counter::Counter;

    #[test]
    pub fn test_counter() {
        let mut counter = Counter::new();
        counter.inc_single(0);
        assert_eq!(1, counter.single_low[0]);
        counter.inc_single(5);
        let mut pos = 0usize;
        let v1 = counter.get_single_and_forward(&mut pos);
        assert_eq!(1, v1);
        assert_eq!(0, pos);
        pos += 1;
        let v2 = counter.get_single_and_forward(&mut pos);
        assert_eq!(5, pos);
        assert_eq!(1, v2);

        counter.reset();
        for _ in 0..255 {
            counter.inc_single(0);
            counter.inc_concat(0, 0);
        }
        assert_eq!(255, counter.single_low[0]);
        counter.inc_single(0);
        pos = 0;
        assert_eq!(256, counter.get_single_and_forward(&mut pos));
        assert_eq!(0, counter.single_low[0]);

        counter.inc_concat(0, 0);
        assert_eq!(256, counter.get_concat_and_forward(0, &mut pos));
    }
}
