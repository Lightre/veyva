//! DXGI pointer composition in source-display pixels, before scaling to NV12.
//! Only the clipped pointer rectangle crosses the GPU/CPU boundary. The desktop
//! stays on the GPU; DXGI's borrowed surface is never modified.
use super::*;

const MAX_SHAPE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, PartialEq)]
struct Region {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    shape_x: u32,
    shape_y: u32,
}

fn region(x: i32, y: i32, w: u32, h: u32, sw: u32, sh: u32) -> Option<Region> {
    let left = i64::from(x).max(0);
    let top = i64::from(y).max(0);
    let right = (i64::from(x) + i64::from(w)).min(i64::from(sw));
    let bottom = (i64::from(y) + i64::from(h)).min(i64::from(sh));
    (right > left && bottom > top).then_some(Region {
        left: left as u32,
        top: top as u32,
        width: (right - left).max(0) as u32,
        height: (bottom - top).max(0) as u32,
        shape_x: (left - i64::from(x)) as u32,
        shape_y: (top - i64::from(y)) as u32,
    })
}

fn shape_height(info: &DXGI_OUTDUPL_POINTER_SHAPE_INFO, bytes: usize) -> RResult<u32> {
    let mono = info.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32;
    let color = info.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32
        || info.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32;
    let min_pitch = if mono {
        u64::from(info.Width).div_ceil(8)
    } else {
        u64::from(info.Width) * 4
    };
    if (!mono && !color)
        || info.Width == 0
        || info.Height == 0
        || info.Width > 1024
        || info.Height > if mono { 2048 } else { 1024 }
        || (mono && !info.Height.is_multiple_of(2))
        || u64::from(info.Pitch) < min_pitch
        || u64::from(info.Pitch) * u64::from(info.Height) > bytes as u64
    {
        return Err(RecorderError::Other("Invalid DXGI cursor shape".into()));
    }
    Ok(if mono { info.Height / 2 } else { info.Height })
}

fn blend(shape: &[u8], info: &DXGI_OUTDUPL_POINTER_SHAPE_INFO, area: &Region, pixels: &mut [u8]) {
    let pitch = info.Pitch as usize;
    for row in 0..area.height as usize {
        for col in 0..area.width as usize {
            let x = area.shape_x as usize + col;
            let y = area.shape_y as usize + row;
            let dst = &mut pixels[(row * area.width as usize + col) * 4..][..4];
            match info.Type {
                t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32 => {
                    let bit = 0x80 >> (x % 8);
                    let and = if shape[y * pitch + x / 8] & bit != 0 {
                        255
                    } else {
                        0
                    };
                    let xor = if shape[(y + info.Height as usize / 2) * pitch + x / 8] & bit != 0 {
                        255
                    } else {
                        0
                    };
                    for channel in &mut dst[..3] {
                        *channel = (*channel & and) ^ xor;
                    }
                }
                t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32 => {
                    let src = &shape[y * pitch + x * 4..][..4];
                    for i in 0..3 {
                        dst[i] = if src[3] == 0 { src[i] } else { dst[i] ^ src[i] };
                    }
                }
                _ => {
                    let src = &shape[y * pitch + x * 4..][..4];
                    let alpha = u32::from(src[3]);
                    for i in 0..3 {
                        // DXGI color pointers use straight alpha.
                        dst[i] =
                            ((u32::from(src[i]) * alpha + u32::from(dst[i]) * (255 - alpha) + 127)
                                / 255) as u8;
                    }
                }
            }
            dst[3] = 255;
        }
    }
}

pub(super) struct CursorCompositor {
    enabled: bool,
    visible: bool,
    x: i32,
    y: i32,
    info: DXGI_OUTDUPL_POINTER_SHAPE_INFO,
    shape: Vec<u8>,
    frame: Option<ID3D11Texture2D>,
    staging: Option<(u32, u32, ID3D11Texture2D)>,
    pixels: Vec<u8>,
}

