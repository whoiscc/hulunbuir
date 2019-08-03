//! 
//! First of all, the "slot" below is not the same as the slot I mentioned in main module.
//! Theoretically, instances of any type which implements `Keep` trait could be inserted
//! into the slots of a collector, and the `Slot<T>` type provided by this module is only one
//! of them. However, there's real benefit to use it instead of some random types.
//!
//! # Multithreading in action
//!
//! Suppose you are migrating the list type from main module's example into multithreading
//! environment. You need to handle at least two problems properly to prevent everything
//! from crashing:
//! * What if one thread triggers garbage collection when some of the objects are replaced
//! with some fake ones by other threads?
//! * What if one thread tries to replace out a managed object, which is currently replaced
//! out by another thread?
//!
//! Fortunately, you can solve both the problems by replacing `Collector<T>` with
//! `Collector<Slot<T>>`, and use `take` and `fill` methods instead of `replace`.
//!
//! # First sight in `Slot`
//!
//! We could rewrite the example in main module with `Slot` like this:
//!
//! ```rust
//! use hulunbuir::{Address, Collector, Keep};
//! use hulunbuir::slot::{Slot, Take};
//!
//! // exactly same type as before
//! struct ListNode(i32, Option<Address>);
//!
//! impl Keep for ListNode {
//!     fn with_keep<F: FnMut(&[Address])>(&self, mut keep: F) {
//!         if let Some(tail) = self.1.to_owned() {
//!             keep(&[tail])
//!         }
//!     }
//! }
//!
//! fn main() {
//!     let mut collector = Collector::new(128);
//!     // allocate a Slot<ListNode> instead of ListNode
//!     let root = collector.allocate(Slot::new(ListNode(0, None))).unwrap();
//!     collector.set_root(root.clone());
//!     let tail = collector.allocate(Slot::new(ListNode(1, None))).unwrap();
//!     // take root object out of slot, and leave a "hole" there
//!     let mut root_node = match collector.take(&root).unwrap() {
//!         Take::Free(object) => object,
//!         Take::Busy(_) => unreachable!(),  // we know that no one is using it
//!     };
//!     root_node.1 = Some(tail);
//!     // fill the hole with updated object
//!     collector.fill(&root, root_node).unwrap();
//!
//!     // the rest part is the same as before
//!     let _orphan = collector.allocate(Slot::new(ListNode(2, None))).unwrap();
//!     assert_eq!(collector.alive_count(), 3);
//!     collector.collect();
//!     assert_eq!(collector.alive_count(), 2);
//! }
//!```
//!
//! By taking object out of slot, `Slot` automatically:
//! * Provides a "hole" object to prevent the following threads taking it, or, to make them
//! realizing that some other one is taking it
//! * Calls `Keep::with_keep` method of the object before giving it out, and caches the
//! result in the hole. So the hole could "pretend" to be the taken object if garbage
//! collection happens.
//!
//! Please pay extra attention to the second function. It means **collector will not be aware of
//! any change to the kept list of taken object until it is filled back**. So, if you are doing
//! something like this:
//! 1. lock the collector, allocate a new object, unlock it
//! 2. (lock) take an object out (unlock) and make it keeping the new object
//! 3. lock the collector, fill the object, unlock it
//!
//! Then you will get chance to lose your new object unexpectedly, if some other thread
//! triggers a garbage collection while your thread is in the second stage. The correct
//! way is to hold the lock through all three stages.
//!
//! # Blocking on taking
//!
//! In most of the time, when we trying to take an object out but someone else is using it,
//! all we want to do is just waiting. However, the `take` method returns immediately, to
//! prevent current thread holding the global collector too long. In addition to trying again
//! and again as a spin lock, you can leverage on the other variant of `Take`:
//!
//! ```rust
//! # use std::sync::Mutex;
//! # use hulunbuir::{Address, Collector, Keep};
//! # use hulunbuir::slot::{Slot, Take};
//!
//! fn wait<T: Keep>(collector: &Mutex<Collector<Slot<T>>>, address: &Address) -> T {
//!     loop {
//!         let take = collector.lock().unwrap().take(address).unwrap();
//!         match take {
//!             Take::Free(value) => return value,
//!             Take::Busy(parker) => parker.park(),
//!         }
//!     }
//! }
//!
//! # fn main() {}
//! ```
//!
//! The `parker` is a [`crossbeam::sync::Parker`][1]. By calling its `park` method, current
//! thread will be blocked until the paired `Unparker::unpark` is called, which will be done
//! by `Slot::fill`. Notice that it's not trivial to extract `take` variable out of `match`
//! block, so that the mutex could be released before current thread is parked which will
//! become a dead lock.
//!
//! [1]: https://docs.rs/crossbeam/0.7.2/crossbeam/sync/struct.Parker.html
//!
//! The `wait` function above may be idiomatic, but I cannot find a way to provide it because
//! I have no idea what kind of mutex you prefer.
//!
//! # Disadvantage on using `Slot`
//!
//! The first disadvantage is that you cannot concurrent read an object in an obvious way.
//! Certainly you can absolutely perform concurrent reading with something like
//!
//! > `Arc<Mutex<Collector<Slot<Arc<RwLock<T>>>>>>`
//!
//! As we all know it turns out that Rust is all about adding another layer.
//!
//! The second disadvantage, which is absolutely not limited to `Slot`, is that objects must
//! be moved back and forth again and again which may hurt performance seriously. This can also
//! be prevented by adding a `Box` layer (what I just say?). At the very end Hulunbuir does not
//! concern much about memory location right now. Maybe some day I will write a new add-on
//! like `Slot` for it!
//!

