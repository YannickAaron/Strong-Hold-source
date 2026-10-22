//! PKWARE DCL "implode" compressor, binary mode, as the engine runs it (FUN_1801a4400 / FUN_1801a40a0, the
//! pklib code; same algorithm as StormLib's implode.c). Byte-exact output matters (saves must match the
//! original), so this is a literal transcription over the engine's own 0x8de8-byte work area: every pointer of
//! the original is an offset into `b`, including its reads past the end of the input window.

/// Work-area layout (TCmpStruct).
const DISTANCE: usize = 0x0;
const OUT_BYTES: usize = 0x4;
const OUT_BITS: usize = 0x8;
const DSIZE_BITS: usize = 0xc;
const DSIZE_MASK: usize = 0x10;
const DSIZE: usize = 0x18;
const DIST_BITS: usize = 0x1c;
const DIST_CODES: usize = 0x5c;
const CH_BITS: usize = 0x9c;
const CH_CODES: usize = 0x3a2;
const KMP: usize = 0x9c8; // u16[0x204]
const HASH_TO_INDEX: usize = 0xdd4; // u16[0x900]
const OUT_BUFF: usize = 0x1fd6; // [0x802]
const WORK: usize = 0x27d8; // [0x2204]
const HASH_OFFS: usize = 0x49dc; // u16[0x2204]
const SIZE: usize = 0x8de8;

// pklib tables (DLL .rdata 0x1802d83a0 / d0 / e0, 0x1802d8320, 0x1802d8360)
const EX_LEN_BITS: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
const LEN_BITS: [u8; 16] = [3, 2, 3, 3, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 7, 7];
const LEN_CODE: [u8; 16] = [5, 3, 1, 6, 10, 2, 12, 20, 4, 24, 8, 48, 16, 32, 64, 0];
const DIST_BITS_TAB: [u8; 64] = [
    2, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];
const DIST_CODE_TAB: [u8; 64] = [
    3, 13, 5, 25, 9, 17, 1, 62, 30, 46, 14, 54, 22, 38, 6, 58, 26, 42, 10, 50, 18, 34, 66, 2, 124, 60, 92, 28, 108, 44, 76,
    12, 116, 52, 84, 20, 100, 36, 68, 4, 120, 56, 88, 24, 104, 40, 72, 8, 240, 112, 176, 48, 208, 80, 144, 16, 224, 96, 160,
    32, 192, 64, 128, 0,
];

struct W<'a, R, F> {
    b: Vec<u8>,
    read: R,
    write: &'a mut F,
}

