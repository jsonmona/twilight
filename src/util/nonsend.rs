use std::marker::PhantomData;

#[derive(Debug, Default, Clone)]
pub struct NonSend(PhantomData<*const ()>);

impl NonSend {
    pub fn new() -> Self {
        Default::default()
    }
}
