use crate::image::ImageBuf;

#[derive(Clone, Debug)]
pub struct CursorState {
    pub visible: bool,
    pub pos_x: u32,
    pub pos_y: u32,
    pub shape: Option<CursorShape>,
}

/**
If xor is true, this cursor is considered to be a BGRA-XOR cursor. When the
alpha value is 0xFF, the RGB value should replace the screen pixel. When the
alpha value is 0, an XOR operation is performed on the RGB value and the
screen pixel; the result replaces the screen pixel. Anything in between is UB.

It's just DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR with alpha inverted.
 */
#[derive(Clone, Debug)]
pub struct CursorShape {
    pub image: ImageBuf,
    pub xor: bool,
    pub hotspot_x: f32,
    pub hotspot_y: f32,
}
