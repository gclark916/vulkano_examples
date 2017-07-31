#[macro_use]
extern crate vulkano;

#[macro_use]
extern crate vulkano_shader_derive;

extern crate image;

use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBuffer;
use vulkano::command_buffer::DynamicState;
use vulkano::device::Device;
use vulkano::device::DeviceExtensions;
use vulkano::format::Format;
use vulkano::framebuffer::Framebuffer;
use vulkano::framebuffer::Subpass;
use vulkano::image::Dimensions;
use vulkano::image::StorageImage;
use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::instance::PhysicalDevice;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::viewport::Viewport;
use vulkano::sync::GpuFuture;

use std::sync::Arc;

fn main() {
    let instance = Instance::new(None, &InstanceExtensions::none(), None)
        .expect("failed to create Vulkan instance");

    let physical_device = PhysicalDevice::enumerate(&instance).next().expect(
        "no device available",
    );

    println!(
        "Using device: {} (type: {:?})",
        physical_device.name(),
        physical_device.ty()
    );

    let queue = physical_device
        .queue_families()
        .find(|&q| q.supports_graphics())
        .expect("couldn't find a graphical queue family");

    let (device, mut queues) = {
        let device_ext = DeviceExtensions::none();

        Device::new(
            physical_device,
            physical_device.supported_features(),
            &device_ext,
            [(queue, 0.5)].iter().cloned(),
        ).expect("failed to create device")
    };

    let queue = queues.next().unwrap();

    let vertex_buffer = {
        #[derive(Debug, Clone)]
        struct Vertex {
            position: [f32; 2],
        }
        impl_vertex!(Vertex, position);

        CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            Some(queue.family()),
            [
                Vertex { position: [-0.5, -0.25] },
                Vertex { position: [0.0, 0.5] },
                Vertex { position: [0.25, -0.1] },
            ].iter()
                .cloned(),
        ).expect("failed to create buffer")
    };

    mod vs {
        #[derive(VulkanoShader)]
        #[ty = "vertex"]
        #[src = "
#version 450
layout(location = 0) in vec2 position;
void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}
"]
        struct Dummy;
    }

    mod fs {
        #[derive(VulkanoShader)]
        #[ty = "fragment"]
        #[src = "
#version 450
layout(location = 0) out vec4 f_color;
void main() {
    f_color = vec4(1.0, 0.0, 0.0, 1.0);
}
"]
        struct Dummy;
    }

    let vs = vs::Shader::load(device.clone()).expect("failed to create shader module");
    let fs = fs::Shader::load(device.clone()).expect("failed to create shader module");

    let render_pass = Arc::new(
        single_pass_renderpass!(device.clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store,
                format: Format::R8G8B8A8Unorm,
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    ).unwrap(),
    );

    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input_single_buffer()
            .vertex_shader(vs.main_entry_point(), ())
            .triangle_list()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(fs.main_entry_point(), ())
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            .build(device.clone())
            .unwrap(),
    );

    let dimensions = Dimensions::Dim2d {
        width: 1024,
        height: 1024,
    };
    let image = StorageImage::new(
        device.clone(),
        dimensions,
        Format::R8G8B8A8Unorm,
        Some(queue.family()),
    ).unwrap();
    //let image = AttachmentImage::new(device.clone(), [dimensions.width(), dimensions.height()],
    //                                                 Format::R8G8B8A8Unorm).unwrap();

    let framebuffer = Framebuffer::start(render_pass.clone())
        .add(image.clone())
        .unwrap()
        .build()
        .unwrap();

    let mut previous_frame_end = Box::new(vulkano::sync::now(device.clone())) as Box<GpuFuture>;

    previous_frame_end.cleanup_finished();

    let buf = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::all(),
        Some(queue.family()),
        (0..1024 * 1024 * 4).map(|_| 0u8),
    ).expect("failed to create buffer");

    let command_buffer = AutoCommandBufferBuilder::new(device.clone(), queue.family())
        .unwrap()
        .begin_render_pass(framebuffer, false, vec![[0.0, 0.0, 1.0, 1.0].into()])
        .unwrap()
        .draw(
            pipeline.clone(),
            DynamicState {
                line_width: None,
                // TODO: Find a way to do this without having to dynamically allocate a Vec
                // every frame.
                viewports: Some(vec![
                    Viewport {
                        origin: [0.0, 0.0],
                        dimensions: [dimensions.width() as f32, dimensions.height() as f32],
                        depth_range: 0.0..1.0,
                    },
                ]),
                scissors: None,
            },
            vertex_buffer.clone(),
            (),
            (),
        )
        .unwrap()
        .end_render_pass()
        .unwrap()
        .copy_image_to_buffer(image.clone(), buf.clone())
        .unwrap()
        .build()
        .unwrap();

    let finished = CommandBuffer::execute(command_buffer, queue.clone()).unwrap();

    GpuFuture::then_signal_fence_and_flush(finished)
        .unwrap()
        .wait(None)
        .unwrap();

    let buffer_content = buf.read().unwrap();
    let output_image =
        image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(1024, 1024, &buffer_content[..])
            .unwrap();

    output_image.save("output.png").unwrap();

    return;
}
