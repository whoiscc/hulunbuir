//

use std::thread;
use std::sync::{Mutex, Arc};

use hulunbuir::Collector;

mod common;
use common::Node;

fn main() {
    let collector = Arc::new(Mutex::new(Collector::<Node>::new(128)));
    let root = collector.lock().unwrap().allocate(Node(Vec::new()));
    collector.lock().unwrap().set_root(root.clone());
    let thread_collector = Arc::clone(&collector);
    let handle = thread::spawn(move || {
        let collector = thread_collector;
        let kept = collector.lock().unwrap().allocate(Node(Vec::new()));
        collector.lock().unwrap().replace(&root, Node(vec![kept]));
        let orphan = collector.lock().unwrap().allocate(Node(Vec::new()));
        collector.lock().unwrap().replace(&orphan.clone(), Node(vec![orphan]));
    });
    handle.join().unwrap();
    assert_eq!(collector.lock().unwrap().alive_count(), 3);
    collector.lock().unwrap().collect();
    assert_eq!(collector.lock().unwrap().alive_count(), 2);
}