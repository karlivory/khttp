use khttp::date::get_date_from_secs;

const DATE_LEN: usize = 37;
fn to_string(bytes: [u8; DATE_LEN]) -> String {
    String::from_utf8(bytes.to_vec()).expect("http-date must be ASCII")
}

#[test]
fn http_date_known_vectors() {
    let cases: &[(i64, &str)] = &[
        (-14_182_940, "date: Sun, 20 Jul 1969 20:17:40 GMT"), // pre-epoch
        (0, "date: Thu, 01 Jan 1970 00:00:00 GMT"),
        (1, "date: Thu, 01 Jan 1970 00:00:01 GMT"),
        (59, "date: Thu, 01 Jan 1970 00:00:59 GMT"),
        (60, "date: Thu, 01 Jan 1970 00:01:00 GMT"),
        (86399, "date: Thu, 01 Jan 1970 23:59:59 GMT"),
        (86400, "date: Fri, 02 Jan 1970 00:00:00 GMT"),
        (-1, "date: Wed, 31 Dec 1969 23:59:59 GMT"),
        (-60, "date: Wed, 31 Dec 1969 23:59:00 GMT"),
        (-61, "date: Wed, 31 Dec 1969 23:58:59 GMT"),
        (946684799, "date: Fri, 31 Dec 1999 23:59:59 GMT"),
        (946684800, "date: Sat, 01 Jan 2000 00:00:00 GMT"),
        (951827696, "date: Tue, 29 Feb 2000 12:34:56 GMT"),
        (951868800, "date: Wed, 01 Mar 2000 00:00:00 GMT"),
        (1136073600, "date: Sun, 01 Jan 2006 00:00:00 GMT"),
        (1230768000, "date: Thu, 01 Jan 2009 00:00:00 GMT"),
        (1456704000, "date: Mon, 29 Feb 2016 00:00:00 GMT"),
        (1456790399, "date: Mon, 29 Feb 2016 23:59:59 GMT"),
        (1709164800, "date: Thu, 29 Feb 2024 00:00:00 GMT"),
        (1709251199, "date: Thu, 29 Feb 2024 23:59:59 GMT"),
        (1754956800, "date: Tue, 12 Aug 2025 00:00:00 GMT"),
        (2147483646, "date: Tue, 19 Jan 2038 03:14:06 GMT"),
        (2147483647, "date: Tue, 19 Jan 2038 03:14:07 GMT"),
        (2147483648, "date: Tue, 19 Jan 2038 03:14:08 GMT"),
        (-14182940, "date: Sun, 20 Jul 1969 20:17:40 GMT"),
        (-2208988800, "date: Mon, 01 Jan 1900 00:00:00 GMT"),
        (-2203891201, "date: Wed, 28 Feb 1900 23:59:59 GMT"),
        (-2203891200, "date: Thu, 01 Mar 1900 00:00:00 GMT"),
        (951782399, "date: Mon, 28 Feb 2000 23:59:59 GMT"),
        (951782400, "date: Tue, 29 Feb 2000 00:00:00 GMT"),
        (13574563200, "date: Tue, 29 Feb 2400 00:00:00 GMT"),
        (13574649600, "date: Wed, 01 Mar 2400 00:00:00 GMT"),
        (915148800, "date: Fri, 01 Jan 1999 00:00:00 GMT"),
        (1330516800, "date: Wed, 29 Feb 2012 12:00:00 GMT"),
        (1435708799, "date: Tue, 30 Jun 2015 23:59:59 GMT"),
        (1435708800, "date: Wed, 01 Jul 2015 00:00:00 GMT"),
        (1582956428, "date: Sat, 29 Feb 2020 06:07:08 GMT"),
        (1704067199, "date: Sun, 31 Dec 2023 23:59:59 GMT"),
        (1704067200, "date: Mon, 01 Jan 2024 00:00:00 GMT"),
        (1735689599, "date: Tue, 31 Dec 2024 23:59:59 GMT"),
        (4102444799, "date: Thu, 31 Dec 2099 23:59:59 GMT"),
    ];

    for &(secs, expected) in cases {
        let got = to_string(get_date_from_secs(secs));
        assert_eq!(&got[..35], expected, "bad format for {secs}");
        assert!(got.starts_with("date: "), "missing prefix for {secs}");
        assert!(got.ends_with(" GMT\r\n"), "missing GMT suffix for {secs}");
    }
}
