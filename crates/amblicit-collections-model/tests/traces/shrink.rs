//! Bounded single-deletion reduction preserving the observed violation class.

use super::{
    program::{Cell, Step, Trace, Violation},
    runner,
};

const MAX_REPLAYS: usize = 16_384;

pub struct Reduced {
    pub trace: Trace,
    pub replays: usize,
    pub one_minimal: bool,
}

pub fn reduce(original: &Trace, kind: Violation) -> Reduced {
    let mut reduced = Reduced {
        trace: original.clone(),
        replays: 0,
        one_minimal: false,
    };
    loop {
        let mut next = None;
        for candidate in deletions(&reduced.trace) {
            if !candidate.valid() {
                continue;
            }
            if reduced.replays == MAX_REPLAYS {
                return reduced;
            }
            reduced.replays = reduced
                .replays
                .checked_add(1)
                .expect("bounded by MAX_REPLAYS");
            if runner::run(&candidate).is_err_and(|failure| failure.kind == kind) {
                next = Some(candidate);
                break;
            }
        }
        if let Some(next) = next {
            reduced.trace = next;
        } else {
            reduced.one_minimal = true;
            return reduced;
        }
    }
}

fn deletions(trace: &Trace) -> Vec<Trace> {
    let mut candidates = vec![];
    for cell in Cell::ALL {
        if trace.initial.get(cell).is_some() {
            let mut candidate = trace.clone();
            candidate.initial.set(cell, None);
            candidates.push(candidate);
        }
    }
    for index in 0..trace.steps.len() {
        let mut candidate = trace.clone();
        candidate.steps.remove(index);
        candidates.push(candidate);
        candidates.extend(request_deletions(trace, index));
    }
    candidates
}

fn request_deletions(trace: &Trace, index: usize) -> Vec<Trace> {
    let Step::Prepare { requests, .. } = &trace.steps[index] else {
        return vec![];
    };
    (0..requests.len())
        .map(|request| {
            let mut candidate = trace.clone();
            if let Step::Prepare { requests, .. } = &mut candidate.steps[index] {
                requests.remove(request);
            }
            candidate
        })
        .collect()
}
