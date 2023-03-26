use std::cell::RefMut;
use std::ops::{Deref, DerefMut};

pub struct UnwrappedRefMut<'a, T>(RefMut<'a, T>);

impl<T> UnwrappedRefMut<'_, Option<T>> {
    #[allow(clippy::manual_map)]
    pub fn new(inner: RefMut<Option<T>>) -> Option<UnwrappedRefMut<Option<T>>> {
        // That warning does not apply because `inner` is borrowed by `as_ref`
        match inner.as_ref() {
            Some(_) => Some(UnwrappedRefMut(inner)),
            None => None,
        }
    }
}

impl<T> Deref for UnwrappedRefMut<'_, Option<T>> {
    type Target = T;

    fn deref(&self) -> &T {
        // Checked when constructing
        unsafe { self.0.as_ref().unwrap_unchecked() }
    }
}

impl<T> DerefMut for UnwrappedRefMut<'_, Option<T>> {
    fn deref_mut(&mut self) -> &mut T {
        // Checked when constructing
        unsafe { self.0.as_mut().unwrap_unchecked() }
    }
}
