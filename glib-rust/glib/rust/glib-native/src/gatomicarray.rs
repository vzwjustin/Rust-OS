//! `gatomicarray.c` compatibility helper.

use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Default)]
pub struct AtomicArray<T: Clone> {
    values: Mutex<Vec<T>>,
}

impl<T: Clone> AtomicArray<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: Mutex::new(Vec::new()),
        }
    }

    pub fn replace(&self, values: Vec<T>) {
        *self.values.lock() = values;
    }

    pub fn push(&self, value: T) {
        self.values.lock().push(value);
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<T> {
        self.values.lock().clone()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::AtomicArray;

    #[test]
    fn snapshots_values() {
        let array = AtomicArray::new();
        array.push(1);
        array.push(2);
        assert_eq!(array.snapshot(), [1, 2]);
    }
}