impl<R: FnMut(&mut [u8]) -> usize, F: FnMut(&[u8])> W<'_, R, F> {
    fn r16(&self, o: usize) -> usize {
        u16::from_le_bytes([self.b[o], self.b[o + 1]]) as usize
    }
    fn w16(&mut self, o: usize, v: usize) {
        self.b[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes());
    }
    fn r32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.b[o..o + 4].try_into().unwrap())
    }
    fn w32(&mut self, o: usize, v: u32) {
        self.b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn hash(&self, p: usize) -> usize {
        HASH_TO_INDEX + 2 * (self.b[p] as usize * 4 + self.b[p + 1] as usize * 5)
    }
    fn offs(&self, i: usize) -> usize {
        WORK + self.r16(HASH_OFFS + 2 * i)
    }

    /// FUN_1801a3ee0 (OutputBits)
    fn out(&mut self, mut nbits: u32, mut bits: u32) {
        if nbits > 8 {
            self.out(8, bits);
            bits >>= 8;
            nbits -= 8;
        }
        let ob = self.r32(OUT_BITS);
        let at = OUT_BUFF + self.r32(OUT_BYTES) as usize;
        self.b[at] |= ((bits & 0xff) << ob) as u8;
        self.w32(OUT_BITS, ob + nbits);
        if ob + nbits < 9 {
            let r = (ob + nbits) & 7;
            self.w32(OUT_BITS, r);
            if r == 0 {
                self.w32(OUT_BYTES, self.r32(OUT_BYTES) + 1);
            }
        } else {
            let n = self.r32(OUT_BYTES) + 1;
            self.w32(OUT_BYTES, n);
            self.b[OUT_BUFF + n as usize] = (bits >> (8 - ob)) as u8;
            self.w32(OUT_BITS, (ob + nbits) & 7);
        }
        if self.r32(OUT_BYTES) > 0x7ff {
            self.flush();
        }
    }

    /// FUN_1801a3e40 (FlushBuf)
    fn flush(&mut self) {
        (self.write)(&self.b[OUT_BUFF..OUT_BUFF + 0x800]);
        let ob = self.r32(OUT_BYTES) as usize;
        let (c1, c2) = (self.b[OUT_BUFF + 0x800], self.b[OUT_BUFF + ob]);
        let ob = ob - 0x800;
        self.w32(OUT_BYTES, ob as u32);
        self.b[OUT_BUFF..OUT_BUFF + 0x802].fill(0);
        if ob != 0 {
            self.b[OUT_BUFF] = c1;
        }
        if self.r32(OUT_BITS) != 0 {
            self.b[OUT_BUFF + ob] = c2;
        }
    }

    /// FUN_1801a3f80 (SortBuffer): bucket every position in [from, to) by its byte-pair hash.
    fn sort(&mut self, from: usize, to: usize) {
        self.b[HASH_TO_INDEX..HASH_TO_INDEX + 0x1200].fill(0);
        for p in from..to {
            let h = self.hash(p);
            self.w16(h, self.r16(h) + 1);
        }
        let mut sum = 0usize;
        for h in (HASH_TO_INDEX..HASH_TO_INDEX + 0x1200).step_by(2) {
            sum = (sum + self.r16(h)) & 0xffff;
            self.w16(h, sum);
        }
        for p in (from..to).rev() {
            let h = self.hash(p);
            let i = (self.r16(h) + 0xffff) & 0xffff;
            self.w16(h, i);
            self.w16(HASH_OFFS + 2 * i, p - WORK);
        }
    }

    /// KMP failure table step shared by both halves of FindRep.
    fn kmp_extend(&mut self, p: usize, offs: &mut usize, di: &mut usize, to: usize) {
        while *offs < to {
            let ok = self.b[p + *offs] == self.b[p + *di] || {
                *di = self.r16(KMP + 2 * *di);
                *di == 0xffff
            };
            if ok {
                *di = (*di + 1) & 0xffff;
                *offs += 1;
                self.w16(KMP + 2 * *offs, *di);
            }
        }
    }

    /// FUN_1801a3ab0 (FindRep): longest earlier match for position p (0 = none); sets DISTANCE.
    fn find_rep(&mut self, p: usize) -> u32 {
        let h = self.hash(p);
        let min = (p.wrapping_sub(WORK).wrapping_sub(self.r32(DSIZE) as usize) + 1) & 0xffff;
        let mut idx = self.r16(h);
        if self.r16(HASH_OFFS + 2 * idx) < min {
            while self.r16(HASH_OFFS + 2 * idx) < min {
                idx = (idx + 1) & 0xffff;
            }
            self.w16(h, idx);
        }
        let lim = p - 1;
        let mut prev = self.offs(idx);
        if lim <= prev {
            return 0;
        }
        let mut len = 1usize;
        let b = |w: &Self, i: usize| w.b[i];
        loop {
            if b(self, p) == b(self, prev) && b(self, p + len - 1) == b(self, prev + len - 1) {
                // count equal bytes from index 2 (the hash implies index 1), unrolled by two as the engine does
                let (mut cnt, mut q, mut ip) = (2usize, prev + 1, p + 3);
                let mut e;
                loop {
                    e = q + 1;
                    if b(self, e) != b(self, ip - 1) {
                        break;
                    }
                    e = q + 2;
                    if b(self, e) != b(self, ip) {
                        cnt += 1;
                        break;
                    }
                    cnt += 2;
                    ip += 2;
                    q = e;
                    if cnt >= 0x204 {
                        break;
                    }
                }
                if len <= cnt {
                    let dist = (cnt as u32 - 1).wrapping_add((p as u32).wrapping_sub(e as u32));
                    self.w32(DISTANCE, dist);
                    len = cnt;
                    if cnt > 10 {
                        if cnt == 0x204 {
                            self.w32(DISTANCE, dist.wrapping_sub(1));
                            return 0x204;
                        }
                        return self.find_rep_long(p, idx, len);
                    }
                }
            }
            idx = (idx + 1) & 0xffff;
            prev = self.offs(idx);
            if lim <= prev {
                return if len > 1 { len as u32 } else { 0 };
            }
        }
    }

    /// FindRep's second half: KMP search through the rest of the hash bucket for a match longer than `len`.
    fn find_rep_long(&mut self, p: usize, mut idx: usize, mut len: usize) -> u32 {
        if p - 1 <= self.offs(idx + 1) {
            return len as u32;
        }
        let (mut offs, mut di) = (1usize, 0usize);
        self.w32(KMP, 0xffff);
        self.kmp_extend(p, &mut offs, &mut di, len);
        let mut rend = self.offs(idx) + len;
        let mut r = len;
        loop {
            let t = self.r16(KMP + 2 * r);
            let k = if t != 0xffff { t } else { 0 };
            let mut prev;
            loop {
                idx = (idx + 1) & 0xffff;
                prev = self.offs(idx);
                if p - 1 <= prev {
                    return len as u32;
                }
                if prev + k >= rend {
                    break;
                }
            }
            let u2 = len - 2;
            let mut e;
            if self.b[p + u2] == self.b[prev + u2] {
                if prev + k == rend {
                    e = rend;
                    r = k;
                } else {
                    e = prev;
                    r = 0;
                }
            } else {
                loop {
                    idx = (idx + 1) & 0xffff;
                    prev = self.offs(idx);
                    if p - 1 <= prev {
                        return len as u32;
                    }
                    if self.b[prev + u2] == self.b[p + u2] && self.b[prev] == self.b[p] {
                        break;
                    }
                }
                e = prev + 2;
                r = 2;
            }
            if self.b[e] == self.b[p + r] {
                loop {
                    r += 1;
                    if r > 0x203 {
                        break;
                    }
                    e += 1;
                    if self.b[e] != self.b[p + r] {
                        break;
                    }
                }
            }
            rend = e;
            if len <= r {
                self.w32(DISTANCE, (p as u32).wrapping_sub(prev as u32).wrapping_sub(1));
                if r == 0x204 {
                    return 0x204;
                }
                self.kmp_extend(p, &mut offs, &mut di, r);
                len = r;
            }
        }
    }

    fn put_char(&mut self, c: usize) {
        self.out(self.b[CH_BITS + c] as u32, self.r16(CH_CODES + 2 * c) as u32);
    }

    /// FUN_1801a40a0 (WriteCmpData)
    fn run(&mut self) {
        let dsize = self.r32(DSIZE) as usize;
        let dbits = self.r32(DSIZE_BITS);
        let mut inp = WORK + 0x204 + dsize;
        self.b[OUT_BUFF] = 0; // ctype: binary
        self.b[OUT_BUFF + 1] = dbits as u8;
        self.w32(OUT_BYTES, 2);
        self.b[OUT_BUFF + 2..OUT_BUFF + 0x802].fill(0);
        self.w32(OUT_BITS, 0);
        let mut phase = 0;
        loop {
            let (mut got, mut ended) = (0usize, false);
            while got < 0x1000 {
                let at = WORK + 0x204 + dsize + got;
                let n = (self.read)(&mut self.b[at..at + 0x1000 - got]);
                if n == 0 {
                    ended = true;
                    break;
                }
                got += n;
            }
            if ended && got == 0 && phase == 0 {
                break;
            }
            let end = WORK + dsize + got + if ended { 0x204 } else { 0 };
            match phase {
                0 => {
                    self.sort(inp, end + 1);
                    phase = if dsize != 0x1000 { 2 } else { 1 };
                }
                1 => {
                    self.sort(inp - dsize + 0x204, end + 1);
                    phase = 2;
                }
                _ => self.sort(inp - dsize, end + 1),
            }
            while inp < end {
                let mut rep = self.find_rep(inp) as usize;
                let mut flush = false;
                while rep != 0 {
                    let dist = self.r32(DISTANCE);
                    if rep == 2 && dist > 0xff {
                        break;
                    }
                    if ended && end < inp + rep {
                        rep = end - inp;
                        if rep > 1 && (rep != 2 || dist < 0x100) {
                            flush = true;
                        }
                        break;
                    }
                    if rep > 7 || end <= inp + 1 {
                        flush = true;
                        break;
                    }
                    let rep2 = self.find_rep(inp + 1) as usize;
                    if rep2 <= rep || (rep2 <= rep + 1 && dist < 0x81) {
                        self.w32(DISTANCE, dist);
                        flush = true;
                        break;
                    }
                    self.put_char(self.b[inp] as usize);
                    inp += 1;
                    rep = rep2;
                }
                if flush {
                    self.put_char(rep + 0xfe);
                    let dist = self.r32(DISTANCE);
                    let (sh, n, low) = if rep == 2 { (2, 2, dist & 3) } else { (dbits, dbits, self.r32(DSIZE_MASK) & dist) };
                    let i = (dist >> sh) as usize;
                    self.out(self.b[DIST_BITS + i] as u32, self.b[DIST_CODES + i] as u32);
                    self.out(n, low);
                    inp += rep;
                } else {
                    self.put_char(self.b[inp] as usize);
                    inp += 1;
                }
            }
            if ended {
                break;
            }
            inp -= 0x1000;
            self.b.copy_within(WORK + 0x1000..WORK + 0x1000 + dsize + 0x204, WORK);
        }
        self.put_char(0x305); // end of stream
        if self.r32(OUT_BITS) != 0 {
            self.w32(OUT_BYTES, self.r32(OUT_BYTES) + 1);
        }
        let n = self.r32(OUT_BYTES) as usize;
        (self.write)(&self.b[OUT_BUFF..OUT_BUFF + n]);
    }
}

