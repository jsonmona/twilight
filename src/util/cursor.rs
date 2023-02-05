use crate::image::ImageBuf;

#[derive(Clone, Debug)]
pub struct CursorState {
    pub visible: bool,
    pub pos_x: u32,
    pub pos_y: u32,
    pub shape: Option<CursorShape>,
}

#[derive(Clone, Debug)]
pub struct CursorShape {
    pub image: ImageBuf,
    pub hotspot_x: f32,
    pub hotspot_y: f32,
}
