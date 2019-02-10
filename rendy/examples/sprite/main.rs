//!
//! A simple sprite example.
//! This examples shows how to render a sprite on a white background.
//!

use rendy::{
    command::{Family, QueueId, RenderPassEncoder},
    factory::{Config, Factory},
    graph::{
        present::PresentNode,
        render::{Layout, PrepareResult, RenderGroupBuilder, SetLayout, SimpleGraphicsPipeline},
        Graph, GraphBuilder, NodeBuffer, NodeImage,
    },
    memory::MemoryUsageValue,
    mesh::{AsVertex, PosTex},
    resource::buffer::Buffer,
    shader::{Shader, ShaderKind, SourceLanguage, StaticShaderInfo},
    texture::{pixel::Rgba8Srgb, Texture, TextureBuilder},
};

use winit::{EventsLoop, WindowBuilder};

#[cfg(feature = "dx12")]
type Backend = rendy::dx12::Backend;

#[cfg(feature = "metal")]
type Backend = rendy::metal::Backend;

#[cfg(feature = "vulkan")]
type Backend = rendy::vulkan::Backend;

lazy_static::lazy_static! {
    static ref VERTEX: StaticShaderInfo = StaticShaderInfo::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sprite/shader.vert"),
        ShaderKind::Vertex,
        SourceLanguage::GLSL,
        "main",
    );

    static ref FRAGMENT: StaticShaderInfo = StaticShaderInfo::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sprite/shader.frag"),
        ShaderKind::Fragment,
        SourceLanguage::GLSL,
        "main",
    );
}

#[derive(Debug)]
struct SpriteGraphicsPipeline<B: gfx_hal::Backend> {
    texture: Texture<B>,
    vertex: Option<Buffer<B>>,
    descriptor_pool: B::DescriptorPool,
    descriptor_set: B::DescriptorSet,
}

