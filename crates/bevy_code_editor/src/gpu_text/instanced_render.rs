//! Proper GPU instancing for text glyphs using Bevy 0.17 APIs
//!
//! Based on Bevy's custom_shader_instancing example

use bevy::{
    core_pipeline::core_2d::{Transparent2d, CORE_2D_DEPTH_FORMAT},
    ecs::{
        query::QueryItem,
        system::{lifetimeless::*, SystemParamItem},
    },
    mesh::VertexBufferLayout,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::{binding_types::*, ShaderType, *},
        renderer::RenderDevice,
        texture::GpuImage,
        view::ExtractedView,
        Render, RenderApp, RenderSystems,
    },
};

use bevy_camera::visibility::RenderLayers;

use crate::text_view::{TextViewState, TextViewViewport};
use crate::types::CodeEditor;

use crate::text_view::render::{GlyphBatchComponent, GlyphInstance};

use bevy::render::Extract;

// ...

fn extract_text_globals(
    mut commands: Commands,
    code_editor_query: Extract<Query<(&TextViewState, &TextViewViewport), With<CodeEditor>>>,
) {
    // Legacy: still insert global resource for backwards compat
    if let Ok((state, viewport)) = code_editor_query.single() {
        commands.insert_resource(ExtractedTextGlobals {
            scroll_offset: Vec2::new(state.horizontal_scroll_offset, state.scroll_offset),
            viewport_size: Vec2::new(viewport.width as f32, viewport.height as f32),
        });
    }
}

/// Plugin for instanced glyph rendering
pub struct InstancedTextRenderPlugin;

impl Plugin for InstancedTextRenderPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "text.wgsl");

        app.add_plugins(ExtractComponentPlugin::<GlyphBatchComponent>::default());

        // Extract system must run in RenderApp's ExtractSchedule to insert resources in render world
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(ExtractSchedule, extract_text_globals);

        render_app
            .add_render_command::<Transparent2d, DrawInstancedText>()
            .init_resource::<SpecializedRenderPipelines<InstancedTextPipeline>>()
            .add_systems(
                Render,
                (
                    init_instanced_text_pipeline
                        .run_if(not(resource_exists::<InstancedTextPipeline>)),
                    prepare_view_bind_group
                        .run_if(resource_exists::<InstancedTextPipeline>)
                        .in_set(RenderSystems::PrepareResources),
                    queue_instanced_text
                        .run_if(resource_exists::<InstancedTextPipeline>)
                        .in_set(RenderSystems::QueueMeshes),
                    prepare_instance_buffers
                        .run_if(resource_exists::<InstancedTextPipeline>)
                        .in_set(RenderSystems::PrepareResources),
                ),
            );
    }
}

/// Make GlyphBatchComponent extractable to render world
impl ExtractComponent for GlyphBatchComponent {
    type QueryData = &'static GlyphBatchComponent;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(GlyphBatchComponent {
            instances: item.instances.clone(),
            atlas_texture: item.atlas_texture.clone(),
            render_layer: item.render_layer,
        })
    }
}

#[derive(Component, Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
#[repr(C)]
struct TextGlobals {
    viewport_size: Vec2,
    scroll_offset: Vec2,
}

#[derive(Resource)]
struct ExtractedTextGlobals {
    scroll_offset: Vec2,
    viewport_size: Vec2,
}

/// GPU buffer for instance data
#[derive(Component)]
pub struct InstanceBuffer {
    pub buffer: Buffer,
    pub length: usize,
}

/// Texture bind group for atlas
#[derive(Component)]
pub struct TextureBindGroup {
    pub bind_group: BindGroup,
}

/// Prepare view bind group — each view gets its own TextGlobals derived from its projection.
///
/// For orthographic cameras (code editor, chat), the viewport size is derived from the
/// projection matrix: width = 2/m[0][0], height = 2/m[1][1]. This gives the logical
/// viewport size matching ScalingMode::Fixed, so each camera gets correct NDC conversion.
fn prepare_view_bind_group(
    mut commands: Commands,
    pipeline: Res<InstancedTextPipeline>,
    render_device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedView)>,
    globals: Option<Res<ExtractedTextGlobals>>,
) {
    for (entity, view) in &views {
        let m = &view.clip_from_view;
        let is_ortho = (m.w_axis.w - 1.0).abs() < 0.001;

        let (scroll_offset, viewport_size) = if is_ortho {
            // Derive logical viewport from orthographic projection matrix
            let width = 2.0 / m.x_axis.x;
            let height = 2.0 / m.y_axis.y;
            // Use extracted scroll offset if this matches the code editor's viewport
            let scroll = if let Some(ref g) = globals {
                if (g.viewport_size.x - width).abs() < 1.0
                    && (g.viewport_size.y - height).abs() < 1.0
                {
                    g.scroll_offset
                } else {
                    Vec2::ZERO
                }
            } else {
                Vec2::ZERO
            };
            (scroll, Vec2::new(width, height))
        } else if let Some(ref g) = globals {
            // Non-ortho fallback: use extracted globals (legacy)
            (g.scroll_offset, g.viewport_size)
        } else {
            let physical = view.viewport.zw().as_vec2();
            (Vec2::ZERO, physical)
        };

        let globals_data = TextGlobals {
            viewport_size,
            scroll_offset,
        };

        let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("text_globals_buffer"),
            contents: bytemuck::bytes_of(&globals_data),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = render_device.create_bind_group(
            "text_view_bind_group",
            &pipeline.view_bind_group_layout,
            &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        );

        commands
            .entity(entity)
            .insert(TextViewBindGroup { bind_group });
    }
}

