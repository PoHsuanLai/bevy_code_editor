//! Tree-sitter syntax highlighting example
//!
//! Demonstrates how to use tree-sitter for syntax highlighting in the code editor.
//! This example highlights Rust code using the tree-sitter-rust grammar.

use bevscode::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Tree-sitter Syntax Highlighting Example".to_string(),
                        resolution: (1200, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::asset::AssetPlugin {
                    file_path: "assets".into(),
                    ..default()
                }),
        )
        .add_plugins(CodeEditorPlugins)
        .add_systems(Startup, (setup_camera, spawn_editor))
        .add_systems(PostStartup, setup_editor_with_treesitter)
        .add_systems(Update, debug_toggles)
        .run();
}

/// Debug actions on mouse-button input.
///
/// - Mouse Back  : toggle main text batch visibility
/// - Mouse Forward : toggle line-numbers GPU batch visibility
/// - Middle click : dump every glyph instance (position/size/uv) to stderr
fn debug_toggles(
    mouse: Res<ButtonInput<MouseButton>>,
    mut text_batches: Query<
        &mut Visibility,
        (
            With<GlyphBatchComponent>,
            Without<bevscode::plugin::gpu_line_numbers::GpuLineNumbersBatch>,
        ),
    >,
    mut line_batches: Query<
        &mut Visibility,
        With<bevscode::plugin::gpu_line_numbers::GpuLineNumbersBatch>,
    >,
    all_batches: Query<(
        Entity,
        Option<&Name>,
        &GlyphBatchComponent,
    )>,
) {
    if mouse.just_pressed(MouseButton::Back) {
        for mut vis in text_batches.iter_mut() {
            *vis = match *vis {
                Visibility::Hidden => Visibility::Visible,
                _ => Visibility::Hidden,
            };
        }
        eprintln!("[debug] toggled main text batch visibility");
    }

    if mouse.just_pressed(MouseButton::Forward) {
        for mut vis in line_batches.iter_mut() {
            *vis = match *vis {
                Visibility::Hidden => Visibility::Visible,
                _ => Visibility::Hidden,
            };
        }
        eprintln!("[debug] toggled line-numbers batch visibility");
    }

    if mouse.just_pressed(MouseButton::Middle) {
        for (e, name, batch) in all_batches.iter() {
            let label = name.map(|n| n.as_str()).unwrap_or("<unnamed>");
            eprintln!(
                "[debug] batch {:?} '{}' — {} instances",
                e,
                label,
                batch.instances.len()
            );
            for (i, inst) in batch.instances.iter().enumerate() {
                eprintln!(
                    "  [{:>4}] pos=({:>8.2},{:>8.2}) size=({:>6.2},{:>6.2}) uv=({:.4},{:.4})..({:.4},{:.4}) z={} color=[{:.2},{:.2},{:.2},{:.2}]",
                    i,
                    inst.position.x, inst.position.y,
                    inst.size.x, inst.size.y,
                    inst.uv_min.x, inst.uv_min.y,
                    inst.uv_max.x, inst.uv_max.y,
                    inst.z_index,
                    inst.color[0], inst.color[1], inst.color[2], inst.color[3],
                );
            }
        }
    }
}

fn spawn_editor(mut commands: Commands) {
    commands.spawn((CodeEditor, AutoResizeViewport, Name::new("CodeEditor")));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(EditorTheme::default().background),
            ..default()
        },
    ));
}

fn setup_editor_with_treesitter(
    mut commands: Commands,
    editor_query: Query<Entity, With<CodeEditor>>,
    asset_server: Res<AssetServer>,
    mut set_text_writer: MessageWriter<SetTextRequested>,
) {
    let Ok(entity) = editor_query.single() else {
        return;
    };

    commands.entity(entity).insert((
        TextFont::from_font_size(14.0)
            .with_font(asset_server.load("fonts/FiraMono-Regular.ttf")),
        MonoFontFaces::default()
            .with_bold(asset_server.load("fonts/FiraMono-Medium.ttf")),
    ));

    #[cfg(feature = "tree-sitter")]
    let text = r#"// Rust syntax highlighting with tree-sitter
use std::collections::HashMap;

/// A simple struct to demonstrate syntax highlighting
#[derive(Debug, Clone)]
pub struct Person {
    pub name: String,
    pub age: u32,
    tags: Vec<String>,
}

impl Person {
    /// Create a new person
    pub fn new(name: String, age: u32) -> Self {
        Self {
            name,
            age,
            tags: Vec::new(),
        }
    }

    /// Add a tag to the person
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.push(tag.into());
    }

    /// Check if person is an adult
    pub fn is_adult(&self) -> bool {
        self.age >= 18
    }
}

fn main() {
    let mut person = Person::new("Alice".to_string(), 25);
    person.add_tag("developer");
    person.add_tag("rust-enthusiast");

    println!("Person: {:?}", person);
    println!("Is adult: {}", person.is_adult());

    // HashMap example
    let mut scores = HashMap::new();
    scores.insert("Alice", 100);
    scores.insert("Bob", 85);

    for (name, score) in &scores {
        println!("{}: {}", name, score);
    }

    // Pattern matching
    match person.age {
        0..=17 => println!("Minor"),
        18..=65 => println!("Adult"),
        _ => println!("Senior"),
    }

    // Closure example
    let numbers = vec![1, 2, 3, 4, 5];
    let doubled: Vec<_> = numbers.iter().map(|x| x * 2).collect();
    println!("Doubled: {:?}", doubled);
}
"#;

    #[cfg(not(feature = "tree-sitter"))]
    let text = "Tree-sitter feature is not enabled!\n\nRun with `--features tree-sitter`.";

    set_text_writer.write(SetTextRequested {
        entity,
        text: text.to_string(),
    });

    #[cfg(feature = "tree-sitter")]
    commands.entity(entity).insert(TreeSitterGrammar::new(
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY,
    ));
}
