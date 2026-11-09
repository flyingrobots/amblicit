//! Small, bounded schedule corpus. No filesystem, network, clocks, or threads.
//! Seed input is fixed here and recorded by the guarded launcher before execution.

use std::collections::BTreeSet;

#[path = "traces/adapter.rs"]
mod adapter;
#[path = "traces/generator.rs"]
mod generator;
#[path = "traces/oracle.rs"]
mod oracle;
#[path = "traces/program.rs"]
mod program;
#[path = "traces/regressions.rs"]
mod regressions;
#[path = "traces/runner.rs"]
mod runner;
#[path = "traces/shrink.rs"]
mod shrink;

#[test]
fn minimized_capacity_trace_refuses_without_publication() {
    assert_eq!(
        runner::run(&regressions::capacity()),
        Ok(BTreeSet::from([runner::Event::Refused]))
    );
}

#[test]
fn minimized_empty_range_trace_rejects_nonmatching_insertion() {
    assert_eq!(
        runner::run(&regressions::empty_range()),
        Ok(BTreeSet::from([
            runner::Event::Accepted,
            runner::Event::Refused,
            runner::Event::Read,
            runner::Event::Conflict,
        ]))
    );
}

#[test]
fn minimized_ownership_trace_refuses_orphan_publication() {
    assert_eq!(
        runner::run(&regressions::ownership()),
        Ok(BTreeSet::from([runner::Event::Refused]))
    );
}

#[test]
fn bounded_trace_corpus_preserves_collection_laws() {
    let mut coverage = BTreeSet::new();
    for seed in program::SEEDS {
        for case in 0..program::CASES_PER_SEED {
            let trace = generator::generate(seed, case);
            match runner::run(&trace) {
                Ok(events) => coverage.extend(events),
                Err(failure) => {
                    let reduced = shrink::reduce(&trace, failure.kind);
                    eprintln!("MODEL_COUNTEREXAMPLE kind={:?} seed={seed} case={case} one_minimal={} replays={}",
                        failure.kind, reduced.one_minimal, reduced.replays);
                    eprintln!(
                        "ORIGINAL_FAILURE step={:?} expected={:?} actual={:?}",
                        failure.step, failure.expected, failure.actual
                    );
                    eprintln!("MINIMIZED_TRACE {:#?}", reduced.trace);
                    assert!(
                        reduced.one_minimal,
                        "counterexample reduction exhausted its bound"
                    );
                    panic!(
                        "model violates independent trace oracle: {:?}",
                        failure.kind
                    );
                }
            }
        }
    }
    assert_eq!(
        coverage,
        BTreeSet::from([
            runner::Event::Accepted,
            runner::Event::Refused,
            runner::Event::Read,
            runner::Event::Conflict,
        ]),
        "the bounded corpus must exercise every declared outcome family"
    );
    eprintln!(
        "TRACE_CORPUS_OK seeds={:?} cases_per_seed={} coverage={coverage:?}",
        program::SEEDS,
        program::CASES_PER_SEED
    );
}
