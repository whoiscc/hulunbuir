//

use std::thread;
use std::sync::{Mutex, Arc};

use hulunbuir::{Allocator, Collector, Keep, Value};

struct Node(Mutex<Vec<Value<Node>>>);

unsafe impl Keep<Node> for Node {
    fn keep<F>(&self, hold: F) where F: FnOnce(&[Value<Node>]) {
        hold(&self.0.lock().unwrap());
    }
}

impl Node {
    fn new() -> Self {
        Node(Mutex::new(Vec::new()))
    }
}

#[test]
fn cross_thread() {
    let collector = Arc::new(Collector::new(Node::new(), 128));
    let thread_collector = Arc::clone(&collector);
    let handle = thread::spawn(move || {
        Allocator::scoped(&thread_collector, 16, |allocator| {
            let node = allocator.allocate(Node::new());
            allocator.entry().0.lock().unwrap().push(node);
            let orphan = allocator.allocate(Node::new());
            orphan.0.lock().unwrap().push(orphan);
        });
    });
    handle.join().unwrap();
    assert_eq!(collector.slot_len(), 3);
    collector.collect();
    assert_eq!(collector.slot_len(), 2);
}