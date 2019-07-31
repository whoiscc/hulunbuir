//

use hulunbuir::{Address, Keep};

pub struct Node(pub Vec<Address>);

unsafe impl Keep for Node {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        f(&self.0)
    }
}
