//

use std::mem::drop;

pub unsafe trait Keep: Sized {
    fn keep(&self) -> &[Value<Self>];
}

pub struct Manager<T> {
    slots: Vec<*mut Slot<T>>,
}

struct Slot<T> {
    value: T,
    status: Status,
}

pub struct Value<T>(*mut Slot<T>);

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        self.to_owned()
    }
}

impl<T> Copy for Value<T> {}

impl<T> Value<T> {
    pub unsafe fn get_mut(&self) -> &mut T {
        &mut (&mut *self.0).value
    }
}

enum Status {
    Unreachable,
    Reachable,
}

impl<T> Manager<T>
where
    T: Keep,
{
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub fn manage(&mut self, value: T) -> Value<T> {
        let slot = Slot {
            value,
            status: Status::Reachable, // doesn't matter
        };
        let managed = Box::into_raw(Box::new(slot));
        self.slots.push(managed);
        Value(managed)
    }

    pub fn collect(&mut self, entry: Value<T>) {
        for slot in self.slots.iter_mut() {
            unsafe {
                (&mut **slot).status = Status::Unreachable;
            }
        }

        let mut stack = Vec::new();
        stack.push(entry.0);
        while let Some(slot) = stack.pop() {
            unsafe {
                let slot = &mut *slot;
                if let Status::Unreachable = slot.status {
                    slot.status = Status::Reachable;
                    for value in slot.value.keep() {
                        stack.push(value.0);
                    }
                }
            }
        }

        let mut alive_slots = Vec::new();
        for slot in self.slots.iter() {
            unsafe {
                if let Status::Reachable = (&mut **slot).status {
                    alive_slots.push(*slot);
                } else {
                    let unmanaged = Box::from_raw(*slot);
                    drop(unmanaged);
                }
            }
        }
        self.slots = alive_slots;
    }

    pub fn managed_len(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Node(Vec<Value<Node>>);

    unsafe impl Keep for Node {
        fn keep(&self) -> &[Value<Self>] {
            &self.0
        }
    }

    #[test]
    fn it_works() {
        let mut manager = Manager::new();
        let v1 = manager.manage(Node(Vec::new()));
        let v2 = manager.manage(Node(vec![v1]));
        let v3 = manager.manage(Node(vec![v1, v2]));
        let v4 = manager.manage(Node(Vec::new()));
        let v5 = manager.manage(Node(vec![v4]));
        unsafe {
            (&mut *v4.0).value.0.push(v5);
        }
        assert_eq!(manager.managed_len(), 5);
        manager.collect(v3);
        assert_eq!(manager.managed_len(), 3);
    }
}