/// Compress `input` (binary mode, dictionary 1024 / 2048 / 4096). `write` gets the stream in the engine's
/// chunks (0x800-byte flushes, then the rest) and returns false to stop: the engine's sink then also makes its
/// source report end of input (FUN_18000d0b0 / FUN_1800086b0), which ends the stream early.
pub fn implode_chunks(input: &[u8], dsize: u32, write: &mut impl FnMut(&[u8]) -> bool) {
    let (bits, mask) = match dsize {
        0x400 => (4, 0xf),
        0x800 => (5, 0x1f),
        0x1000 => (6, 0x3f),
        _ => panic!("implode: bad dictionary size {dsize}"),
    };
    let stop = std::cell::Cell::new(false);
    let mut pos = 0;
    let read = |buf: &mut [u8]| {
        if stop.get() {
            return 0;
        }
        let n = buf.len().min(input.len() - pos);
        buf[..n].copy_from_slice(&input[pos..pos + n]);
        pos += n;
        n
    };
    let mut sink = |c: &[u8]| {
        if !stop.get() && !write(c) {
            stop.set(true);
        }
    };
    let mut w = W { b: vec![0; SIZE], read, write: &mut sink };
    w.w32(DSIZE_BITS, bits);
    w.w32(DSIZE_MASK, mask);
    w.w32(DSIZE, dsize);
    for i in 0..0x100 {
        w.b[CH_BITS + i] = 9;
        w.w16(CH_CODES + 2 * i, i * 2);
    }
    let mut n = 0x100;
    for i in 0..16 {
        for k in 0..1usize << EX_LEN_BITS[i] {
            w.b[CH_BITS + n] = EX_LEN_BITS[i] + LEN_BITS[i] + 1;
            w.w16(CH_CODES + 2 * n, (k << (LEN_BITS[i] + 1)) | (LEN_CODE[i] as usize * 2) | 1);
            n += 1;
        }
    }
    w.b[DIST_BITS..DIST_BITS + 64].copy_from_slice(&DIST_BITS_TAB);
    w.b[DIST_CODES..DIST_CODES + 64].copy_from_slice(&DIST_CODE_TAB);
    w.run();
}

/// Compress `input` into one stream.
pub fn implode(input: &[u8], dsize: u32) -> Vec<u8> {
    let mut out = vec![];
    implode_chunks(input, dsize, &mut |c| {
        out.extend_from_slice(c);
        true
    });
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trips_through_explode() {
        let mut s = 1u32;
        let mut data: Vec<u8> = (0..70_000u32)
            .map(|i| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                if i % 7 < 3 { (s >> 24) as u8 & 3 } else { (i / 13) as u8 }
            })
            .collect();
        data.extend(std::iter::repeat_n(0u8, 5000));
        for d in [0x400, 0x800, 0x1000] {
            for n in [0, 1, 2, 3, 600, 4096, 4097, 9000, data.len()] {
                let c = super::implode(&data[..n], d);
                assert_eq!(crate::blast::explode(&c, n).unwrap(), &data[..n], "dsize {d} n {n}");
            }
        }
    }
}
