//! Seeded finite families and random walks, with explicit schedule bounds.

use super::program::{Actor, Cell, Request, Selection, Space, State, Step, Trace, MAX_STEPS};

struct Generator(u64);

impl Generator {
    fn next(&mut self) -> u64 {
        // Wrapping is the specified test PRNG recurrence, not domain arithmetic.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0 ^ (self.0 >> 32)
    }

    fn choose<T: Copy>(&mut self, values: &[T]) -> T {
        let limit = u64::try_from(values.len()).expect("finite choice table");
        let index = usize::try_from(self.next() % limit).expect("index fits choice table");
        *values.get(index).expect("nonempty finite choice table")
    }

    fn value(&mut self) -> i64 {
        self.choose(&[-1, 0, 1, 2])
    }
    fn actor(&mut self) -> Actor {
        self.choose(&[Actor::First, Actor::Second])
    }

    fn request(&mut self) -> Request {
        let cell = self.choose(&Cell::ALL);
        match self.next() % 6 {
            0 => Request::Add(cell, self.value()),
            1 => Request::Remove(cell),
            2 => Request::Replace(cell, self.value()),
            3 => Request::Get(cell),
            4 => Request::Iter(self.choose(&Space::ALL), Selection::All),
            _ => Request::Iter(self.choose(&Space::ALL), Selection::Equal(self.value())),
        }
    }
}

pub fn generate(seed: u64, case: u16) -> Trace {
    let mut rng = Generator(seed.wrapping_add(u64::from(case).wrapping_mul(0x9e37_79b9_7f4a_7c15)));
    let mut trace = Trace {
        seed,
        case,
        capacity: 1,
        initial: State::EMPTY,
        steps: vec![],
    };
    match case % 4 {
        0 => capacity_family(&mut trace, &mut rng),
        1 => range_family(&mut trace, &mut rng),
        2 => ownership_family(&mut trace, &mut rng),
        _ => random_walk(&mut trace, &mut rng),
    }
    trace
}

fn noise(trace: &mut Trace, actor: Actor, cell: Cell) {
    trace.steps.push(Step::Prepare {
        actor,
        requests: vec![Request::Get(cell)],
    });
    trace.steps.push(Step::Commit(actor));
}

fn capacity_family(trace: &mut Trace, rng: &mut Generator) {
    let actor = rng.actor();
    let space = rng.choose(&[Space::Owners, Space::Remote]);
    let [first, second] = space.cells();
    let unrelated = if space == Space::Owners {
        Cell::RemoteA
    } else {
        Cell::Owner
    };
    trace.initial.set(unrelated, Some(rng.value()));
    noise(trace, actor.other(), unrelated);
    let requests = if rng.next().is_multiple_of(2) {
        vec![
            Request::Add(second, rng.value()),
            Request::Get(unrelated),
            Request::Add(first, rng.value()),
        ]
    } else {
        trace.initial.set(first, Some(rng.value()));
        vec![Request::Get(unrelated), Request::Add(second, rng.value())]
    };
    trace.steps.push(Step::Prepare { actor, requests });
    trace.steps.push(Step::Commit(actor));
}

fn range_family(trace: &mut Trace, rng: &mut Generator) {
    let reader = rng.actor();
    let writer = reader.other();
    trace.initial.set(Cell::Owner, Some(rng.value()));
    noise(trace, writer, Cell::Owner);
    trace.steps.push(Step::Prepare {
        actor: reader,
        requests: vec![
            Request::Get(Cell::Owner),
            Request::Iter(Space::Remote, Selection::Equal(rng.value())),
        ],
    });
    trace.steps.push(Step::Prepare {
        actor: writer,
        requests: vec![Request::Add(
            rng.choose(&Space::Remote.cells()),
            rng.value(),
        )],
    });
    let first = rng.choose(&[reader, writer]);
    trace.steps.push(Step::Commit(first));
    trace.steps.push(Step::Commit(first.other()));
}

fn ownership_family(trace: &mut Trace, rng: &mut Generator) {
    let actor = rng.actor();
    trace.initial.set(Cell::Owner, Some(rng.value()));
    trace.initial.set(Cell::RemoteA, Some(rng.value()));
    noise(trace, actor.other(), Cell::RemoteA);
    let remove = Request::Remove(Cell::Owner);
    let add = Request::Add(rng.choose(&Space::Children.cells()), rng.value());
    let requests = if rng.next().is_multiple_of(2) {
        vec![remove, add]
    } else {
        vec![add, remove]
    };
    trace.steps.push(Step::Prepare { actor, requests });
    trace.steps.push(Step::Commit(actor));
}

fn random_walk(trace: &mut Trace, rng: &mut Generator) {
    trace.capacity = rng.choose(&[1, 2]);
    for cell in Cell::ALL {
        let mut proposed = trace.initial;
        proposed.set(cell, Some(rng.value()));
        if rng.next().is_multiple_of(2)
            && !proposed.has_orphan()
            && !proposed.over_capacity(trace.capacity)
        {
            trace.initial = proposed;
        }
    }
    for _ in 0..MAX_STEPS - 2 {
        let actor = rng.actor();
        if rng.next().is_multiple_of(3) {
            trace.steps.push(Step::Commit(actor));
        } else {
            let count = rng.next() % 5;
            let requests = (0..count).map(|_| rng.request()).collect();
            trace.steps.push(Step::Prepare { actor, requests });
        }
    }
    trace
        .steps
        .extend([Step::Commit(Actor::First), Step::Commit(Actor::Second)]);
}
