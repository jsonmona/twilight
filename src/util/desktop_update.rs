use crate::util::{CursorState, Timings};

#[derive(Debug, Clone)]
pub struct DesktopUpdate<T: ?Sized> {
    pub cursor: Option<CursorState>,
    pub timings: Timings,
    pub desktop: T,
}

impl<T> DesktopUpdate<T> {
    pub fn split(self) -> (DesktopUpdate<()>, T) {
        (
            DesktopUpdate {
                cursor: self.cursor,
                timings: self.timings,
                desktop: (),
            },
            self.desktop,
        )
    }

    pub fn clone_split(&self) -> (DesktopUpdate<()>, &T) {
        (
            DesktopUpdate {
                cursor: self.cursor.clone(),
                timings: self.timings.clone(),
                desktop: (),
            },
            &self.desktop,
        )
    }

    pub fn with_desktop<R>(self, desktop: R) -> DesktopUpdate<R> {
        DesktopUpdate {
            cursor: self.cursor,
            timings: self.timings,
            desktop,
        }
    }

    pub fn map_desktop<F, R>(self, map_fn: F) -> DesktopUpdate<R>
    where
        F: FnOnce(T) -> R,
    {
        DesktopUpdate {
            cursor: self.cursor,
            timings: self.timings,
            desktop: map_fn(self.desktop),
        }
    }

    pub fn and_then_desktop<F, R, E>(self, map_fn: F) -> Result<DesktopUpdate<R>, E>
    where
        F: FnOnce(T) -> Result<R, E>,
    {
        Ok(DesktopUpdate {
            cursor: self.cursor,
            timings: self.timings,
            desktop: map_fn(self.desktop)?,
        })
    }

    pub fn collapse_from(&mut self, prev: Self) {
        if self.cursor.is_none() {
            self.cursor = prev.cursor;
        } else if prev.cursor.is_some() {
            // merge cursor shape
            let curr_cursor = self.cursor.as_mut().expect("already checked");
            let prev_cursor = prev.cursor.expect("already checked");

            if curr_cursor.shape.is_none() {
                curr_cursor.shape = prev_cursor.shape;
            }
        }
    }

    pub fn collapse_from_iter(&mut self, prev: impl IntoIterator<Item = Self>) {
        let mut cursor = None;
        let mut shape = None;

        for now in prev {
            if let Some(mut c) = now.cursor {
                if let Some(s) = c.shape.take() {
                    shape = Some(s);
                }
                cursor = Some(c);
            }
        }

        if self.cursor.is_none() {
            self.cursor = cursor;
        }

        if let Some(c) = self.cursor.as_mut() {
            if c.shape.is_none() {
                c.shape = shape;
            }
        }
    }
}
