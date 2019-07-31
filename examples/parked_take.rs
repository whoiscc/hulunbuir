//

use std::thread;
use std::sync::{Mutex, Arc};
use std::time::Duration;

use hulunbuir::{Collector, slot::Slot};

mod common;
use common::Node;

fn main() {
    let collector = Arc::new(Mutex::new(Collector::new(128)));
    let slot = collector.lock().unwrap().allocate(Slot::new(Node(Vec::new())));
    let take_one = collector.lock().unwrap().take(&slot);
    assert!(take_one.is_ok());
    println!("main thread is taking the slot");
    let thread_collector = Arc::clone(&collector);
    let thread_slot = slot.clone();
    let handle = thread::spawn(move || {
        let collector = thread_collector;
        let slot = thread_slot;
        loop {
            let result = collector.lock().unwrap().take(&slot);
            match result {
                Ok(mut node) => {
                    println!("child thread is taking the slot");
                    node.0.push(slot.clone());
                    collector.lock().unwrap().fill(&slot, node);
                    break
                },
                Err(parker) => {
                    println!("child thread is parking for slot");
                    parker.park();
                },
            }
        }
    });
    thread::sleep(Duration::from_millis(10));
    println!("main thread will fill the slot");
    collector.lock().unwrap().fill(&slot, take_one.unwrap());
    handle.join().unwrap();
}