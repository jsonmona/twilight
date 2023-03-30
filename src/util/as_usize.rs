use cfg_if::cfg_if;

pub trait AsUsize: Copy + PartialEq + PartialOrd {
    /// Convert number into usize, panic if out of range
    fn as_usize(self) -> usize;

    /// Compare against a usize without clipping value
    fn equals_usize(self, rhs: usize) -> bool;
}

impl AsUsize for u8 {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.into()
    }

    #[inline(always)]
    fn equals_usize(self, rhs: usize) -> bool {
        self as usize == rhs
    }
}

impl AsUsize for u16 {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.into()
    }

    #[inline(always)]
    fn equals_usize(self, rhs: usize) -> bool {
        self as usize == rhs
    }
}

impl AsUsize for u32 {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.try_into()
            .unwrap_or_else(|_| panic!("unable to convert {self}u32 into usize"))
    }

    #[inline(always)]
    fn equals_usize(self, rhs: usize) -> bool {
        cfg_if! {
            if #[cfg(target_pointer_width = "16")] {
                self == rhs as u32
            } else {
                self as usize == rhs
            }
        }
    }
}

impl AsUsize for u64 {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.try_into()
            .unwrap_or_else(|_| panic!("unable to convert {self}u64 into usize"))
    }

    #[inline(always)]
    fn equals_usize(self, rhs: usize) -> bool {
        cfg_if! {
            if #[cfg(any(target_pointer_width = "16", target_pointer_width = "32"))] {
                self == rhs as u64
            } else {
                self as usize == rhs
            }
        }
    }
}

impl AsUsize for u128 {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.try_into()
            .unwrap_or_else(|_| panic!("unable to convert {self}u128 into usize"))
    }

    #[inline(always)]
    fn equals_usize(self, rhs: usize) -> bool {
        cfg_if! {
            if #[cfg(any(target_pointer_width = "16", target_pointer_width = "32", target_pointer_width = "64"))] {
                self == rhs as u128
            } else {
                self as usize == rhs
            }
        }
    }
}
