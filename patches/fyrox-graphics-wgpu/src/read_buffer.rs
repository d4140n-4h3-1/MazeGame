// Copyright (c) 2019-present Dmitry Stepanov and Fyrox Engine contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use crate::server::WgpuGraphicsServer;
use fyrox_graphics::{
    core::math::Rect, error::FrameworkError, framebuffer::GpuFrameBufferTrait,
    gpu_texture::GpuTextureKind, read_buffer::GpuAsyncReadBufferTrait,
};
use std::cell::{Cell, RefCell};
use std::rc::Weak;

/// Wgpu implementation of [`GpuAsyncReadBufferTrait`].
///
/// Provides asynchronous pixel readback from a framebuffer's color attachment to
/// CPU-accessible memory. Internally uses a `MAP_READ | COPY_DST` buffer and
/// `copy_texture_to_buffer` to transfer pixel data.
///
/// # Usage
///
/// 1. Create via [`GraphicsServer::create_async_read_buffer`](fyrox_graphics::server::GraphicsServer::create_async_read_buffer)
/// 2. Call [`schedule_pixels_transfer`](Self::schedule_pixels_transfer) to start the transfer
/// 3. Poll [`try_read`](Self::try_read) each frame until the data is ready
pub struct WgpuAsyncReadBuffer {
    server: Weak<WgpuGraphicsServer>,
    /// Grows when a transfer needs more room than the pixels themselves, because copied rows are
    /// padded to [`wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`].
    buffer: RefCell<wgpu::Buffer>,
    _pixel_count: usize,
    pixel_size: usize,
    request_pending: Cell<bool>,
    size_bytes: usize,
    /// Layout of the scheduled transfer: row count, unpadded and padded row length in bytes.
    rows: Cell<(usize, usize, usize)>,
}

impl WgpuAsyncReadBuffer {
    /// Creates a new async read buffer with enough capacity for `pixel_count` pixels
    /// of `pixel_size` bytes each.
    pub fn new(
        server: &WgpuGraphicsServer,
        name: &str,
        pixel_size: usize,
        pixel_count: usize,
    ) -> Result<Self, FrameworkError> {
        let size_bytes = pixel_count * pixel_size;
        let buffer = server.state.device.create_buffer(&wgpu::BufferDescriptor {
            label: if server.named_objects {
                Some(name)
            } else {
                None
            },
            size: size_bytes.max(1) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            server: server.weak_ref(),
            buffer: RefCell::new(buffer),
            _pixel_count: pixel_count,
            pixel_size,
            request_pending: Cell::new(false),
            size_bytes,
            rows: Cell::new((0, 0, 0)),
        })
    }
}

impl GpuAsyncReadBufferTrait for WgpuAsyncReadBuffer {
    fn schedule_pixels_transfer(
        &self,
        framebuffer: &dyn GpuFrameBufferTrait,
        color_buffer_index: u32,
        _rect: Option<Rect<i32>>,
    ) -> Result<(), FrameworkError> {
        if self.request_pending.get() {
            return Ok(());
        }
        let Some(server) = self.server.upgrade() else {
            return Err(FrameworkError::GraphicsServerUnavailable);
        };
        let color_attachment = framebuffer
            .color_attachments()
            .get(color_buffer_index as usize)
            .ok_or_else(|| FrameworkError::Custom("No color attachment".into()))?;
        let wgpu_tex = color_attachment
            .texture
            .as_any()
            .downcast_ref::<crate::texture::WgpuTexture>()
            .ok_or_else(|| FrameworkError::Custom("Expected WgpuTexture".into()))?;
        let (w, h) = match color_attachment.texture.kind() {
            GpuTextureKind::Rectangle { width, height } => (width, height),
            _ => return Err(FrameworkError::Custom("Only rectangular textures".into())),
        };
        let bpp = self.pixel_size;

        // The copy must see everything drawn before it, so send those draws first.
        server.submit_pending();

        let row = w * bpp;
        let padded_row = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let needed = if h == 0 {
            0
        } else {
            padded_row * (h - 1) + row
        };
        if needed as u64 > self.buffer.borrow().size() {
            *self.buffer.borrow_mut() =
                server.state.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: needed as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
        }
        self.rows.set((h, row, padded_row));
        let buffer = self.buffer.borrow();

        let mut encoder = server
            .state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: wgpu_tex.wgpu_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row as u32),
                    rows_per_image: Some(h as u32),
                },
            },
            wgpu::Extent3d {
                width: w as u32,
                height: h as u32,
                depth_or_array_layers: 1,
            },
        );
        server.state.queue.submit(std::iter::once(encoder.finish()));
        self.request_pending.set(true);
        Ok(())
    }

    fn is_request_running(&self) -> bool {
        self.request_pending.get()
    }

    fn try_read(&self) -> Option<Vec<u8>> {
        if !self.request_pending.get() {
            return None;
        }
        let server = self.server.upgrade()?;
        let buffer = self.buffer.borrow();
        let (rows, row, padded_row) = self.rows.get();
        let copied = if rows == 0 {
            0
        } else {
            padded_row * (rows - 1) + row
        };
        let slice = buffer.slice(..copied.max(1) as u64);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).ok();
        });
        server
            .state
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();
        match rx.recv() {
            Ok(Ok(())) => {
                if let Ok(mapped) = slice.get_mapped_range() {
                    let mut result = vec![0u8; self.size_bytes];
                    for y in 0..rows {
                        let src = y * padded_row;
                        let dst = y * row;
                        let len = row.min(self.size_bytes.saturating_sub(dst));
                        result[dst..dst + len].copy_from_slice(&mapped[src..src + len]);
                    }
                    drop(mapped);
                    buffer.unmap();
                    self.request_pending.set(false);
                    Some(result)
                } else {
                    self.request_pending.set(false);
                    None
                }
            }
            _ => {
                self.request_pending.set(false);
                None
            }
        }
    }
}
