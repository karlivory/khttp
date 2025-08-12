// Code is *heavily* adapted from:
// https://github.com/errantmind/faf/blob/2b1456d3fb492811c173c1c467f656df543ffdc6/src/http_date.rs
//
// MIT License
//
// Copyright (c) 2018 James Bates
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#[inline]
pub fn get_date_now() -> [u8; DATE_LEN] {
    DATE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let now = now_unix_sec();
        if cache.last_sec != now {
            format_http_date(&mut cache.buf, now);
            cache.last_sec = now;
        }
        cache.buf
    })
}

#[inline]
pub fn get_date_from_secs(seconds: i64) -> [u8; DATE_LEN] {
    let mut buf = BASE;
    format_http_date(&mut buf, seconds);
    buf
}

#[inline]
pub fn get_date_now_uncached() -> [u8; DATE_LEN] {
    let mut buf = BASE;
    format_http_date(&mut buf, now_unix_sec());
    buf
}

const BASE: [u8; 35] = *b"date: Mon, 00 Jan 0000 00:00:00 GMT";
const DATE_LEN: usize = BASE.len();

struct DateCache {
    buf: [u8; DATE_LEN],
    last_sec: i64,
}

thread_local! {
    static DATE_CACHE: core::cell::RefCell<DateCache> = const {
        core::cell::RefCell::new(DateCache {
            buf: BASE,
            last_sec: i64::MIN, // force first update
        })
    };
}

#[inline]
fn divmod_i64(n: i64, d: i64) -> (i64, i64) {
    let q = n.div_euclid(d);
    let r = n.rem_euclid(d);
    (q, r)
}

#[inline]
fn now_unix_sec() -> i64 {
    // Prefer coarse clock (Linux) -> cheaper, 1s granularity is enough
    #[cfg(target_os = "linux")]
    const CLOCK_REALTIME_FAST: i32 = 5; // CLOCK_REALTIME_COARSE
    #[cfg(not(target_os = "linux"))]
    const CLOCK_REALTIME_FAST: i32 = 0; // CLOCK_REALTIME

    const CLOCK_REALTIME: i32 = 0;

    let mut ts = core::mem::MaybeUninit::<Timespec>::uninit();
    let rc = unsafe { clock_gettime(CLOCK_REALTIME_FAST, ts.as_mut_ptr()) };
    if rc == 0 {
        unsafe { ts.assume_init() }.tv_sec
    } else {
        let mut ts2 = core::mem::MaybeUninit::<Timespec>::uninit();
        unsafe { clock_gettime(CLOCK_REALTIME, ts2.as_mut_ptr()) };
        unsafe { ts2.assume_init() }.tv_sec
    }
}

#[repr(C, align(32))]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn clock_gettime(clk_id: i32, tp: *mut Timespec) -> i32;
}

#[inline]
fn format_http_date(buf: &mut [u8; DATE_LEN], secs_since_epoch: i64) {
    *buf = BASE;

    const SECS_PER_MIN: i64 = 60;
    const SECS_PER_HOUR: i64 = 3600;
    const SECS_PER_DAY: i64 = 86400;

    const LEAPOCH: i64 = 11017;
    const DAYS_PER_400Y: i64 = 365 * 400 + 97;
    const DAYS_PER_100Y: i64 = 365 * 100 + 24;
    const DAYS_PER_4Y: i64 = 365 * 4 + 1;

    let (days_total, secs_of_day) = {
        let (d, r) = divmod_i64(secs_since_epoch, SECS_PER_DAY);
        (d - LEAPOCH, r)
    };

    let mut wday = (3 + days_total).rem_euclid(7);
    if wday <= 0 {
        wday += 7;
    }
    let woff = ((wday as usize) - 1) * 3;

    let qc_cycles = days_total.div_euclid(DAYS_PER_400Y);
    let mut remdays = days_total.rem_euclid(DAYS_PER_400Y);

    let mut c_cycles = remdays / DAYS_PER_100Y;
    if c_cycles == 4 {
        c_cycles -= 1;
    }
    remdays -= c_cycles * DAYS_PER_100Y;

    let mut q_cycles = remdays / DAYS_PER_4Y;
    if q_cycles == 25 {
        q_cycles -= 1;
    }
    remdays -= q_cycles * DAYS_PER_4Y;

    let mut remyears = remdays / 365;
    if remyears == 4 {
        remyears -= 1;
    }
    remdays -= remyears * 365;

    let mut year = 2000 + remyears + 4 * q_cycles + 100 * c_cycles + 400 * qc_cycles;

    const WDAY_STRS: &[u8; 21] = b"MonTueWedThuFriSatSun";
    const MON_STRS: &[u8; 36] = b"JanFebMarAprMayJunJulAugSepOctNovDec";
    const MONTHS: [i64; 12] = [31, 30, 31, 30, 31, 31, 30, 31, 30, 31, 31, 29];

    let mut mon_idx = 0;
    let mut rd = remdays;
    while mon_idx < 12 {
        let ml = MONTHS[mon_idx];
        if rd < ml {
            break;
        }
        rd -= ml;
        mon_idx += 1;
    }

    let mday = (rd + 1) as u8;

    let mut mon = mon_idx + 3;
    if mon > 12 {
        year += 1;
        mon -= 12;
    }
    let mon = mon as u8;

    let (hour, rem) = divmod_i64(secs_of_day, SECS_PER_HOUR);
    let (min, sec) = divmod_i64(rem, SECS_PER_MIN);
    let hour = hour as u8;
    let min = min as u8;
    let sec = sec as u8;

    buf[6..9].copy_from_slice(&WDAY_STRS[woff..woff + 3]);

    write_2d(&mut buf[11..13], mday);

    let moff = ((mon as usize) - 1) * 3;
    buf[14..17].copy_from_slice(&MON_STRS[moff..moff + 3]);

    // Year
    write_4d(&mut buf[18..22], year as u16);

    // HH:MM:SS
    write_2d(&mut buf[23..25], hour);
    write_2d(&mut buf[26..28], min);
    write_2d(&mut buf[29..31], sec);
}

#[inline]
fn write_2d(buf: &mut [u8], v: u8) {
    buf[0] = b'0' + (v / 10);
    buf[1] = b'0' + (v % 10);
}

#[inline]
fn write_4d(buf: &mut [u8], v: u16) {
    buf[0] = b'0' + ((v / 1000) as u8);
    buf[1] = b'0' + ((v / 100 % 10) as u8);
    buf[2] = b'0' + ((v / 10 % 10) as u8);
    buf[3] = b'0' + ((v % 10) as u8);
}