/// Prepare instance buffers on the GPU
fn prepare_instance_buffers(
    mut commands: Commands,
    query: Query<(Entity, &GlyphBatchComponent)>,
    render_device: Res<RenderDevice>,
    pipeline: Res<InstancedTextPipeline>,
    gpu_images: Res<RenderAssets<GpuImage>>,
) {
    // We now expect multiple batches (main text + line numbers), so no warning needed.
    // let batch_count = query.iter().count();
    // if batch_count > 1 {
    //     warn!("WARNING: Multiple batch entities detected in render world: {}", batch_count);
    // }

    for (entity, batch) in &query {
        if batch.instances.is_empty() {
            // Remove stale InstanceBuffer so the draw command skips this entity
            commands.entity(entity).remove::<InstanceBuffer>();
            continue;
        }

        // Debug first buffer preparation
        use std::sync::atomic::{AtomicBool, Ordering};
        static BUFFER_LOGGED: AtomicBool = AtomicBool::new(false);
        if !BUFFER_LOGGED.swap(true, Ordering::Relaxed) {
            info!("Preparing {} instances", batch.instances.len());
            if let Some(first) = batch.instances.first() {
                info!("First instance in buffer: pos=({}, {}), uv_min=({}, {}), uv_max=({}, {}), size=({}, {})",
                    first.position.x, first.position.y,
                    first.uv_min.x, first.uv_min.y,
                    first.uv_max.x, first.uv_max.y,
                    first.size.x, first.size.y);
            }
        }

        let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("glyph_instance_buffer"),
            contents: bytemuck::cast_slice(&batch.instances),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        commands.entity(entity).insert(InstanceBuffer {
            buffer,
            length: batch.instances.len(),
        });

        // Create texture bind group
        if let Some(gpu_image) = gpu_images.get(&batch.atlas_texture) {
            // Remove verbose logging - this runs every frame
            // info!("Creating texture bind group for atlas");
            let bind_group = render_device.create_bind_group(
                "text_texture_bind_group",
                &pipeline.texture_bind_group_layout,
                &BindGroupEntries::sequential((&gpu_image.texture_view, &gpu_image.sampler)),
            );

            commands
                .entity(entity)
                .insert(TextureBindGroup { bind_group });
        } else {
            warn!("Atlas texture not found in RenderAssets!");
        }
    }
}

/// Pipeline for instanced text rendering
#[derive(Resource)]
pub struct InstancedTextPipeline {
    shader: Handle<Shader>,
    view_bind_group_layout: BindGroupLayout,
    texture_bind_group_layout: BindGroupLayout,
}

fn init_instanced_text_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    render_device: Res<RenderDevice>,
) {
    // Create view bind group layout (for camera/view uniforms)
    let view_bind_group_layout = render_device.create_bind_group_layout(
        "text_view_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (uniform_buffer::<TextGlobals>(false),),
        ),
    );

    // Create texture bind group layout for atlas
    let texture_bind_group_layout = render_device.create_bind_group_layout(
        "text_texture_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );

    let shader = bevy::asset::load_embedded_asset!(asset_server.as_ref(), "text.wgsl");

    commands.insert_resource(InstancedTextPipeline {
        shader,
        view_bind_group_layout,
        texture_bind_group_layout,
    });
}

impl SpecializedRenderPipeline for InstancedTextPipeline {
    type Key = u32; // MSAA sample count

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        // Instance buffer layout
        let instance_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                // position (vec2)
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // uv_min (vec2)
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                // uv_max (vec2)
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 16,
                    shader_location: 2,
                },
                // size (vec2)
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 24,
                    shader_location: 3,
                },
                // color (vec4)
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 4,
                },
                // z_index (f32)
                VertexAttribute {
                    format: VertexFormat::Float32,
                    offset: 48,
                    shader_location: 5,
                },
                // corner_radius (f32)
                VertexAttribute {
                    format: VertexFormat::Float32,
                    offset: 52,
                    shader_location: 6,
                },
                // skew (f32)
                VertexAttribute {
                    format: VertexFormat::Float32,
                    offset: 56,
                    shader_location: 7,
                },
            ],
        };

        RenderPipelineDescriptor {
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: vec![],
                entry_point: Some("vertex".into()),
                buffers: vec![instance_layout],
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: vec![],
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            layout: vec![
                self.view_bind_group_layout.clone(),
                self.texture_bind_group_layout.clone(),
            ],
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_2D_DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState {
                count: key,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: true,
            label: Some("instanced_text_pipeline".into()),
        }
    }
}

