use {
    super::{RenderGroup, RenderGroupDesc},
    crate::{
        command::RenderPassEncoder,
        factory::Factory,
        node::{
            render::PrepareResult, BufferAccess, DescBuilder, ImageAccess, NodeBuffer, NodeImage,
        },
    },
    gfx_hal::{Backend, Device},
    std::marker::PhantomData,
};

/// Set layout
#[derive(Clone, Debug, Default)]
pub struct SetLayout {
    /// Set layout bindings.
    pub bindings: Vec<gfx_hal::pso::DescriptorSetLayoutBinding>,
}

/// Pipeline layout
#[derive(Clone, Debug)]
pub struct Layout {
    /// Sets in pipeline layout.
    pub sets: Vec<SetLayout>,

    /// Push constants in pipeline layout.
    pub push_constants: Vec<(gfx_hal::pso::ShaderStageFlags, std::ops::Range<u32>)>,
}

/// Pipeline info
#[derive(Clone, Debug)]
pub struct Pipeline {
    /// Layout for pipeline.
    pub layout: Layout,

    /// Vertex input for pipeline.
    pub vertices: Vec<(
        Vec<gfx_hal::pso::Element<gfx_hal::format::Format>>,
        gfx_hal::pso::ElemStride,
        gfx_hal::pso::InstanceRate,
    )>,

    /// Colors for pipeline.
    pub colors: Vec<gfx_hal::pso::ColorBlendDesc>,

    /// Depth stencil for pipeline.
    pub depth_stencil: gfx_hal::pso::DepthStencilDesc,
}

/// Simple render pipeline.
pub trait SimpleGraphicsPipeline<B: Backend, T: ?Sized>:
    std::fmt::Debug + Send + Sync + 'static
{
    /// Render pipeline name.
    fn name() -> &'static str
    where
        Self: Sized;

    /// Make simple render group builder.
    fn builder() -> DescBuilder<B, T, PhantomData<Self>>
    where
        Self: Sized,
    {
        PhantomData.builder()
    }

    /// Get set or buffer resources the node uses.
    fn buffers() -> Vec<BufferAccess>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Get set or image resources the node uses.
    fn images() -> Vec<ImageAccess>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Number of color output images.
    fn colors() -> usize
    where
        Self: Sized,
    {
        1
    }

    /// Is depth image used.
    fn depth() -> bool
    where
        Self: Sized,
    {
        true
    }

    /// Get vertex input.
    fn vertices() -> Vec<(
        Vec<gfx_hal::pso::Element<gfx_hal::format::Format>>,
        gfx_hal::pso::ElemStride,
        gfx_hal::pso::InstanceRate,
    )>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Layout for graphics pipeline
    /// Default implementation for `pipeline` will use this.
    fn layout() -> Layout {
        Layout {
            sets: Vec::new(),
            push_constants: Vec::new(),
        }
    }

    /// Graphics pipelines
    fn pipeline() -> Pipeline
    where
        Self: Sized,
    {
        Pipeline {
            layout: Self::layout(),
            vertices: Self::vertices(),
            colors: (0..Self::colors())
                .map(|_| {
                    gfx_hal::pso::ColorBlendDesc(
                        gfx_hal::pso::ColorMask::ALL,
                        gfx_hal::pso::BlendState::ALPHA,
                    )
                })
                .collect(),
            depth_stencil: if Self::depth() {
                gfx_hal::pso::DepthStencilDesc {
                    depth: gfx_hal::pso::DepthTest::On {
                        fun: gfx_hal::pso::Comparison::Less,
                        write: true,
                    },
                    depth_bounds: false,
                    stencil: gfx_hal::pso::StencilTest::Off,
                }
            } else {
                gfx_hal::pso::DepthStencilDesc::default()
            },
        }
    }

    /// Load shader set.
    /// This function should create required shader modules and fill `GraphicsShaderSet` structure.
    ///
    /// # Parameters
    ///
    /// `storage`   - vector where this function can store loaded modules to give them required lifetime.
    ///
    /// `factory`   - factory to create shader modules.
    ///
    /// `aux`       - auxiliary data container. May be anything the implementation desires.
    ///
    fn load_shader_set<'a>(
        storage: &'a mut Vec<B::ShaderModule>,
        factory: &mut Factory<B>,
        aux: &mut T,
    ) -> gfx_hal::pso::GraphicsShaderSet<'a, B>
    where
        Self: Sized;

    /// Build pass instance.
    fn build<'a>(
        factory: &mut Factory<B>,
        aux: &mut T,
        buffers: Vec<NodeBuffer<'a, B>>,
        images: Vec<NodeImage<'a, B>>,
        set_layouts: &[B::DescriptorSetLayout],
    ) -> Result<Self, failure::Error>
    where
        Self: Sized;

    /// Prepare to record drawing commands.
    ///
    /// Should return true if commands must be re-recorded.
    fn prepare(
        &mut self,
        _factory: &mut Factory<B>,
        _set_layouts: &[B::DescriptorSetLayout],
        _index: usize,
        _aux: &T,
    ) -> PrepareResult {
        PrepareResult::DrawRecord
    }

    /// Record drawing commands to the command buffer provided.
    fn draw(
        &mut self,
        layout: &B::PipelineLayout,
        encoder: RenderPassEncoder<'_, B>,
        index: usize,
        aux: &T,
    );

    fn dispose(self, factory: &mut Factory<B>, aux: &mut T);
}

#[derive(Debug)]
pub struct SimpleRenderGroup<B: Backend, P> {
    set_layouts: Vec<B::DescriptorSetLayout>,
    pipeline_layout: B::PipelineLayout,
    graphics_pipeline: B::GraphicsPipeline,
    pipeline: P,
}

