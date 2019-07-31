//

use std::collections::HashMap;
use std::mem;
use std::sync::{Arc, Mutex};
use std::thread;

use hulunbuir::{
    slot::{Slot, Take},
    Address, Collector, Keep,
};

use rand::{thread_rng, Rng};

struct Node {
    children: Vec<Address>,
    locked: HashMap<Address, usize>,
}

impl Keep for Node {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        let union: Vec<_> = self
            .children
            .iter()
            .chain(self.locked.keys())
            .cloned()
            .collect();
        f(&union)
    }
}

impl Node {
    fn new() -> Self {
        Node {
            children: Vec::new(),
            locked: HashMap::new(),
        }
    }

    fn lock(&mut self, address: &Address) {
        if self.locked.contains_key(address) {
            *self.locked.get_mut(address).unwrap() += 1;
        } else {
            self.locked.insert(address.clone(), 1);
        }
    }

    fn unlock(&mut self, address: &Address) {
        // println!("unlocking");
        *self.locked.get_mut(address).unwrap() -= 1;
        if *self.locked.get(address).unwrap() == 0 {
            // println!("removing");
            self.locked.remove(address);
        }
    }
}

fn wait(collector: &Mutex<Collector<Slot<Node>>>, address: &Address) -> Node {
    loop {
        let result = collector.lock().unwrap().take(address).unwrap();
        match result {
            Take::Free(node) => return node,
            Take::Busy(parker) => parker.park(),
        }
    }
}

fn main() {
    let collector = Arc::new(Mutex::new(Collector::new(4096)));
    let root = collector
        .lock()
        .unwrap()
        .allocate(Slot::new(Node::new()))
        .unwrap();
    collector.lock().unwrap().set_root(root.clone());
    let mut handle: [Option<thread::JoinHandle<()>>; 10] = Default::default();
    for i in 0..10 {
        let thread_collector = Arc::clone(&collector);
        let thread_root = root.clone();
        handle[i] = Some(thread::spawn(move || {
            let mut rng = thread_rng();
            let collector = thread_collector;
            let root = thread_root;
            for _j in 0..16384 {
                let mut current = root.clone();
                let mut node;
                let mut node_stack = Vec::new();
                loop {
                    // println!("start loop");
                    node = wait(&collector, &current);
                    let stop = node.children.is_empty() || rng.gen::<f64>() < 0.05;
                    if stop {
                        // current node is still used outside loop block
                        // so it is not filled before breaking
                        // go to hell RAII
                        break;
                    }
                    let child_index = rng.gen_range(0, node.children.len());
                    let next_current = node.children[child_index].to_owned();
                    node.lock(&next_current);
                    collector.lock().unwrap().fill(&current, node).unwrap();
                    node_stack.push(current.clone());
                    current = next_current;
                }
                let replaced_child = rng.gen_range(0, 100);
                // mutex lock is saved for reusing here
                // otherwise, other thread may trigger a collecting between
                // allocation of new object and filling its parent
                // which will collect the new object immediately
                let mut new_child_write = collector.lock().unwrap();
                let new_child = new_child_write.allocate(Slot::new(Node::new())).unwrap();
                if node.children.len() <= replaced_child {
                    node.children.push(new_child);
                } else {
                    node.children[replaced_child] = new_child;
                }
                new_child_write.fill(&current, node).unwrap();
                mem::drop(new_child_write);
                while let Some(parent) = node_stack.pop() {
                    let mut node = wait(&collector, &parent);
                    node.unlock(&current);
                    collector.lock().unwrap().fill(&parent, node).unwrap();
                    current = parent;
                }
            }
        }));
    }
    for i in 0..10 {
        handle[i].take().unwrap().join().unwrap();
    }
}