use crate::{Address, Collector, Keep, error::Error};

use crossbeam::sync::{Parker as ParkerPriv, Unparker};

pub type Parker = ParkerPriv;

enum SlotPriv<T> {
    Free(T),
    Busy {
        keep: Vec<Address>,
        unparkers: Vec<Unparker>,
    },
}

/// A managable type which provides some more functionality.
///
/// See module level document for more detail.
pub struct Slot<T>(SlotPriv<T>);

impl<T> Slot<T> {
    /// Create a new slot with `value`.
    pub fn new(value: T) -> Self {
        Self(SlotPriv::Free(value))
    }
}

impl<T: Keep> Keep for Slot<T> {
    fn with_keep<F: FnMut(&[Address])>(&self, mut f: F) {
        match &self.0 {
            SlotPriv::Free(value) => value.with_keep(f),
            SlotPriv::Busy { keep, .. } => f(keep),
        }
    }
}

/// The result of trying to take an object out.
pub enum Take<T> {
    /// The object is not in used.
    Free(T),
    /// The object is currently used by others. You could block current thread until it
    /// is returned by calling `Parker::park`.
    Busy(Parker),
}

impl<T: Keep> Collector<Slot<T>> {
    /// Take the object at `address` out and leave a hole there. `Error::InvalidAddress`
    /// will be thrown if there's no alive object at `address`.
    pub fn take(&mut self, address: &Address) -> Result<Take<T>, Error> {
        let mut keep = Vec::new();
        match &mut self
            .slots
            .get_mut(&address)
            .ok_or(Error::InvalidAddress)?
            .content
            .0
        {
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
}

impl<T> Collector<Slot<T>> {
    /// Fill the hole at `address` with `value`. If the address does not contain a hole of
    /// an alive object, `Error::InvalidAddress` will be thrown. If there is already a not-in-used
    /// object at `address`, then `Error::DuplicatedFilling` will be thrown.
    pub fn fill(&mut self, address: &Address, value: T) -> Result<(), Error> {
        match self.replace(address, Slot(SlotPriv::Free(value)))?.0 {
            SlotPriv::Free(_) => Err(Error::DuplicatedFilling),
            SlotPriv::Busy { unparkers, .. } => {
                for unparker in unparkers {
                    unparker.unpark();
                }
                Ok(())
            }
        }
    }
}