impl<B, T, P> RenderGroupDesc<B, T> for PhantomData<P>
where
    B: Backend,
    T: ?Sized,
    P: SimpleGraphicsPipeline<B, T>,
{
    fn name(&self) -> &str {
        P::name()
    }

    fn buffers(&self) -> Vec<BufferAccess> {
        P::buffers()
    }

    fn images(&self) -> Vec<ImageAccess> {
        P::images()
    }

    fn colors(&self) -> usize {
        P::colors()
    }

    fn depth(&self) -> bool {
        P::depth()
    }

    fn build<'a>(
        &self,
        factory: &mut Factory<B>,
        aux: &mut T,
        framebuffer_width: u32,
        framebuffer_height: u32,
        subpass: gfx_hal::pass::Subpass<'_, B>,
        buffers: Vec<NodeBuffer<'a, B>>,
        images: Vec<NodeImage<'a, B>>,
    ) -> Result<Box<dyn RenderGroup<B, T>>, failure::Error> {
        let mut shaders = Vec::new();

        log::trace!("Load shader sets for '{}'", P::name());
        let shader_set = P::load_shader_set(&mut shaders, factory, aux);

        let pipeline = P::pipeline();

        let set_layouts = pipeline
            .layout
            .sets
            .into_iter()
            .map(|set| unsafe {
                factory
                    .device()
                    .create_descriptor_set_layout(set.bindings, std::iter::empty::<B::Sampler>())
            })
            .collect::<Result<Vec<_>, _>>()?;

        let pipeline_layout = unsafe {
            factory
                .device()
                .create_pipeline_layout(&set_layouts, pipeline.layout.push_constants)
        }?;

        assert_eq!(pipeline.colors.len(), P::colors());

        let mut vertex_buffers = Vec::new();
        let mut attributes = Vec::new();

        for &(ref elemets, stride, rate) in &pipeline.vertices {
            push_vertex_desc(elemets, stride, rate, &mut vertex_buffers, &mut attributes);
        }

        let rect = gfx_hal::pso::Rect {
            x: 0,
            y: 0,
            w: framebuffer_width as i16,
            h: framebuffer_height as i16,
        };

        let graphics_pipeline = unsafe {
            factory.device().create_graphics_pipelines(
                Some(gfx_hal::pso::GraphicsPipelineDesc {
                    shaders: shader_set,
                    rasterizer: gfx_hal::pso::Rasterizer::FILL,
                    vertex_buffers,
                    attributes,
                    input_assembler: gfx_hal::pso::InputAssemblerDesc {
                        primitive: gfx_hal::Primitive::TriangleList,
                        primitive_restart: gfx_hal::pso::PrimitiveRestart::Disabled,
                    },
                    blender: gfx_hal::pso::BlendDesc {
                        logic_op: None,
                        targets: pipeline.colors.clone(),
                    },
                    depth_stencil: pipeline.depth_stencil,
                    multisampling: None,
                    baked_states: gfx_hal::pso::BakedStates {
                        viewport: Some(gfx_hal::pso::Viewport {
                            rect,
                            depth: 0.0..1.0,
                        }),
                        scissor: Some(rect),
                        blend_color: None,
                        depth_bounds: None,
                    },
                    layout: &pipeline_layout,
                    subpass,
                    flags: gfx_hal::pso::PipelineCreationFlags::empty(),
                    parent: gfx_hal::pso::BasePipeline::None,
                }),
                None,
            )
        }
        .remove(0)?;

        let pipeline = P::build(factory, aux, buffers, images, &set_layouts)?;

        Ok(Box::new(SimpleRenderGroup::<B, P> {
            set_layouts,
            pipeline_layout,
            graphics_pipeline,
            pipeline,
        }))
    }
}

impl<B, T, P> RenderGroup<B, T> for SimpleRenderGroup<B, P>
where
    B: Backend,
    T: ?Sized,
    P: SimpleGraphicsPipeline<B, T>,
{
    fn prepare(&mut self, factory: &mut Factory<B>, index: usize, aux: &T) -> PrepareResult {
        self.pipeline
            .prepare(factory, &self.set_layouts, index, aux)
    }

    fn draw_inline(&mut self, mut encoder: RenderPassEncoder<'_, B>, index: usize, aux: &T) {
        encoder.bind_graphics_pipeline(&self.graphics_pipeline);
        self.pipeline
            .draw(&self.pipeline_layout, encoder, index, aux);
    }

    fn dispose(self: Box<Self>, factory: &mut Factory<B>, aux: &mut T) {
        self.pipeline.dispose(factory, aux);

        unsafe {
            factory
                .device()
                .destroy_graphics_pipeline(self.graphics_pipeline);
            factory
                .device()
                .destroy_pipeline_layout(self.pipeline_layout);
            for set_layout in self.set_layouts.into_iter() {
                factory.device().destroy_descriptor_set_layout(set_layout);
            }
        }
    }
}

fn push_vertex_desc(
    elements: &[gfx_hal::pso::Element<gfx_hal::format::Format>],
    stride: gfx_hal::pso::ElemStride,
    rate: gfx_hal::pso::InstanceRate,
    vertex_buffers: &mut Vec<gfx_hal::pso::VertexBufferDesc>,
    attributes: &mut Vec<gfx_hal::pso::AttributeDesc>,
) {
    let index = vertex_buffers.len() as gfx_hal::pso::BufferIndex;

    vertex_buffers.push(gfx_hal::pso::VertexBufferDesc {
        binding: index,
        stride,
        rate,
    });

    let mut location = attributes.last().map_or(0, |a| a.location + 1);
    for &element in elements {
        attributes.push(gfx_hal::pso::AttributeDesc {
            location,
            binding: index,
            element,
        });
        location += 1;
    }
}