impl CursorCompositor {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            visible: false,
            x: 0,
            y: 0,
            info: Default::default(),
            shape: Vec::new(),
            frame: None,
            staging: None,
            pixels: Vec::new(),
        }
    }

    pub fn convert(
        &mut self,
        gfx: &GraphicsStack,
        converter: &ColorConverter,
        source: &ID3D11Texture2D,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) -> RResult<()> {
        // Invisible means either hidden or already embedded in the desktop.
        // Drawing only a separate visible pointer avoids a duplicate cursor.
        if !self.enabled {
            return converter.convert(source);
        }
        if frame_info.LastMouseUpdateTime != 0 {
            self.visible = frame_info.PointerPosition.Visible.as_bool();
            // DXGI gives output-relative top-left pixels, independent of hotspot
            // and UI DPI. Composition before the existing scaler preserves scale.
            self.x = frame_info.PointerPosition.Position.x;
            self.y = frame_info.PointerPosition.Position.y;
        }
        unsafe {
            if frame_info.PointerShapeBufferSize != 0 {
                let bytes = frame_info.PointerShapeBufferSize as usize;
                if bytes > MAX_SHAPE_BYTES {
                    return Err(RecorderError::Other(
                        "DXGI cursor shape exceeds the bounded buffer".into(),
                    ));
                }
                self.shape.resize(bytes, 0);
                let mut required = 0;
                gfx.duplication
                    .GetFramePointerShape(
                        bytes as u32,
                        self.shape.as_mut_ptr().cast(),
                        &mut required,
                        &mut self.info,
                    )
                    .map_err(hr)?;
                if required as usize > bytes {
                    return Err(RecorderError::Other(
                        "DXGI cursor shape exceeds the supplied buffer".into(),
                    ));
                }
                self.shape.truncate(required as usize);
                shape_height(&self.info, self.shape.len())?;
            }
            if !self.visible || self.shape.is_empty() {
                return converter.convert(source);
            }
            let height = shape_height(&self.info, self.shape.len())?;
            let Some(area) = region(
                self.x,
                self.y,
                self.info.Width,
                height,
                gfx.src_width,
                gfx.src_height,
            ) else {
                return converter.convert(source);
            };
            if self.frame.is_none() {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: gfx.src_width,
                    Height: gfx.src_height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                    ..Default::default()
                };
                gfx.device
                    .CreateTexture2D(&desc, None, Some(&mut self.frame))
                    .map_err(hr)?;
            }
            if self
                .staging
                .as_ref()
                .is_none_or(|(w, h, _)| *w != area.width || *h != area.height)
            {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: area.width,
                    Height: area.height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..Default::default()
                };
                let mut texture = None;
                gfx.device
                    .CreateTexture2D(&desc, None, Some(&mut texture))
                    .map_err(hr)?;
                self.staging = Some((
                    area.width,
                    area.height,
                    texture.ok_or_else(|| {
                        RecorderError::Other("Missing cursor staging texture".into())
                    })?,
                ));
            }
            let frame = self
                .frame
                .as_ref()
                .ok_or_else(|| RecorderError::Other("Missing cursor composition texture".into()))?;
            let staging = &self.staging.as_ref().unwrap().2;
            let bounds = D3D11_BOX {
                left: area.left,
                top: area.top,
                right: area.left + area.width,
                bottom: area.top + area.height,
                front: 0,
                back: 1,
            };
            gfx.context.CopyResource(frame, source);
            gfx.context
                .CopySubresourceRegion(staging, 0, 0, 0, 0, source, 0, Some(&bounds));
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gfx.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(hr)?;
            let row_bytes = area.width as usize * 4;
            self.pixels.resize(row_bytes * area.height as usize, 0);
            for row in 0..area.height as usize {
                std::ptr::copy_nonoverlapping(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    self.pixels.as_mut_ptr().add(row * row_bytes),
                    row_bytes,
                );
            }
            gfx.context.Unmap(staging, 0);
            blend(&self.shape, &self.info, &area, &mut self.pixels);
            gfx.context.UpdateSubresource(
                frame,
                0,
                Some(&bounds),
                self.pixels.as_ptr().cast(),
                row_bytes as u32,
                0,
            );
            converter.convert(frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info(
        kind: DXGI_OUTDUPL_POINTER_SHAPE_TYPE,
        width: u32,
        height: u32,
        pitch: u32,
    ) -> DXGI_OUTDUPL_POINTER_SHAPE_INFO {
        DXGI_OUTDUPL_POINTER_SHAPE_INFO {
            Type: kind.0 as u32,
            Width: width,
            Height: height,
            Pitch: pitch,
            ..Default::default()
        }
    }
    #[test]
    fn clips_in_source_pixels_without_hotspot_adjustment() {
        assert_eq!(
            region(-2, -1, 4, 3, 100, 100),
            Some(Region {
                left: 0,
                top: 0,
                width: 2,
                height: 2,
                shape_x: 2,
                shape_y: 1
            })
        );
        assert_eq!(region(99, 99, 4, 3, 100, 100).unwrap().width, 1);
        assert!(region(100, 1, 4, 3, 100, 100).is_none());
    }
    #[test]
    fn monochrome_and_xor_cover_all_four_operations() {
        let info = info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, 4, 2, 1);
        let mut pixels = [0x55; 16];
        blend(
            &[0b00110000, 0b01010000],
            &info,
            &region(0, 0, 4, 1, 4, 1).unwrap(),
            &mut pixels,
        );
        assert_eq!(
            [pixels[0], pixels[4], pixels[8], pixels[12]],
            [0, 255, 0x55, 0xaa]
        );
    }
    #[test]
    fn color_alpha_and_masked_color_are_distinct() {
        let area = region(0, 0, 2, 1, 2, 1).unwrap();
        let mut pixels = [40, 80, 120, 255, 40, 80, 120, 255];
        blend(
            &[100, 100, 100, 0, 100, 100, 100, 128],
            &info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, 2, 1, 8),
            &area,
            &mut pixels,
        );
        assert_eq!(pixels, [40, 80, 120, 255, 70, 90, 110, 255]);
        blend(
            &[1, 2, 3, 0, 255, 255, 255, 255],
            &info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, 2, 1, 8),
            &area,
            &mut pixels,
        );
        assert_eq!(pixels, [1, 2, 3, 255, 185, 165, 145, 255]);
    }
    #[test]
    fn rejects_truncated_or_invalid_shapes() {
        assert!(shape_height(&info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, 2, 2, 8), 15).is_err());
        assert!(shape_height(
            &info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, 8, 3, 1),
            3
        )
        .is_err());
        assert!(shape_height(&info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, 2, 2, 4), 16).is_err());
    }
}