/// Queue instanced text for rendering, filtered by RenderLayers.
fn queue_instanced_text(
    transparent_2d_draw_functions: Res<DrawFunctions<Transparent2d>>,
    instanced_text_pipeline: Res<InstancedTextPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<InstancedTextPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    batches: Query<(Entity, Option<&GlobalTransform>, &GlyphBatchComponent)>,
    mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent2d>>,
    views: Query<(Entity, &ExtractedView, Option<&RenderLayers>)>,
) {
    let draw_function = transparent_2d_draw_functions
        .read()
        .id::<DrawInstancedText>();

    for (_view_entity, view, view_layers) in &views {
        let view_render_layers = view_layers.cloned().unwrap_or(RenderLayers::layer(0));

        let Some(transparent_phase) = transparent_render_phases.get_mut(&view.retained_view_entity)
        else {
            continue;
        };

        // Use MSAA sample count of 4 (default for Bevy 2D)
        let pipeline_id = pipelines.specialize(&pipeline_cache, &instanced_text_pipeline, 4);

        for (entity, global_transform, batch) in &batches {
            // Filter: only queue batches visible to this view's render layers.
            // None = no explicit layer = visible to all cameras (skip filtering).
            if let Some(layer) = batch.render_layer {
                let batch_render_layers = RenderLayers::layer(layer as usize);
                if !view_render_layers.intersects(&batch_render_layers) {
                    continue;
                }
            }

            // Use Z from GlobalTransform as sort key for proper layering with sprites
            // Higher Z values render on top (later in the sorted phase)
            let z = global_transform.map(|t| t.translation().z).unwrap_or(0.0);

            transparent_phase.add(Transparent2d {
                entity: (entity, entity.into()),
                pipeline: pipeline_id,
                draw_function,
                sort_key: bevy::math::FloatOrd(z),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                extracted_index: usize::MAX,
                indexed: false, // We use draw() not draw_indexed()
            });
        }
    }
}

/// Draw command for instanced text
type DrawInstancedText = (
    SetItemPipeline,
    SetViewBindGroup<0>,
    SetTextureBindGroup<1>,
    DrawGlyphInstances,
);

/// Set view bind group
struct SetViewBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetViewBindGroup<I> {
    type Param = ();
    type ViewQuery = Read<TextViewBindGroup>;
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        view_bind_group: &'w TextViewBindGroup,
        _query: Option<()>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &view_bind_group.bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// Bind group for view uniforms
#[derive(Component)]
struct TextViewBindGroup {
    bind_group: BindGroup,
}

/// Set texture bind group
struct SetTextureBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTextureBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<TextureBindGroup>;

    fn render<'w>(
        _item: &P,
        _view: (),
        texture_bind_group: Option<&'w TextureBindGroup>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(texture_bind_group) = texture_bind_group else {
            return RenderCommandResult::Skip;
        };

        pass.set_bind_group(I, &texture_bind_group.bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct DrawGlyphInstances;

impl<P: PhaseItem> RenderCommand<P> for DrawGlyphInstances {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<InstanceBuffer>;

    fn render<'w>(
        _item: &P,
        _view: (),
        instance_buffer: Option<&'w InstanceBuffer>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(instance_buffer) = instance_buffer else {
            return RenderCommandResult::Skip;
        };

        if instance_buffer.length == 0 {
            return RenderCommandResult::Skip;
        }

        // Debug: log first draw call
        use std::sync::atomic::{AtomicBool, Ordering};
        static DRAW_LOGGED: AtomicBool = AtomicBool::new(false);
        if !DRAW_LOGGED.swap(true, Ordering::Relaxed) {
            bevy::log::info!(
                "DrawGlyphInstances: drawing {} instances, buffer size: {} bytes",
                instance_buffer.length,
                instance_buffer.buffer.size()
            );
        }

        // Set instance buffer (slot 0, no mesh needed)
        pass.set_vertex_buffer(0, instance_buffer.buffer.slice(..));

        // Draw quad instances (6 vertices per quad, triangle list)
        pass.draw(0..6, 0..instance_buffer.length as u32);

        RenderCommandResult::Success
    }
}
