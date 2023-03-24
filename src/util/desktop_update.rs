use crate::util::CursorState;

#[derive(Debug)]
pub struct DesktopUpdate<T: ?Sized> {
    pub cursor: Option<CursorState>,
    pub desktop: T,
}
