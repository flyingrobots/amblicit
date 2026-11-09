//! Finite trace vocabulary and independent array-state invariants.

pub const SEEDS: [u64; 4] = [159, 296, 290, 201_400_007];
pub const CASES_PER_SEED: u16 = 64;
pub const MAX_STEPS: usize = 24;
pub const MAX_REQUESTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    First,
    Second,
}

impl Actor {
    pub fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }
    pub fn other(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Owner,
    Peer,
    RemoteA,
    RemoteB,
    ChildA,
    ChildB,
}

impl Cell {
    pub const ALL: [Self; 6] = [
        Self::Owner,
        Self::Peer,
        Self::RemoteA,
        Self::RemoteB,
        Self::ChildA,
        Self::ChildB,
    ];
    pub fn index(self) -> usize {
        match self {
            Self::Owner => 0,
            Self::Peer => 1,
            Self::RemoteA => 2,
            Self::RemoteB => 3,
            Self::ChildA => 4,
            Self::ChildB => 5,
        }
    }
    pub fn space(self) -> Space {
        match self {
            Self::Owner | Self::Peer => Space::Owners,
            Self::RemoteA | Self::RemoteB => Space::Remote,
            Self::ChildA | Self::ChildB => Space::Children,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Owners,
    Remote,
    Children,
}

impl Space {
    pub const ALL: [Self; 3] = [Self::Owners, Self::Remote, Self::Children];
    pub fn cells(self) -> [Cell; 2] {
        match self {
            Self::Owners => [Cell::Owner, Cell::Peer],
            Self::Remote => [Cell::RemoteA, Cell::RemoteB],
            Self::Children => [Cell::ChildA, Cell::ChildB],
        }
    }
    pub fn owner(self) -> Option<Cell> {
        match self {
            Self::Children => Some(Cell::Owner),
            Self::Owners | Self::Remote => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    All,
    Equal(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Add(Cell, i64),
    Remove(Cell),
    Replace(Cell, i64),
    Get(Cell),
    Iter(Space, Selection),
}

impl Request {
    pub fn mutation(self) -> Option<Cell> {
        match self {
            Self::Add(cell, _) | Self::Remove(cell) | Self::Replace(cell, _) => Some(cell),
            Self::Get(_) | Self::Iter(_, _) => None,
        }
    }
    pub fn space(self) -> Space {
        match self {
            Self::Add(cell, _) | Self::Remove(cell) | Self::Replace(cell, _) | Self::Get(cell) => {
                cell.space()
            }
            Self::Iter(space, _) => space,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Prepare {
        actor: Actor,
        requests: Vec<Request>,
    },
    Commit(Actor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State(pub [Option<i64>; 6]);

impl State {
    pub const EMPTY: Self = Self([None; 6]);
    pub fn get(self, cell: Cell) -> Option<i64> {
        self.0[cell.index()]
    }
    pub fn set(&mut self, cell: Cell, value: Option<i64>) {
        self.0[cell.index()] = value;
    }
    pub fn count(self, space: Space) -> usize {
        space
            .cells()
            .into_iter()
            .filter(|cell| self.get(*cell).is_some())
            .count()
    }
    pub fn has_orphan(self) -> bool {
        self.get(Cell::Owner).is_none() && self.count(Space::Children) != 0
    }
    pub fn over_capacity(self, capacity: u16) -> bool {
        Space::ALL
            .into_iter()
            .any(|space| self.count(space) > usize::from(capacity))
    }
    pub fn apply(&mut self, request: Request) {
        match request {
            Request::Add(cell, value) | Request::Replace(cell, value) => {
                self.set(cell, Some(value));
            }
            Request::Remove(cell) => self.set(cell, None),
            Request::Get(_) | Request::Iter(_, _) => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub seed: u64,
    pub case: u16,
    pub capacity: u16,
    pub initial: State,
    pub steps: Vec<Step>,
}

impl Trace {
    pub fn valid(&self) -> bool {
        matches!(self.capacity, 1 | 2)
            && self.steps.len() <= MAX_STEPS
            && !self.initial.has_orphan()
            && !self.initial.over_capacity(self.capacity)
            && self.steps.iter().all(|step| match step {
                Step::Prepare { requests, .. } => requests.len() <= MAX_REQUESTS,
                Step::Commit(_) => true,
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Unavailable,
    DuplicateMutation,
    Conflict,
    OwnerMissing,
    AlreadyPresent,
    NotFound,
    NotEmpty,
    OwnerSurvival,
    Capacity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedRead {
    Slot(Option<i64>),
    Entries(Vec<(Cell, i64)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    InvalidTrace,
    Construction,
    Preparation,
    Readings,
    UnexpectedAdmission,
    UnexpectedRefusal,
    StaleAccepted,
    WrongRefusal,
    AtomicRefusal,
    Membership,
    UnknownPlace,
    UnexpectedContent,
    Capacity,
    Ownership,
}
