//! Bench harness entrypoint.
//! - Server benches -> `rewrk` (spawned by server::run_server_bench)
//! - Parser benches -> Criterion
//! - Date benches   -> Criterion
//!
//! Run from repo root (example):
//!   cargo bench --manifest-path benchmarks/Cargo.toml -- server:minimal
//!
//! Filters allowed: `server`, `server:*`, `server:name`,
//!                  `axum`, `axum:*`, `axum:name`,
//!                  `parser`, `parser:*`, `parser:name`,
//!                  `httparse`, `httparse:*`, `httparse:name`,
//!                  `date`, `date:*`, `date:name`
//!
//! (inspired by axum's benches.rs)

#[path = "targets/date.rs"]
mod date;
#[path = "targets/parser.rs"]
mod parser;
#[path = "targets/server.rs"]
mod server;

use criterion::Criterion;
use date::DATE_BENCHES;
use parser::PARSER_BENCHES;
use server::SERVER_BENCHES;
use std::fmt;

fn main() {
    let filters = user_filters();
    let mut ran_any = false;

    let selected_servers: Vec<_> = SERVER_BENCHES
        .iter()
        .filter(|sb| should_run(&filters, sb.id))
        .collect();

    // Server benches (rewrk)
    if !selected_servers.is_empty() {
        ran_any = true;
        server::ensure_rewrk();
        for sb in selected_servers {
            server::run_server_bench(sb);
        }
    }

    ran_any |= run_criterion_targets("parser", PARSER_BENCHES, &filters);
    ran_any |= run_criterion_targets("date", DATE_BENCHES, &filters);

    if !ran_any && !filters.is_empty() {
        print_no_match(&filters);
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------
// types & utils
// ---------------------------------------------------------------------

#[derive(Copy, Clone)]
pub struct BenchId {
    pub group: &'static str,
    pub name: &'static str,
}

impl BenchId {
    pub const fn new(group: &'static str, name: &'static str) -> Self {
        Self { group, name }
    }
}

impl fmt::Display for BenchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.group, self.name)
    }
}

pub type CriterionBenchFn =
    fn(&mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>);

pub struct CriterionBench {
    pub id: BenchId,
    pub run: CriterionBenchFn,
}

fn run_criterion_targets(group_name: &str, benches: &[CriterionBench], filters: &[String]) -> bool {
    let selected: Vec<_> = benches
        .iter()
        .filter(|b| should_run(filters, b.id))
        .collect();

    if selected.is_empty() {
        return false;
    }

    let mut crit = Criterion::default();
    let mut group = crit.benchmark_group(group_name);
    for b in selected {
        eprintln!("Running {}", b.id);
        (b.run)(&mut group);
    }
    group.finish();
    crit.final_summary();
    true
}

fn user_filters() -> Vec<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        args
    }
}

fn should_run(filters: &[String], id: BenchId) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|t| match_token(t, id))
}

fn match_token(token: &str, id: BenchId) -> bool {
    if let Some(i) = token.find(':') {
        let (tg, tn) = token.split_at(i);
        let tn = &tn[1..];
        tg == id.group && (tn == "*" || tn == id.name)
    } else {
        // group-only match
        token == id.group
    }
}

fn print_no_match(filters: &[String]) {
    eprintln!("No benchmarks matched your filter(s): {:?}", filters);
    eprintln!("Available benches:");

    SERVER_BENCHES
        .iter()
        .map(|b| b.id)
        .chain(PARSER_BENCHES.iter().map(|b| b.id))
        .chain(DATE_BENCHES.iter().map(|b| b.id))
        .for_each(|id| eprintln!("{id}"));
}
