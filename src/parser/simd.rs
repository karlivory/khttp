// Code is *heavily* adapted from:
// https://github.com/seanmonstar/httparse/blob/36147265105338185f49ceac51a9bea83941a1ec/src/simd/swar.rs
//
// Copyright (c) 2015-2025 Sean McArthur
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use super::request::is_valid_uri_byte;

#[inline]
pub fn match_path_vectored(buf: &[u8]) -> usize {
    swar_match_path_vectored(buf)
}

#[inline]
pub fn match_uri_vectored(buf: &[u8]) -> usize {
    swar_match_uri_vectored(buf)
}

#[inline]
fn swar_match_path_vectored(buf: &[u8]) -> usize {
    let len = buf.len();
    let mut i = 0;

    while i + BLOCK_SIZE <= len {
        let x = unsafe { core::ptr::read_unaligned(buf.as_ptr().add(i) as *const usize) };

        // byte-equality for '?' and ' ':
        // hit(c) = ((x ^ C) - 0x01) & ~(x ^ C) & 0x80 per byte
        const ONE: usize = uniform_block(0x01);
        const M128: usize = uniform_block(0x80);
        const QQ: usize = uniform_block(b'?');
        const SP: usize = uniform_block(b' ');

        let yq = x ^ QQ;
        let hq = yq.wrapping_sub(ONE) & !yq & M128;

        let ys = x ^ SP;
        let hs = ys.wrapping_sub(ONE) & !ys & M128;

        let hit = hq | hs;
        if hit != 0 {
            return i + offsetnz(hit);
        }
        i += BLOCK_SIZE;
    }

    // read tail
    while i < len {
        let b = unsafe { *buf.get_unchecked(i) };
        if b == b'?' || b == b' ' {
            break;
        }
        i += 1;
    }
    i
}

#[inline]
fn swar_match_uri_vectored(buf: &[u8]) -> usize {
    let len = buf.len();
    let mut i = 0;

    while i + BLOCK_SIZE <= len {
        let x = unsafe { core::ptr::read_unaligned(buf.as_ptr().add(i) as *const usize) };
        // 33 <= (x != 127) <= 255
        const M: u8 = 0x21;
        // uniform block full of exclamation mark (!) (33).
        const BM: usize = uniform_block(M);
        // uniform block full of 1.
        const ONE: usize = uniform_block(0x01);
        // uniform block full of DEL (127).
        const DEL: usize = uniform_block(0x7f);
        // uniform block full of 128.
        const M128: usize = uniform_block(128);

        let lt = x.wrapping_sub(BM) & !x;
        let y = x ^ DEL;
        let eq = y.wrapping_sub(ONE) & !y;

        let hit = (lt | eq) & M128;
        if hit != 0 {
            // find the first offending byte in this word and return
            return i + offsetnz(hit);
        }
        i += BLOCK_SIZE;
    }

    // read tail
    while i < len {
        if !is_valid_uri_byte(unsafe { *buf.get_unchecked(i) }) {
            break;
        }
        i += 1;
    }
    i
}

// Adapt block-size to match native register size, i.e: 32bit => 4, 64bit => 8
const BLOCK_SIZE: usize = core::mem::size_of::<usize>();

// creates a u64 whose bytes are each equal to b
const fn uniform_block(b: u8) -> usize {
    usize::from_ne_bytes([b; BLOCK_SIZE])
}

#[inline]
fn offsetnz(block: usize) -> usize {
    // fast path optimistic case (common for long valid sequences)
    if block == 0 {
        return BLOCK_SIZE;
    }

    // perf: rust will unroll this loop
    for (i, b) in block.to_ne_bytes().iter().copied().enumerate() {
        if b != 0 {
            return i;
        }
    }
    unreachable!()
}
