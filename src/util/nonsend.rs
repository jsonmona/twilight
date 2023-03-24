use std::marker::PhantomData;

#[derive(Debug, Default, Clone)]
pub struct NonSend(PhantomData<*const usize>);

impl NonSend {
    pub fn new() -> Self {
        Default::default()
    }
}
