#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
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
        assert!(std::mem::size_of::<T>() >= std::mem::size_of::<usize>(), "We're not ready to handle types smaller than usize!");
        Self { ptr: Unique::new(0 as *mut T)}
    }

    pub fn raw_ptr(&self) -> *const T {
        (self.ptr.as_ptr() as usize + Self::get_offset()) as *mut T
    }

    const fn get_offset() -> usize {
        // Round up sizeof(usize) * 2 to a multiple of alignof(T)
        // * 2 is because there is both len and cap
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
            let align = std::cmp::max(mem::align_of::<T>(), mem::align_of::<usize>());
            let elem_size = mem::size_of::<T>();
            let cap_size = Self::get_offset();


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

