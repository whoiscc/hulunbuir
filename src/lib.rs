//

use std::mem::{drop, replace};
use std::ops::Deref;
use std::sync::Mutex;

struct CollectorInternal<T: Keep> {
    slots: Vec<Value<T>>,
    slot_max: usize,
}

pub struct Collector<T: Keep>(Mutex<CollectorInternal<T>>);

fn make_value<T>(value: T) -> Value<T> {
    Value(Box::into_raw(Box::new(Slot {
        content: value,
        mark: false,
    })))
}

impl<T: Keep> CollectorInternal<T> {
    fn new(entry: T, global_max: usize) -> Self {
        Self {
            slots: vec![make_value(entry)],
            slot_max: global_max,
        }
    }

    fn entry(&self) -> Value<T> {
        self.slots[0]
    }

    fn slot_len(&self) -> usize {
        self.slots.len()
    }
}

impl<T: Keep> Collector<T> {
    pub fn new(entry: T, global_max: usize) -> Self {
        Self(Mutex::new(CollectorInternal::new(entry, global_max)))
    }

    fn entry(&self) -> Value<T> {
        self.0.lock().unwrap().entry()
    }

    pub fn slot_len(&self) -> usize {
        self.0.lock().unwrap().slot_len()
    }
}

pub struct Allocator<'a, T: Keep> {
    collector: &'a Collector<T>,
    slots: Vec<Value<T>>,
    slot_max: usize,
    entry: Value<T>,
}

impl<'a, T: Keep> Allocator<'a, T> {
    pub fn new(collector: &'a Collector<T>, local_max: usize) -> Self {
        let entry = collector.entry();
        Self {
            collector,
            slots: Vec::new(),
            slot_max: local_max,
            entry,
        }
    }

    pub fn slot_len(&self) -> usize {
        self.slots.len()
    }
}

struct Slot<T> {
    content: T,
    mark: bool,
}
pub struct Value<T>(*mut Slot<T>);

// this one is easy: Value<T> is some kinds of &T, so Value<T> is Send iff T is Sync
unsafe impl<T: Sync> Send for Value<T> {}
// this one is hard
// after all, if you want Collector and Allocator live in different threads, you need
// to pass T cross thread boundary when calling Collector::store, so T must be Send
// on the other hand, if Collector and Allocator live in different threads, &Collector
// must be Send, so Collector must be Sync, so as Vec<Value<T>>, and Value<T>
// something like this
unsafe impl<T: Send> Sync for Value<T> {}

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
        Value(self.0)
    }
}

impl<T> Copy for Value<T> {}

impl<'a, T: Keep> Allocator<'a, T> {
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
        self.collector.store(old_slots);
    }
}

impl<'a, T: Keep> Drop for Allocator<'a, T> {
    fn drop(&mut self) {
        if self.slots.len() != 0 {
            self.clean();
        }
    }
}

impl<T: Keep> CollectorInternal<T> {
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

impl<T: Keep> Collector<T> {
    fn store(&self, slots: Vec<Value<T>>) {
        self.0.lock().unwrap().store(slots)
    }
}

pub unsafe trait Keep: Sized {
    fn keep(&self) -> Vec<Value<Self>>;
}

impl<T: Keep> CollectorInternal<T> {
    fn collect(&mut self) {
        let mut stack = vec![self.slots[0].0];
        while let Some(slot) = stack.pop() {
            unsafe {
                let slot_mut = &mut *slot;
                if slot_mut.mark {
                    continue;
                }
                slot_mut.mark = true;
                for value in slot_mut.content.keep().into_iter() {
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

impl<T: Keep> Collector<T> {
    pub fn collect(&self) {
        self.0.lock().unwrap().collect()
    }
}

impl<'a, T: Keep> Allocator<'a, T> {
    pub fn entry(&self) -> Value<T> {
        self.entry
    }
}

impl<T: Keep> Drop for CollectorInternal<T> {
    fn drop(&mut self) {
        let old_slots = replace(&mut self.slots, Vec::new());
        for slot in old_slots.into_iter() {
            unsafe {
                drop(Box::from_raw(slot.0));
            }
        }
    }
}
