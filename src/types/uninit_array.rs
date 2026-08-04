use std::mem::MaybeUninit;


pub struct UninitArray<T: Copy, const N: usize> {
    array: [MaybeUninit<T>; N],
    length: usize,
    back: usize
}

impl<T: Copy, const N: usize> UninitArray<T, N> {
    pub fn new() -> Self {
        Self {
            array: [MaybeUninit::uninit(); N],
            length: 0,
            back: N
        }
    }

    // is `index` inside one of the two written regions?
    fn written(&self, index: usize) -> bool {
        index < self.length || index >= self.back
    }

    pub fn push(&mut self, value: T) {
        debug_assert!(self.length < self.back);

        self.array[self.length].write(value);
        self.length += 1;
    }

    pub fn push_back(&mut self, value: T) {
        debug_assert!(self.back > self.length);

        self.back -= 1;
        self.array[self.back].write(value);
    }

    pub fn pop(&mut self) -> T {
        debug_assert!(self.length > 0);

        self.length -= 1;
        unsafe { self.array[self.length].assume_init() }
    }

    pub fn read(&self, index: usize) -> T {
        debug_assert!(self.written(index));

        unsafe { self.array[index].assume_init() }
    }

    pub fn read_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(self.written(index));

        unsafe { self.array[index].assume_init_mut() }
    }

    pub fn swap(&mut self, i: usize, j: usize) {
        debug_assert!(self.written(i) && self.written(j));

        self.array.swap(i, j);
    }

    // number of values at the front
    pub fn len(&self) -> usize {
        self.length
    }

    // index of the first value at the back; N when the back region is empty
    pub fn back(&self) -> usize {
        self.back
    }

    // number of values at the back
    pub fn back_len(&self) -> usize {
        N - self.back
    }

    pub fn clear(&mut self) {
        self.length = 0;
        self.back = N;
    }
}

impl<T: Copy, const N: usize> Clone for UninitArray<T, N> {
    fn clone(&self) -> Self {
        Self {
            array: self.array,
            length: self.length,
            back: self.back,
        }
    }
}
