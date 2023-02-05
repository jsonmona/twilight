use crate::util::CursorState;

pub struct DesktopUpdate<T: ?Sized> {
    pub cursor: Option<CursorState>,
    pub desktop: T,
}
