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
//! 
//! # Basic usage
//! 
//! ```
//! use hulunbuir::{Address, Collector, Keep};
//! 
//! // create a managed type
//! struct ListNode(i32, Option<Address>);
//! 
//! // implement Keep for it, so it could be managed
//! impl Keep for ListNode {
//!     fn with_keep<F: FnOnce(&[Address])>(&self, keep: F) {
//!         // each node keeps only its tail, so call `keep` with it...
//!         if let Some(tail) = self.1.to_owned() {
//!             // ...if the node has tail
//!             keep(&[tail])
//!         }
//!     }
//! }
//! 
//! fn main() {
//!     // create a collector with 128 slots available
//!     let mut collector = Collector::new(128);
//!     let root = collector.allocate(ListNode(0, None)).unwrap();
//!     collector.set_root(root.clone());
//!     let tail = collector.allocate(ListNode(1, None)).unwrap();
//!     // replace root node out with something not important
//!     let mut root_node = collector.replace(&root, ListNode(42, None)).unwrap();
//!     root_node.1 = Some(tail);
//!     // replace root node back
//!     let _ = collector.replace(&root, root_node).unwrap();
//!     
//!     let _orphan = collector.allocate(ListNode(2, None)).unwrap();
//!     // before collecting...
//!     assert_eq!(collector.alive_count(), 3);
//!     collector.collect();
//!     // after collecting...
//!     assert_eq!(collector.alive_count(), 2);
//! }
//! ```
//! 
//! This `replace`-based object updating strategy is suitable for simple single-thread usage.
//! The collector will work correctly **only when no garbage collection happens when any 
//! "real" object is replaced out**, which means, when any of them *is* replaced out:
//! * no explicit calling to `Collector::collect`
//! * no calling to `Collector::allocate`, since it may trigger collection as well if there's
//! no slot available
//! 
//! In multithreading context, none of above could be archieved since each thread has no idea
//! about what the others are doing. So more complicated strategy must be introduced. Hulunbuir
//! provides `slot` module for this purpose, but you are free to develop your own one.

/// Slot-based abstraction for automatic dependency caching and thread parking.
pub mod slot;

use std::collections::HashMap;
use std::mem;
use std::time::Instant;

#[macro_use]
extern crate failure_derive;

use log::info;

/// Memory manager for allocation and garbage collection.
/// 
/// See module level document for basic usage.
#[derive(Debug)]
pub struct Collector<T> {
    slots: HashMap<Address, Slot<T>>,
    slot_max: usize,
    next_id: usize,
    root: Option<Address>,
}

/// Virtual memory address token.
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct Address(usize);

/// Required trait for managed objects' type.
pub trait Keep {
    /// When this method is called, it should calls back `keep` with a slice of addresses,
    /// the objects at which are "kept" by current object. If current object is considered
    /// as alive in a garbage collecting pass (probably since this method is called), then
    /// all the kept objects will also be considered as alive.
    /// 
    /// If this method is not implemented properly, such as not calling `keep` or calling it
    /// with insufficient addresses, `Memory::InvalidAddress` may be thrown in arbitrary time
    /// in the future.
    /// 
    /// There's no reason for this method to fail. Please panic if you have to.
    fn with_keep<F: FnOnce(&[Address])>(&self, keep: F);
}

/// Errors thrown by collector.
#[derive(Debug, Fail)]
pub enum MemoryError {
    /// Alive objects count reaches `slot_max` passed to `Collector::new`, and no object
    /// is collectable.
    #[fail(display = "out of slots")]
    OutOfSlots,
    /// Trying to access object with invalid address.
    #[fail(display = "invalid address")]
    InvalidAddress,
    /// Calling `Collector::fill` on non-empty slot. See document of `slot` module for details.
    #[fail(display = "duplicated filling")]
    DuplicatedFilling,
}

impl<T> Collector<T> {
    /// Create a collector with `slot_max` slots available. Each slot is able to store a managed
    /// object typed `T`.
    pub fn new(slot_max: usize) -> Self {
        Self {
            slots: HashMap::new(),
            slot_max,
            next_id: 0,
            root: None,
        }
    }

    /// Replace the value of object at `address` with `value`. Return the original value of
    /// managed object. If there's no object at `address` (maybe the object there has been
    /// collected), throw `MemoryError::InvalidAddress`.
    pub fn replace(&mut self, address: &Address, value: T) -> Result<T, MemoryError> {
        let slot = self
            .slots
            .get_mut(address)
            .ok_or(MemoryError::InvalidAddress)?;
        let content = mem::replace(&mut slot.content, value);
        Ok(content)
    }

    /// Set object at `address` as root object. Only root object and objects kept by any 
    /// object that has been considered as alive object in the current collecting pass 
    /// will stay alive during garbage collection.
    pub fn set_root(&mut self, address: Address) {
        self.root = Some(address);
    }

    /// Return current root object. If no root object is set, return `None`, and every object
    /// will be collected if a collecting pass is triggered.
    pub fn root(&self) -> &Option<Address> {
        &self.root
    }

    /// Return the total number of managed objects. Some of them may already be dead and will
    /// be collected in the following garbage collection.
    pub fn alive_count(&self) -> usize {
        self.slots.len()
    }
}

#[derive(Debug)]
struct Slot<T> {
    mark: bool,
    content: T,
}

impl<T: Keep> Collector<T> {
    /// Create a new managed object with `value`. If there's no available slot a garbage 
    /// collecting pass will be triggered. If there's still no available slot then
    /// `MemoryError::OutOfSlot` will be thrown. Any error thrown by collecting process
    /// will be re-thrown.
    pub fn allocate(&mut self, value: T) -> Result<Address, MemoryError> {
        if self.slots.len() == self.slot_max {
            self.collect()?;
        }
        if self.slots.len() == self.slot_max {
            return Err(MemoryError::OutOfSlots);
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
        Ok(address)
    }

    /// Clean up all dead objects, which are unreachable from root object, or all objects
    /// if the root object is not set. If root object address is invalid, or any alive object
    /// keeps an object at invalid address, then `Memory::InvalidAddress` will be thrown.
    /// 
    /// This method will be invoked if `Collector::allocate` is called but no slot is available,
    /// but it could also be explicit called by user. Statistics log will be printed after
    /// each collecting pass.
    pub fn collect(&mut self) -> Result<(), MemoryError> {
        let start = Instant::now();

        let mut stack = Vec::new();
        if let Some(address) = &self.root {
            stack.push(address.to_owned());
        }
        while let Some(address) = stack.pop() {
            let slot = self
                .slots
                .get_mut(&address)
                .ok_or(MemoryError::InvalidAddress)?;
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

        info!(
            target: "hulunbuir",
            "garbage collected in {} ms, {:.2}% of available slots used",
            start.elapsed().as_micros() as f32 / 1000.0,
            self.slots.len() as f32 / self.slot_max as f32 * 100.0
        );
        Ok(())
    }
}
