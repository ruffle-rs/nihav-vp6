use std::ops::{Deref, DerefMut};
use std::convert::AsRef;
use std::sync::atomic::*;

struct NABufferData<T> {
    data:       T,
    refs:       AtomicUsize,
}

impl<T> NABufferData<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            refs:       AtomicUsize::new(1),
        }
    }
    fn inc_refs(obj: &mut Self) {
        obj.refs.fetch_add(1, Ordering::SeqCst);
    }
    fn dec_refs(obj: &mut Self) -> bool {
        obj.refs.fetch_sub(1, Ordering::SeqCst) == 0
    }
    fn get_num_refs(obj: &Self) -> usize {
        obj.refs.load(Ordering::Relaxed)
    }
    fn get_read_ptr(obj: &Self) -> &T {
        &obj.data
    }
    fn get_write_ptr(obj: &mut Self) -> Option<&mut T> {
        Some(&mut obj.data)
    }
}

pub struct NABufferRef<T> {
    ptr: *mut NABufferData<T>,
}

impl<T> NABufferRef<T> {
    pub fn new(val: T) -> Self {
        let bdata = NABufferData::new(val);
        let nbox: Box<_> = Box::new(bdata);
        Self { ptr: Box::into_raw(nbox) }
    }
    pub fn get_num_refs(&self) -> usize {
        unsafe {
            NABufferData::get_num_refs(self.ptr.as_mut().unwrap())
        }
    }
    pub fn as_mut(&mut self) -> Option<&mut T> {
        unsafe {
            NABufferData::get_write_ptr(self.ptr.as_mut().unwrap())
        }
    }
}

impl<T> AsRef<T> for NABufferRef<T> {
    fn as_ref(&self) -> &T {
        unsafe {
            NABufferData::get_read_ptr(self.ptr.as_mut().unwrap())
        }
    }
}

impl<T> Deref for NABufferRef<T> {
    type Target = T;
    fn deref(&self) -> &T { self.as_ref() }
}

impl<T> DerefMut for NABufferRef<T> {
    fn deref_mut(&mut self) -> &mut T { self.as_mut().unwrap() }
}

impl<T> Clone for NABufferRef<T> {
    fn clone(&self) -> Self {
        unsafe {
            NABufferData::inc_refs(self.ptr.as_mut().unwrap());
        }
        Self { ptr: self.ptr }
    }
}

impl<T> Drop for NABufferRef<T> {
    fn drop(&mut self) {
        unsafe {
            if NABufferData::dec_refs(self.ptr.as_mut().unwrap()) {
                std::ptr::drop_in_place(self.ptr);
            }
        }
    }
}

impl<T:Default> Default for NABufferRef<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
