//

use std::thread;
use std::sync::{Arc, RwLock};

use hulunbuir::{Allocator, Collector, Value, Keep};

struct Node(RwLock<Vec<Value<Node>>>);

unsafe impl Keep for Node {
    fn keep(&self) -> Vec<Value<Self>> {
        self.0.read().unwrap().to_owned()
    }
}

fn main() {
    let coll = Arc::new(Collector::new(Node(RwLock::new(Vec::new())), 128));
    let mut allo = Allocator::new(&coll, 16);
    let val1 = allo.allocate(Node(RwLock::new(Vec::new())));
    let thread_coll = Arc::clone(&coll);
    let handle = thread::spawn(move || {
        let mut allo = Allocator::new(&thread_coll, 16);
        let val2 = allo.allocate(Node(RwLock::new(Vec::new())));
        val1.0.write().unwrap().push(val2);
    });
    handle.join().unwrap();
    assert_eq!(val1.0.read().unwrap().len(), 1);
}
