//

use std::sync::RwLock;
use std::mem::{replace, drop};

pub struct Collector<T> {
    slots: Vec<Value<T>>,
    slot_max: usize,
}

fn make_value<T>(value: T) -> Value<T> {
    Value(Box::into_raw(Box::new(Slot {
        content: value,
        mark: false,
    })))
}

impl<T> Collector<T> {
    pub fn new(entry: T, global_max: usize) -> Self {
        Self {
            slots: vec![make_value(entry)],
            slot_max: global_max,
        }
    }

    fn entry(&self) -> Value<T> {
        self.slots[0]
    }
}

pub struct Allocator<'a, T> {
    collector: &'a RwLock<Collector<T>>,
    slots: Vec<Value<T>>,
    slot_max: usize,
    entry: Value<T>,
}

impl<'a, T> Allocator<'a, T> {
    pub fn new(collector: &'a RwLock<Collector<T>>, local_max: usize) -> Self {
        let entry = collector.read().unwrap().entry();
        Self {
            collector,
            slots: Vec::new(),
            slot_max: local_max,
            entry,
        }
    }
}

struct Slot<T> {
    content: T,
    mark: bool,
}

pub struct Value<T>(*mut Slot<T>);

unsafe impl<T: Sync> Sync for Value<T> {}

impl<T> Value<T> {
    pub unsafe fn get_mut(&self) -> &mut T {
        &mut (&mut *self.0).content
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        self.to_owned()
    }
}

impl<T> Copy for Value<T> {}

impl<'a, T: Keep> Allocator<'a, T> {
    pub fn allocate(&mut self, value: T) -> Value<T> {
        if self.slots.len() == self.slot_max {
            let old_slots = replace(&mut self.slots, Vec::new());
            self.collector.write().unwrap().store(old_slots);
        }

        let value = make_value(value);
        self.slots.push(value);
        value
    }
}

impl<T: Keep> Collector<T> {
    fn store(&mut self, slots: Vec<Value<T>>) {
        self.slots.extend(slots.into_iter());
        if self.slots.len() >= self.slot_max {
            self.collect();
        }
        if self.slots.len() >= self.slot_max {
            panic!("memory overflow");
        }
    }
}

pub trait Keep: Sized {
    fn keep(&self) -> &[Value<Self>];
}

impl<T: Keep> Collector<T> {
    fn collect(&mut self) {
        let mut stack = vec![self.slots[0].0];
        while let Some(slot) = stack.pop() {
            unsafe { 
                let slot_mut = &mut *slot;
                slot_mut.mark = true;
                for value in slot_mut.content.keep() {
                    stack.push(value.0);
                }
            }
        }

        let old_slots = replace(&mut self.slots, Vec::new());
        for slot in old_slots.into_iter() {
            unsafe {
                if (&*slot.0).mark {
                    (&mut *slot.0).mark = false;
                    self.slots.push(slot);
                } else {
                    drop(Box::from_raw(slot.0));
                }
            }
        }
    }
}

impl<'a, T> Allocator<'a, T> {
    pub fn entry(&self) -> Value<T> {
        self.entry
    }
}
