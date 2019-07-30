//

use std::collections::HashMap;
use std::mem;

pub struct Collector<T> {
    slots: HashMap<Address, Slot<T>>,
    slot_max: usize,
    next_id: usize,
    root: Option<Address>,
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct Address(usize);

pub unsafe trait Keep {
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

    pub fn allocate(&mut self, value: T) -> Address {
        if self.slots.len() == self.slot_max {
            //
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

    pub fn replace(&mut self, address: Address, value: T) -> T {
        let slot = self.slots.get_mut(&address).unwrap();
        let content = mem::replace(&mut slot.content, value);
        content
    }

    pub fn set_root(&mut self, address: Address) {
        self.root = Some(address);
    }

    pub fn root(&self) -> Option<Address> {
        self.root.to_owned()
    }

    pub fn alive_count(&self) -> usize {
        self.slots.len()
    }
}

impl<T: Keep> Collector<T> {
    pub fn collect(&mut self) {
        let mut stack = Vec::new();
        if let Some(address) = &self.root {
            stack.push(address.to_owned());
        }
        while let Some(address) = stack.pop() {
            let slot = self.slots.get_mut(&address).unwrap();
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
    }
}

struct Slot<T> {
    mark: bool,
    content: T,
}

pub enum SlotContent<T> {
    Value(T),
    Placeholder(Vec<Address>),
}

unsafe impl<T: Keep> Keep for SlotContent<T> {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        match self {
            SlotContent::Value(value) => value.with_keep(f),
            SlotContent::Placeholder(keep_list) => f(keep_list),
        }
    }
}
