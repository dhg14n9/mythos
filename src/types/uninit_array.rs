use std::mem::MaybeUninit;

pub struct UninitArray<T: Copy, const N: usize> {
    array: [MaybeUninit<T>; N],
    length: usize,
}

impl<T: Copy, const N: usize> UninitArray<T, N> {
    pub fn new() -> Self {
        Self {
            array: [MaybeUninit::uninit(); N],
            length: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        debug_assert!(self.length < N);

        self.array[self.length].write(value);
        self.length += 1;
    }

    pub fn pop(&mut self) -> T {
        debug_assert!(self.length > 0);

        self.length -= 1;
        unsafe { self.array[self.length].assume_init() }
    }

    pub fn read(&self, index: usize) -> T {
        debug_assert!(index < self.length);

        unsafe { self.array[index].assume_init() }
    }

    pub fn read_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(index < self.length);

        unsafe { self.array[index].assume_init_mut() }
    }

    pub fn swap(&mut self, i: usize, j: usize) {
        debug_assert!(i < self.length && j < self.length);

        self.array.swap(i, j);
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn clear(&mut self) {
        self.length = 0;
    }
}

impl<T: Copy, const N: usize> Clone for UninitArray<T, N> {
    fn clone(&self) -> Self {
        Self {
            array: self.array,
            length: self.length,
        }
    }
}
