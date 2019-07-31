//

use crate::{Address, Collector, Keep, MemoryError};

use crossbeam::sync::{Parker, Unparker};

enum SlotPriv<T> {
    Free(T),
    Busy {
        keep: Vec<Address>,
        unparkers: Vec<Unparker>,
    },
}

pub struct Slot<T>(SlotPriv<T>);

impl<T> Slot<T> {
    pub fn new(value: T) -> Self {
        Self(SlotPriv::Free(value))
    }
}

impl<T: Keep> Keep for Slot<T> {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        match &self.0 {
            SlotPriv::Free(value) => value.with_keep(f),
            SlotPriv::Busy { keep, .. } => f(keep),
        }
    }
}

pub enum Take<T> {
    Free(T),
    Busy(Parker),
}

impl<T: Keep> Collector<Slot<T>> {
    pub fn take(&mut self, address: &Address) -> Result<Take<T>, MemoryError> {
        let mut keep = Vec::new();
        match &mut self.slots.get_mut(&address).unwrap().content.0 {
            SlotPriv::Free(value) => value.with_keep(|keep_list| keep = keep_list.into()),
            SlotPriv::Busy { unparkers, .. } => {
                let parker = Parker::new();
                unparkers.push(parker.unparker().to_owned());
                return Ok(Take::Busy(parker));
            }
        }
        let busy = Slot(SlotPriv::Busy {
            keep,
            unparkers: Vec::new(),
        });
        match self.replace(address, busy)?.0 {
            SlotPriv::Free(value) => Ok(Take::Free(value)),
            _ => unreachable!(),
        }
    }

    pub fn fill(&mut self, address: &Address, value: T) -> Result<(), MemoryError> {
        match self.replace(address, Slot(SlotPriv::Free(value)))?.0 {
            SlotPriv::Free(_) => Err(MemoryError::DuplicatedFilling),
            SlotPriv::Busy { unparkers, .. } => {
                for unparker in unparkers {
                    unparker.unpark();
                }
                Ok(())
            }
        }
    }
}
