//! Implements a non-blocking synchronization primitive for lazy-initialized
//! immutable references.

use std::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
    sync::atomic::{AtomicPtr, Ordering},
};

use crossbeam_utils::atomic::AtomicConsume;

/// A non-blocking synchronization primitive (cell) for lazy-initialized
/// immutable references.
///
/// # Examples
///
/// Writing to a `LazyRef` from separate threads:
///
/// ```rust
/// use rayon::prelude::*;
/// use lazy_ref::LazyRef;
///
/// let lazy_ref = LazyRef::new();
/// let thread_ids: Vec<usize> = vec![1, 2, 3];
///
/// thread_ids.par_iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(thread_ids.contains(r));
///        }
///     );
/// let x = lazy_ref.get().unwrap();
/// assert!(thread_ids.contains(x));
/// ```
///
/// `LazyRef` is invariant over the underlying reference, so the following
/// examples wouldn't compile.
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// let lazy_ref = LazyRef::new();
/// let thread_ids: Vec<usize> = vec![1, 2, 3];
///
/// thread_ids.iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(thread_ids.contains(r));
///        }
///     );
/// drop(thread_ids);
/// let x = lazy_ref.get().unwrap();
/// assert_eq!(x, &1);
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// let thread_ids: Vec<usize> = vec![1, 2, 3];
/// let lazy_ref = LazyRef::new();
///
/// thread_ids.iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(thread_ids.contains(r));
///        }
///     );
/// drop(thread_ids);
/// let x = lazy_ref.get().unwrap();
/// assert_eq!(x, &1);
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// static ZERO: usize = 0;
///
/// let lazy_ref = LazyRef::new();
/// let thread_ids: Vec<usize> = vec![1, 2, 3];
///
/// thread_ids.iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(thread_ids.contains(r));
///        }
///     );
/// drop(thread_ids);
/// let x = lazy_ref.get_or_init(|| &ZERO);
/// assert_eq!(x, &1);
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// static ZERO: usize = 0;
///
/// let thread_ids: Vec<usize> = vec![1, 2, 3];
/// let lazy_ref = LazyRef::new();
///
/// thread_ids.iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(thread_ids.contains(r));
///        }
///     );
/// drop(thread_ids);
/// let x = lazy_ref.get_or_init(|| &ZERO);
/// assert_eq!(x, &1);
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// static THREAD_IDS: &[usize] = &[1, 2, 3];
///
/// let lazy_ref = LazyRef::new();
///
/// THREAD_IDS.iter()
///     .for_each(
///         |id| {
///            let r = lazy_ref.get_or_init(|| id);
///            assert!(THREAD_IDS.contains(r));
///        }
///     );
///
/// {
///     let zero = 0;
///     let _ = lazy_ref.get_or_init(|| &zero);
/// };
///
/// let x = lazy_ref.get().unwrap();
/// assert_eq!(x, &1);
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// fn lifetime_invariance<'a: 'b, 'b, T>(value: LazyRef<'a, T>) -> LazyRef<'b, T> {
///     value
/// }
/// ```
///
/// ```compile_fail
/// use lazy_ref::LazyRef;
///
/// fn lifetime_contravariance<'a: 'b, 'b, T>(value: LazyRef<'b, T>) -> LazyRef<'a, T> {
///     value
/// }
/// ```
#[repr(transparent)]
pub struct LazyRef<'a, T> {
    ptr: AtomicPtr<T>,
    _phantom: PhantomData<VarianceMarker<'a, T>>,
}

/// Asserts invariance over `'a`, covariance over `T`.
type VarianceMarker<'a, T> = fn(&'a ()) -> &'a T;

impl<T> Clone for LazyRef<'_, T> {
    #[inline]
    fn clone(&self) -> Self {
        self.get().map(Self::new_initialized).unwrap_or_default()
    }
}

impl<T: PartialEq> PartialEq for LazyRef<'_, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for LazyRef<'_, T> {}

impl<T: Debug> Debug for LazyRef<'_, T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_tuple("LazyRef");
        match self.get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

impl<T> Default for LazyRef<'_, T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> LazyRef<'a, T> {
    /// Creates a new empty cell.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ptr: AtomicPtr::new(std::ptr::null_mut()),
            _phantom: PhantomData,
        }
    }

    /// Creates a new initialized cell.
    #[inline]
    #[must_use]
    pub const fn new_initialized(r: &'a T) -> Self {
        Self {
            ptr: AtomicPtr::new(std::ptr::from_ref(r).cast_mut()),
            _phantom: PhantomData,
        }
    }

    /// Gets the underlying reference.
    ///
    /// Returns `None` if the cell is empty.
    #[inline]
    #[must_use]
    pub fn get(&self) -> Option<&'a T> {
        let ptr = self.ptr.load_consume();
        // SAFETY:
        // This is safe because this pointer can only be created from a valid reference,
        // or it is null.
        unsafe { ptr.as_ref() }
    }

    /// Gets the underlying reference. It doesn't introduce any overhead
    /// compared to the [`get`](Self::get) method, but is only available
    /// through unique access.
    ///
    /// Returns `None` if the cell is empty.
    #[inline]
    #[must_use]
    pub fn get_owned(&mut self) -> Option<&'a T> {
        let ptr = *self.ptr.get_mut();
        // SAFETY:
        // This is safe because this pointer can only be created from a valid reference,
        // or it is null.
        unsafe { ptr.as_ref() }
    }

    /// Sets the contents of this cell to `r`.
    #[inline]
    pub fn set(&self, r: &'a T) {
        self.ptr
            .store(std::ptr::from_ref(r).cast_mut(), Ordering::Release);
    }

    /// Sets the contents of this cell to `r`. It doesn't introduce any overhead
    /// compared to the [`set`](Self::set) method, but is only available
    /// through unique access.
    #[inline]
    pub fn set_owned(&mut self, r: &'a T) {
        *self.ptr.get_mut() = std::ptr::from_ref(r).cast_mut();
    }

    /// Consumes the `LazyRef`, returning the wrapped reference.
    /// Returns `None` if the cell was empty.
    #[inline]
    #[must_use]
    pub fn into_inner(mut self) -> Option<&'a T> {
        self.get_owned()
    }

    /// Gets the underlying reference of the cell, initializing it with `f` if
    /// the cell was empty.
    ///
    /// Many threads may call `get_or_init` concurrently with different
    /// initializing functions. In this case multiple functions can be
    /// executed.
    #[inline]
    #[must_use]
    pub fn get_or_init(&self, f: impl FnOnce() -> &'a T) -> &'a T {
        self.get().unwrap_or_else(|| {
            let r = f();
            self.ptr
                .store(std::ptr::from_ref(r).cast_mut(), Ordering::Release);
            r
        })
    }

    /// Checks whether the cell is initialized.
    #[inline]
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        !self.ptr.load_consume().is_null()
    }
}
