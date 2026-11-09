//! Private live-world identity; never serialized or used as content identity.

use std::rc::Rc;

#[derive(Clone, Debug)]
pub(crate) struct Origin(Rc<()>);

impl Origin {
    pub(crate) fn new() -> Self {
        Self(Rc::new(()))
    }
}

impl PartialEq for Origin {
    fn eq(&self, other: &Self) -> bool {
        // Equality means the same live authority, not equal empty payloads.
        // No pointer value escapes, affects ordering, or enters artifact bytes.
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Origin {}
