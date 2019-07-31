//

use crate::{Address, Collector, Keep};

use crossbeam::sync::{Parker, Unparker};

pub enum Slot<T> {
    Free(T),
    Busy { keep: Vec<Address>, unparkers: Vec<Unparker> },
}

impl<T> Slot<T> {
    pub fn new(value: T) -> Self {
        Slot::Free(value)
    }
}

impl<T: Keep> Keep for Slot<T> {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        match self {
            Slot::Free(value) => value.with_keep(f),
            Slot::Busy { keep, .. } => f(keep),
        }
    }
}

impl<T: Keep> Collector<Slot<T>> {
    pub fn take(&mut self, address: &Address) -> Result<T, Parker> {
        let mut keep = Vec::new();
        match &mut self.slots.get_mut(&address).unwrap().content {
            Slot::Free(value) => value.with_keep(|keep_list| keep = keep_list.into()),
            Slot::Busy { unparkers, .. } => {
                let parker = Parker::new();
                unparkers.push(parker.unparker().to_owned());
                return Err(parker);
            },
        }
        let busy = Slot::Busy {
            keep,
            unparkers: Vec::new(),
        };
        match self.replace(address, busy) {
            Slot::Free(value) => Ok(value),
            _ => unreachable!(),
        }
    }

    pub fn fill(&mut self, address: &Address, value: T) {
        match self.replace(address, Slot::Free(value)) {
            Slot::Free(_) => panic!(),
            Slot::Busy { unparkers, .. } => {
                for unparker in unparkers {
                    unparker.unpark();
                }
            },
        }
    }
}