impl<B, T> SimpleGraphicsPipeline<B, T> for SpriteGraphicsPipeline<B>
where
    B: gfx_hal::Backend,
    T: ?Sized,
{
    fn name() -> &'static str {
        "Sprite"
    }

    fn depth() -> bool {
        false
    }

    fn vertices() -> Vec<(
        Vec<gfx_hal::pso::Element<gfx_hal::format::Format>>,
        gfx_hal::pso::ElemStride,
        gfx_hal::pso::InstanceRate,
    )> {
        vec![PosTex::VERTEX.gfx_vertex_input_desc(0)]
    }

    fn load_shader_set<'b>(
        storage: &'b mut Vec<B::ShaderModule>,
        factory: &mut Factory<B>,
        _aux: &mut T,
    ) -> gfx_hal::pso::GraphicsShaderSet<'b, B> {
        storage.clear();

        log::trace!("Load shader module '{:#?}'", *VERTEX);
        storage.push(VERTEX.module(factory).unwrap());

        log::trace!("Load shader module '{:#?}'", *FRAGMENT);
        storage.push(FRAGMENT.module(factory).unwrap());

        gfx_hal::pso::GraphicsShaderSet {
            vertex: gfx_hal::pso::EntryPoint {
                entry: "main",
                module: &storage[0],
                specialization: gfx_hal::pso::Specialization::default(),
            },
            fragment: Some(gfx_hal::pso::EntryPoint {
                entry: "main",
                module: &storage[1],
                specialization: gfx_hal::pso::Specialization::default(),
            }),
            hull: None,
            domain: None,
            geometry: None,
        }
    }

    fn layout() -> Layout {
        Layout {
            sets: vec![SetLayout {
                bindings: vec![
                    gfx_hal::pso::DescriptorSetLayoutBinding {
                        binding: 0,
                        ty: gfx_hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: gfx_hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    gfx_hal::pso::DescriptorSetLayoutBinding {
                        binding: 1,
                        ty: gfx_hal::pso::DescriptorType::Sampler,
                        count: 1,
                        stage_flags: gfx_hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                ],
            }],
            push_constants: Vec::new(),
        }
    }

    fn build<'b>(
        factory: &mut Factory<B>,
        _aux: &mut T,
        buffers: Vec<NodeBuffer<'b, B>>,
        images: Vec<NodeImage<'b, B>>,
        set_layouts: &[B::DescriptorSetLayout],
    ) -> Result<Self, failure::Error> {
        assert!(buffers.is_empty());
        assert!(images.is_empty());
        assert_eq!(set_layouts.len(), 1);

        // This is how we can load an image and create a new texture.
        let image_bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/sprite/logo.png"
        ));
        let image = image::load_from_memory(&image_bytes[..]).unwrap().to_rgba();

        let (width, height) = image.dimensions();

        let family: &Family<B> = factory.families().first().unwrap();

        let mut image_data = Vec::<Rgba8Srgb>::new();

        for y in 0..height {
            for x in 0..width {
                image_data.push(Rgba8Srgb {
                    repr: image.get_pixel(x, y).data,
                });
            }
        }

        let texture_builder = TextureBuilder::new()
            .with_kind(gfx_hal::image::Kind::D2(width, height, 1, 1))
            .with_view_kind(gfx_hal::image::ViewKind::D2)
            .with_data_width(width)
            .with_data_height(height)
            .with_data(&image_data);

        let texture = texture_builder
            .build(
                QueueId(family.id(), 0),
                gfx_hal::image::Access::TRANSFER_WRITE,
                gfx_hal::image::Layout::TransferDstOptimal,
                factory,
            )
            .unwrap();

        let mut descriptor_pool = unsafe {
            gfx_hal::Device::create_descriptor_pool(
                factory.device(),
                1,
                &[
                    gfx_hal::pso::DescriptorRangeDesc {
                        ty: gfx_hal::pso::DescriptorType::SampledImage,
                        count: 1,
                    },
                    gfx_hal::pso::DescriptorRangeDesc {
                        ty: gfx_hal::pso::DescriptorType::Sampler,
                        count: 1,
                    },
                ],
            )
        }
        .unwrap();

        let descriptor_set = unsafe {
            gfx_hal::pso::DescriptorPool::allocate_set(&mut descriptor_pool, &set_layouts[0])
        }
        .unwrap();

        unsafe {
            gfx_hal::Device::write_descriptor_sets(
                factory.device(),
                vec![
                    gfx_hal::pso::DescriptorSetWrite {
                        set: &descriptor_set,
                        binding: 0,
                        array_offset: 0,
                        descriptors: vec![gfx_hal::pso::Descriptor::Image(
                            texture.image_view.raw(),
                            gfx_hal::image::Layout::ShaderReadOnlyOptimal,
                        )],
                    },
                    gfx_hal::pso::DescriptorSetWrite {
                        set: &descriptor_set,
                        binding: 1,
                        array_offset: 0,
                        descriptors: vec![gfx_hal::pso::Descriptor::Sampler(texture.sampler.raw())],
                    },
                ],
            );
        }

        Ok(SpriteGraphicsPipeline {
            texture,
            vertex: None,
            descriptor_pool,
            descriptor_set,
        })
    }

    fn prepare(
        &mut self,
        factory: &mut Factory<B>,
        _set_layouts: &[B::DescriptorSetLayout],
        _index: usize,
        _aux: &T,
    ) -> PrepareResult {
        if self.vertex.is_some() {
            return PrepareResult::DrawReuse;
        }

        let mut vbuf = factory
            .create_buffer(
                512,
                PosTex::VERTEX.stride as u64 * 6,
                (gfx_hal::buffer::Usage::VERTEX, MemoryUsageValue::Dynamic),
            )
            .unwrap();

        unsafe {
            // Fresh buffer.
            factory
                .upload_visible_buffer(
                    &mut vbuf,
                    0,
                    &[
                        PosTex {
                            position: [-0.5, 0.33, 0.0].into(),
                            tex_coord: [0.0, 1.0].into(),
                        },
                        PosTex {
                            position: [0.5, 0.33, 0.0].into(),
                            tex_coord: [1.0, 1.0].into(),
                        },
                        PosTex {
                            position: [0.5, -0.33, 0.0].into(),
                            tex_coord: [1.0, 0.0].into(),
                        },
                        PosTex {
                            position: [-0.5, 0.33, 0.0].into(),
                            tex_coord: [0.0, 1.0].into(),
                        },
                        PosTex {
                            position: [0.5, -0.33, 0.0].into(),
                            tex_coord: [1.0, 0.0].into(),
                        },
                        PosTex {
                            position: [-0.5, -0.33, 0.0].into(),
                            tex_coord: [0.0, 0.0].into(),
                        },
                    ],
                )
                .unwrap();
        }

        self.vertex = Some(vbuf);

        return PrepareResult::DrawRecord;
    }

    fn draw(
        &mut self,
        layout: &B::PipelineLayout,
        mut encoder: RenderPassEncoder<'_, B>,
        _index: usize,
        _aux: &T,
    ) {
        let vbuf = self.vertex.as_ref().unwrap();
        encoder.bind_graphics_descriptor_sets(
            layout,
            0,
            std::iter::once(&self.descriptor_set),
            std::iter::empty::<u32>(),
        );
        encoder.bind_vertex_buffers(0, Some((vbuf.raw(), 0)));
        encoder.draw(0..3, 0..1);
        encoder.draw(3..6, 0..1);
    }

    fn dispose(self, _factory: &mut Factory<B>, _aux: &mut T) {}
}

