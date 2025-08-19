use crate::{BenchId, CriterionBench};
use criterion::{BenchmarkGroup, BenchmarkId, measurement::WallTime};

pub const DATE_BENCHES: &[CriterionBench] = &[
    CriterionBench {
        id: BenchId::new("date", "uncached"),
        run: bench_date_uncached,
    },
    CriterionBench {
        id: BenchId::new("date", "httpdate"),
        run: bench_date_httpdate,
    },
];

// ---------------------------------------------------------------------
// benchmark functions
// ---------------------------------------------------------------------

fn bench_date_uncached(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("date:uncached", ""), |b| {
        b.iter(|| {
            let d = khttp::date::get_date_now_uncached();
            std::hint::black_box(d); // prevent optimization
        });
    });
}

fn bench_date_httpdate(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("date:httpdate", ""), |b| {
        b.iter(|| {
            let s = httpdate::fmt_http_date(std::time::SystemTime::now());
            std::hint::black_box(&s); // prevent optimization
        });
    });
}
