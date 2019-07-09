//

use std::mem::{drop, replace};
use std::ops::Deref;
use std::sync::RwLock;

pub struct Collector<T: Keep + Send> {
    slots: Vec<Value<T>>,
    slot_max: usize,
}

fn make_value<T>(value: T) -> Value<T> {
    Value(Box::into_raw(Box::new(Slot {
        content: value,
        mark: false,
    })))
}

impl<T: Keep + Send> Collector<T> {
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

pub struct Allocator<'a, T: Keep + Send> {
    collector: &'a RwLock<Collector<T>>,
    slots: Vec<Value<T>>,
    slot_max: usize,
    entry: Value<T>,
}

impl<'a, T: Keep + Send> Allocator<'a, T> {
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

unsafe impl<T: Sync> Send for Value<T> {}
unsafe impl<T: Sync> Sync for Value<T> {}

impl<T> Value<T> {
    fn get(&self) -> &T {
        unsafe { &(&*self.0).content }
    }

    pub unsafe fn get_mut(&self) -> &mut T {
        &mut (&mut *self.0).content
    }
}

impl<T> Deref for Value<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        self.to_owned()
    }
}

impl<T> Copy for Value<T> {}

impl<'a, T: Keep + Send> Allocator<'a, T> {
    pub fn allocate(&mut self, value: T) -> Value<T> {
        if self.slots.len() == self.slot_max {
            self.clean();
        }

        let value = make_value(value);
        self.slots.push(value);
        value
    }

    pub fn clean(&mut self) {
        let old_slots = replace(&mut self.slots, Vec::new());
        self.collector.write().unwrap().store(old_slots);
    }
}

impl<'a, T: Keep + Send> Drop for Allocator<'a, T> {
    fn drop(&mut self) {
        if self.slots.len() != 0 {
            self.clean();
        }
    }
}

impl<T: Keep + Send> Collector<T> {
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

pub unsafe trait Keep: Sized {
    fn keep(&self) -> Vec<Value<Self>>;
}

impl<T: Keep + Send> Collector<T> {
    pub fn collect(&mut self) {
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

impl<'a, T: Keep + Send> Allocator<'a, T> {
    pub fn entry(&self) -> Value<T> {
        self.entry
    }
}

impl<T: Keep + Send> Drop for Collector<T> {
    fn drop(&mut self) {
        let old_slots = replace(&mut self.slots, Vec::new());
        for slot in old_slots.into_iter() {
            unsafe {
                drop(Box::from_raw(slot.0));
            }
        }
    }
}
