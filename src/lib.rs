#[cfg(test)]
mod tests {
    use crate::HeapVec;
    #[test]
    fn size_ok() {
        use std::mem::size_of;
        assert_eq!(size_of::<*mut u8>(), size_of::<HeapVec<u8>>());
    }

    #[test]
    fn insert_test() {
        let mut hv: crate::HeapVec<u8> = crate::HeapVec::new();
        assert_eq!(hv.len(), 0);
        hv.push(0);
        assert_eq!(hv.len(), 1);

        hv.push(1);
        assert_eq!(hv.len(), 2);


        assert_eq!(hv[0], 0);
        assert_eq!(hv[1], 1);

        assert_eq!(hv.pop(), Some(1));
        assert_eq!(hv.len(), 1);
        assert_eq!(hv.pop(), Some(0));
        assert_eq!(hv.len(), 0);
        assert_eq!(hv.pop(), None)
    }

    #[test]
    #[should_panic(expected = "droppanic.drop()")]
    fn test_drop_panic() {
        // This test should only panic once, and not double panic,
        // which would mean a double drop
        struct DropPanic {
            test: u8
        };
        // TODO: make DropPanic a zero-sized type.

        impl Drop for DropPanic {
            fn drop(&mut self) {
                panic!("droppanic.drop()");

            }
        }

        let mut v = HeapVec::new();
        v.push(DropPanic{test: 0});
    } 
}

use std::marker::PhantomData;
use std::mem;

struct Unique<T> {
    ptr: *const T,              // *const for variance
    _marker: PhantomData<T>,    // For the drop checker
}

// Deriving Send and Sync is safe because we are the Unique owners
// of this data. It's like Unique<T> is "just" T.
unsafe impl<T: Send> Send for Unique<T> {}
unsafe impl<T: Sync> Sync for Unique<T> {}

impl<T> Unique<T> {
    pub fn new(ptr: *mut T) -> Self {
        Unique { ptr: ptr, _marker: PhantomData }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.ptr as *mut T
    }
    
    pub fn is_null(&self) -> bool {
        self.ptr as usize == 0
    }
}

use std::alloc;

pub struct HeapVec<T> {
    ptr: Unique<T>,
}

impl<T> HeapVec<T> {
    pub fn new() -> Self {
        assert!(std::mem::size_of::<T>() != 0, "We're not ready to handle types with size 0!");
        Self { ptr: Unique::new(0 as *mut T)}
    }

    pub fn raw_ptr(&self) -> *const T {
        (self.ptr.as_ptr() as usize + Self::get_offset()) as *mut T
    }

    const fn get_offset() -> usize {
        // Round up sizeof(usize) * 2 to a multiple of alignof(T)
        // * 2 is because there is both len and cap
        // The division is (a + b - 1) / b which is ceiling integer division.
        ((mem::size_of::<usize>()*2 + mem::align_of::<T>() - 1) / mem::align_of::<T>()) * mem::align_of::<T>()
    }

    fn get_offset_of(&self, index: usize) -> *mut T {
        (self.ptr.as_ptr() as usize + Self::get_offset() + mem::size_of::<T>() * index) as *mut T
    }

    fn capacity(&self) -> usize {
        if self.ptr.is_null() {
            0
        } else {
            unsafe {
                *(self.ptr.as_ptr() as *const usize)
            }
        }
    }
    
    pub fn len(&self) -> usize {
        if self.ptr.is_null() {
            0
        } else {
            unsafe {
                *((self.ptr.as_ptr() as usize + mem::size_of::<usize>()) as *const usize)
            }
        }
    }

    fn get_cap_mut(&mut self) -> &mut usize {
        unsafe {
            &mut*(self.ptr.as_ptr() as *mut usize)
        }
    }
    
    fn get_len_mut(&mut self) -> &mut usize {
        unsafe {
            &mut *((self.ptr.as_ptr() as usize + mem::size_of::<usize>()) as *mut usize)
        }
    }

    fn grow(&mut self) {
        unsafe {
            let cap_size = Self::get_offset();
            let elem_size = mem::size_of::<T>();
            let align = std::cmp::max(mem::align_of::<T>(), mem::align_of::<usize>());


            if self.ptr.is_null() {
                let new_num_bytes = cap_size + elem_size;

                let ptr = alloc::alloc(alloc::Layout::from_size_align(new_num_bytes, align).expect("Couldn't create layout!"));

                if ptr.is_null() {
                    panic!("Allocation failed!");
                }

                self.ptr = Unique::new(ptr as *mut T);
                *self.get_cap_mut() = 1;

            } else {
                let old_cap = self.capacity();
                let new_cap = old_cap * 2;
                let old_num_bytes = cap_size + old_cap*elem_size;
                let new_num_bytes = cap_size + 2*old_cap*elem_size;

                let ptr = alloc::realloc(self.ptr.as_ptr() as *mut u8,
                        alloc::Layout::from_size_align(old_num_bytes, align).expect("Couldn't create layout!"),
                        new_num_bytes
                );

                self.ptr = Unique::new(ptr as *mut T);
                *self.get_cap_mut() = new_cap;
            };
        }
    }

    pub fn push(&mut self, elem: T) {
        if self.len() == self.capacity() {
            self.grow();
        }

        unsafe {
            std::ptr::write(self.get_offset_of(self.len()), elem);
        }

        *self.get_len_mut() += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len() == 0 {
            None
        } else {
            *self.get_len_mut() -= 1;
            unsafe {
                Some(std::ptr::read(self.get_offset_of(self.len())))
            }
        }
    }

    pub fn insert(&mut self, index: usize, elem: T) {
        assert!(index <= self.len(), "index out of bounds");
        if self.capacity() == self.len() { self.grow(); }

        unsafe {
            if index < self.len() {
                // ptr::copy(src, dest, len): "copy from source to dest len elems"
                std::ptr::copy(self.get_offset_of(index),
                        self.get_offset_of(index + 1),
                        self.len() - index);
            }
            std::ptr::write(self.get_offset_of(index), elem);
        }
        *self.get_len_mut() += 1;
    }

    pub fn remove(&mut self, index: usize) -> T {
        // Note: `<` because it's *not* valid to remove after everything
        assert!(index < self.len(), "index out of bounds");
        unsafe {
            *self.get_len_mut() -= 1;
            let result = std::ptr::read(self.get_offset_of(index));
            std::ptr::copy(self.get_offset_of(index + 1),
                    self.get_offset_of(index),
                    self.len() - index);
            result
        }
    }
}

impl<T> Drop for HeapVec<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            while let Some(_) = self.pop() { }

            let align = std::cmp::max(mem::align_of::<T>(), mem::align_of::<usize>());
            let elem_size = mem::size_of::<T>();
            let cap_size = Self::get_offset();
            let num_bytes = cap_size + elem_size * self.capacity();
            unsafe {
                alloc::dealloc(self.ptr.as_ptr() as *mut _, alloc::Layout::from_size_align(num_bytes, align).expect("Couldn't create layout!"));
            }
        }
    }
}

use std::ops::Deref;
impl<T> Deref for HeapVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        unsafe {
            std::slice::from_raw_parts(self.get_offset_of(0), self.len())
        }
    }
}
use std::ops::DerefMut;
impl<T> DerefMut for HeapVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(self.get_offset_of(0), self.len())
        }
    }
}


// TODO: implement IntoIter
// TODO: implement Drain
// TODO: support types with size 0
