//! Hulunbuir is a cross-thread garbage collector. The managed objects could be used in 
//! multithreads, and collecting process may happen in any of them.
//! 
//! Normally, reading or updating a managed object must lock global collector as well, 
//! which significantly decrease multithread performance. However, Hulunbuir does not provide
//! common "read guard" and "write guard" interface; instead it only supports two functions:
//! `allocate` and `replace`. The first one create a managed object, and may trigger a garbage
//! collecting process if necessary; the second one replace the value of a managed object with
//! a new one provided by argument. The global collector only have to be locked during replacing
//! and the lock could be released when working thread owns the value. So the lock will not
//! become the bottleneck of performance.
//! 
//! Hulunbuir also provides `Slot` as higher level abstraction and interface.

pub mod slot;

use std::collections::HashMap;
use std::mem;
use std::time::Instant;

pub struct Collector<T> {
    slots: HashMap<Address, Slot<T>>,
    slot_max: usize,
    next_id: usize,
    root: Option<Address>,
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct Address(usize);

pub trait Keep {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F);
}

impl<T> Collector<T> {
    pub fn new(slot_max: usize) -> Self {
        Self {
            slots: HashMap::new(),
            slot_max,
            next_id: 0,
            root: None,
        }
    }

    pub fn replace(&mut self, address: &Address, value: T) -> T {
        let slot = self.slots.get_mut(address).unwrap();
        let content = mem::replace(&mut slot.content, value);
        content
    }

    pub fn set_root(&mut self, address: Address) {
        self.root = Some(address);
    }

    pub fn root(&self) -> &Option<Address> {
        &self.root
    }

    pub fn alive_count(&self) -> usize {
        self.slots.len()
    }
}

impl<T: Keep> Collector<T> {
    pub fn allocate(&mut self, value: T) -> Address {
        if self.slots.len() == self.slot_max {
            self.collect();
        }
        if self.slots.len() == self.slot_max {
            panic!();
        }
        let address = Address(self.next_id);
        self.next_id += 1;
        self.slots.insert(
            address.clone(),
            Slot {
                mark: false,
                content: value,
            },
        );
        address
    }

    pub fn collect(&mut self) {
        let start = Instant::now();

        let mut stack = Vec::new();
        if let Some(address) = &self.root {
            stack.push(address.to_owned());
        }
        while let Some(address) = stack.pop() {
            let slot = self.slots.get_mut(&address).unwrap();
            if slot.mark {
                continue;
            }
            slot.mark = true;
            slot.content.with_keep(|keep_list| {
                stack.extend(keep_list.to_owned());
            });
        }
        let mut alive_slots = HashMap::new();
        for (address, slot) in mem::replace(&mut self.slots, HashMap::new()).into_iter() {
            if slot.mark {
                alive_slots.insert(
                    address,
                    Slot {
                        mark: false,
                        content: slot.content,
                    },
                );
            }
        }
        self.slots = alive_slots;

        println!(
            "[hulunbuir] garbage collected in {} ms, {:.2}% slots used",
            start.elapsed().as_micros() as f32 / 1000.0,
            self.slots.len() as f32 / self.slot_max as f32 * 100.0
        );
    }
}

struct Slot<T> {
    mark: bool,
    content: T,
}
