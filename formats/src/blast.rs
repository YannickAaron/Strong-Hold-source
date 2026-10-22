//! PKWARE DCL "explode" decompressor, port of zlib's contrib/blast.c (Mark Adler).
//! The engine calls it via FUN_180007270 -> FUN_180097ee0.

const MAXBITS: usize = 13;

struct Huffman {
    /// Indexed by the next MAXBITS input bits: (symbol, code length); length 0 = invalid code.
    lut: Vec<(u16, u8)>,
}

impl Huffman {
    /// `rep`: compact lengths, each byte = (repeat-1) << 4 | bit length.
    fn new(rep: &[u8]) -> Self {
        let mut lengths = Vec::new();
        for &b in rep {
            lengths.extend(std::iter::repeat_n(b & 15, (b >> 4) as usize + 1));
        }
        let mut count = [0u16; MAXBITS + 1];
        for &l in &lengths {
            count[l as usize] += 1;
        }
        let mut offs = [0u16; MAXBITS + 1];
        for len in 1..MAXBITS {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        // Canonical codes, first symbol of each length gets `first`. DCL sends each code
        // MSB-first and bit-inverted; the input is LSB-first, so reverse + invert into the index.
        let mut lut = vec![(0u16, 0u8); 1 << MAXBITS];
        let (mut first, mut index) = (0u32, 0usize);
        for len in 1..=MAXBITS {
            for k in 0..count[len] as usize {
                let code = first + k as u32;
                let mut idx = 0usize;
                for bit in 0..len {
                    idx |= ((((code >> (len - 1 - bit)) & 1) ^ 1) as usize) << bit;
                }
                for fill in (idx..1 << MAXBITS).step_by(1 << len) {
                    lut[fill] = (symbol[index + k], len as u8);
                }
            }
            index += count[len] as usize;
            first = (first + count[len] as u32) << 1;
        }
        Huffman { lut }
    }
}

struct Bits<'a> {
    input: &'a [u8],
    pos: usize,
    buf: u64,
    cnt: u32, // valid bits in buf
    pad: u32, // zero bits appended past the end of input
}

impl Bits<'_> {
    fn refill(&mut self) {
        while self.cnt <= 56 {
            let byte = match self.input.get(self.pos) {
                Some(&b) => b,
                None => { self.pad += 8; 0 }
            };
            self.buf |= (byte as u64) << self.cnt;
            self.pos += 1;
            self.cnt += 8;
        }
    }

    fn consume(&mut self, n: u32) -> Option<()> {
        self.buf >>= n;
        self.cnt -= n;
        // reading into the zero padding means the stream was truncated
        (self.cnt >= self.pad).then_some(())
    }

    fn bits(&mut self, need: u32) -> Option<u32> {
        self.refill();
        let v = (self.buf & ((1u64 << need) - 1)) as u32;
        self.consume(need)?;
        Some(v)
    }

    fn decode(&mut self, h: &Huffman) -> Option<u16> {
        self.refill();
        let (sym, len) = h.lut[(self.buf & ((1 << MAXBITS) - 1)) as usize];
        if len == 0 {
            return None;
        }
        self.consume(len as u32)?;
        Some(sym)
    }
}

const LITLEN: [u8; 98] = [
    11, 124, 8, 7, 28, 7, 188, 13, 76, 4, 10, 8, 12, 10, 12, 10, 8, 23, 8, 9, 7, 6, 7, 8, 7, 6, 55,
    8, 23, 24, 12, 11, 7, 9, 11, 12, 6, 7, 22, 5, 7, 24, 6, 11, 9, 6, 7, 22, 7, 11, 38, 7, 9, 8, 25,
    11, 8, 11, 9, 12, 8, 12, 5, 38, 5, 38, 5, 11, 7, 5, 6, 21, 6, 10, 53, 8, 7, 24, 10, 27, 44, 253,
    253, 253, 252, 252, 252, 13, 12, 45, 12, 45, 12, 61, 12, 45, 44, 173,
];
const LENLEN: [u8; 6] = [2, 35, 36, 53, 38, 23];
const DISTLEN: [u8; 7] = [2, 20, 53, 230, 247, 151, 248];
const BASE: [u16; 16] = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264];
const EXTRA: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];

#[derive(Debug, PartialEq)]
pub enum Error {
    Truncated,
    BadHeader,
    BadCode,
    BadDistance,
}

pub fn explode(input: &[u8], size_hint: usize) -> Result<Vec<u8>, Error> {
    // ponytail: tables rebuilt per call; cache in a OnceLock if profiling ever shows it.
    let (lit_t, len_t, dist_t) = (Huffman::new(&LITLEN), Huffman::new(&LENLEN), Huffman::new(&DISTLEN));
    let mut s = Bits { input, pos: 0, buf: 0, cnt: 0, pad: 0 };
    let lit = s.bits(8).ok_or(Error::Truncated)?;
    let dict = s.bits(8).ok_or(Error::Truncated)?;
    if lit > 1 || !(4..=6).contains(&dict) {
        return Err(Error::BadHeader);
    }
    let mut out = Vec::with_capacity(size_hint);
    loop {
        if s.bits(1).ok_or(Error::Truncated)? == 1 {
            let sym = s.decode(&len_t).ok_or(Error::BadCode)? as usize;
            let len = BASE[sym] as usize + s.bits(EXTRA[sym] as u32).ok_or(Error::Truncated)? as usize;
            if len == 519 {
                return Ok(out);
            }
            let shift = if len == 2 { 2 } else { dict };
            let dist = ((s.decode(&dist_t).ok_or(Error::BadCode)? as usize) << shift)
                + s.bits(shift).ok_or(Error::Truncated)? as usize
                + 1;
            if dist > out.len() {
                return Err(Error::BadDistance);
            }
            // Overlapping copy (dist < len) repeats the last `dist` bytes; copy in growing chunks.
            let start = out.len() - dist;
            let mut left = len;
            while left > 0 {
                let n = left.min(out.len() - start);
                out.extend_from_within(start..start + n);
                left -= n;
            }
        } else {
            let b = if lit == 1 { s.decode(&lit_t).ok_or(Error::BadCode)? as u8 } else { s.bits(8).ok_or(Error::Truncated)? as u8 };
            out.push(b);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn blast_c_reference_vector() {
        let input = [0x00, 0x04, 0x82, 0x24, 0x25, 0x8f, 0x80, 0x7f];
        assert_eq!(super::explode(&input, 0).unwrap(), b"AIAIAIAIAIAIA");
    }
}
