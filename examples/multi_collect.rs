//

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use hulunbuir::{Allocator, Collector, Keep, Value};

struct Node(Mutex<VecDeque<Value<Node>>>);

unsafe impl Keep for Node {
    fn keep(&self) -> Vec<Value<Self>> {
        let owned = self.0.lock().unwrap().to_owned();
        owned.into_iter().collect()
    }
}

impl Node {
    fn new() -> Self {
        Node(Mutex::new(VecDeque::new()))
    }
}

fn main() {
    let coll = Arc::new(Collector::new(Node::new(), 128));
    let thread_coll = Arc::clone(&coll);
    let handle = thread::spawn(move || {
        let mut allo = Allocator::new(&thread_coll, 16);
        allo.entry()
            .0
            .lock()
            .unwrap()
            .push_back(allo.allocate(Node::new()));
        allo.allocate(Node::new());
    });
    handle.join().unwrap();
    assert_eq!(coll.slot_len(), 3);
    coll.collect();
    assert_eq!(coll.slot_len(), 2);
}
