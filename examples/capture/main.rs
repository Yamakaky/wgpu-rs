/// This example shows how to capture an image by rendering it to a texture, copying the texture to
/// a buffer, and retrieving it from the buffer. This could be used for "taking a screenshot," with
/// the added benefit that this method doesn't require a window to be created.
use std::{mem::size_of, time::Duration};

async fn run() {
    let adapter = wgpu::Instance::new()
        .request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: None,
            },
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: false,
                },
                limits: wgpu::Limits::default(),
            },
            None,
        )
        .await
        .unwrap();

    let device = std::sync::Arc::new(device);
    std::thread::spawn({
        let weak = std::sync::Arc::downgrade(&device);
        move || {
            while let Some(device) = weak.upgrade() {
                device.poll(wgpu::Maintain::Wait);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });

    // Rendered image is 256×256 with 32-bit RGBA color
    let size = 256u32;

    let texture_extent = wgpu::Extent3d {
        width: size,
        height: size,
        depth: 1,
    };

    // The render pipeline renders data into this texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
        label: None,
    });

    let (staging_depth_buffer_send, staging_depth_buffer_recv) = async_channel::unbounded();
    for _ in 0..1 {
        staging_depth_buffer_send
            .send(device.create_buffer(&wgpu::BufferDescriptor {
                size: (size * size) as u64 * size_of::<u32>() as u64,
                usage: wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_DST,
                label: None,
            }))
            .await
            .unwrap();
    }

    loop {
        let output_buffer = staging_depth_buffer_recv.try_recv();
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &texture.create_default_view(),
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::RED,
            }],
            depth_stencil_attachment: None,
        });

        if let Ok(ref output_buffer) = output_buffer {
            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                wgpu::TextureCopyView {
                    texture: &texture,
                    mip_level: 0,
                    array_layer: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                wgpu::BufferCopyView {
                    buffer: output_buffer,
                    offset: 0,
                    bytes_per_row: size_of::<u32>() as u32 * size,
                    rows_per_image: 0,
                },
                texture_extent,
            );
        }

        queue.submit(Some(encoder.finish()));

        if let Ok(output_buffer) = output_buffer {
            let sender = staging_depth_buffer_send.clone();
            smol::Task::spawn(async move {
                {
                    output_buffer
                        .map_read(0, (size * size) as u64 * size_of::<u32>() as u64)
                        .await
                        .unwrap();
                }
                sender.send(output_buffer).await.unwrap();
            })
            .detach();
        }
        futures_timer::Delay::new(std::time::Duration::from_millis(1)).await;
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        smol::run(run());
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());
    }
}