#[cfg(any(feature = "dx12", feature = "metal", feature = "vulkan"))]
fn run(
    event_loop: &mut EventsLoop,
    factory: &mut Factory<Backend>,
    mut graph: Graph<Backend, ()>,
) -> Result<(), failure::Error> {
    let started = std::time::Instant::now();

    std::thread::spawn(move || {
        while started.elapsed() < std::time::Duration::new(30, 0) {
            std::thread::sleep(std::time::Duration::new(1, 0));
        }

        std::process::abort();
    });

    let mut frames = 0u64..;
    let mut elapsed = started.elapsed();

    for _ in &mut frames {
        factory.cleanup();
        event_loop.poll_events(|_| ());
        graph.run(factory, &mut ());

        elapsed = started.elapsed();
        if elapsed >= std::time::Duration::new(5, 0) {
            break;
        }
    }

    let elapsed_ns = elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64;

    log::info!(
        "Elapsed: {:?}. Frames: {}. FPS: {}",
        elapsed,
        frames.start,
        frames.start * 1_000_000_000 / elapsed_ns
    );

    graph.dispose(factory, &mut ());
    Ok(())
}

#[cfg(any(feature = "dx12", feature = "metal", feature = "vulkan"))]
fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Warn)
        .filter_module("sprite", log::LevelFilter::Trace)
        .init();

    let config: Config = Default::default();

    let mut factory: Factory<Backend> = Factory::new(config).unwrap();

    let mut event_loop = EventsLoop::new();

    let window = WindowBuilder::new()
        .with_title("Rendy example")
        .build(&event_loop)
        .unwrap();

    event_loop.poll_events(|_| ());

    let surface = factory.create_surface(window.into());

    let mut graph_builder = GraphBuilder::<Backend, ()>::new();

    let color = graph_builder.create_image(
        surface.kind(),
        1,
        gfx_hal::format::Format::Rgba8Unorm,
        MemoryUsageValue::Data,
        Some(gfx_hal::command::ClearValue::Color(
            [1.0, 1.0, 1.0, 1.0].into(),
        )),
    );

    let pass = graph_builder.add_node(
        SpriteGraphicsPipeline::builder()
            .into_subpass()
            .with_color(color)
            .into_pass(),
    );

    graph_builder.add_node(PresentNode::builder(surface, color).with_dependency(pass));

    let graph = graph_builder.build(&mut factory, &mut ()).unwrap();

    run(&mut event_loop, &mut factory, graph).unwrap();
}

#[cfg(not(any(feature = "dx12", feature = "metal", feature = "vulkan")))]
fn main() {
    panic!("Specify feature: { dx12, metal, vulkan }");
}
