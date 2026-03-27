//! Tree-sitter syntax highlighting example
//!
//! Demonstrates how to use tree-sitter for syntax highlighting in the code editor.
//! This example highlights Rust code using the tree-sitter-rust grammar.

use bevy::prelude::*;
use bevy_code_editor::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Tree-sitter Syntax Highlighting Example".to_string(),
                resolution: (1200, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CodeEditorPlugin::default())
        .add_plugins(EditorUiPlugin::default())
        .add_systems(PostStartup, setup_editor_with_treesitter)
        .run();
}

#[cfg(feature = "tree-sitter")]
fn setup_editor_with_treesitter(
    mut editor_query: Query<(&mut CodeEditorState, &mut TextViewState), With<CodeEditor>>,
    mut syntax: ResMut<SyntaxResource>,
) {
    let Ok((mut state, mut tv)) = editor_query.single_mut() else {
        return;
    };

    #[cfg(feature = "instanced-rendering")]
    info!("Instanced rendering enabled! using optimized single-draw-call rendering.");
    #[cfg(not(feature = "instanced-rendering"))]
    info!("Using default per-line mesh rendering. Enable 'instanced-rendering' feature for better performance.");

    // Sample Rust code to demonstrate syntax highlighting
    let rust_code = r#"// Rust syntax highlighting with tree-sitter
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

    state.set_text(&mut tv, rust_code);

    // Define Rust language configuration
    let rust_lang = Language {
        name: "rust",
        tree_sitter: Some(TreeSitterConfig {
            grammar: tree_sitter_rust::LANGUAGE.into(),
            highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY,
        }),
        #[cfg(feature = "lsp")]
        lsp_command: Some(("rust-analyzer", &[])),
    };

    // Set up tree-sitter highlighting using the language configuration
    if let Some(provider) = rust_lang.create_tree_sitter_provider() {
        syntax.set_provider(provider);
    }

    tv.needs_update = true;
}

#[cfg(not(feature = "tree-sitter"))]
fn setup_editor_with_treesitter(
    mut editor_query: Query<(&mut CodeEditorState, &mut TextViewState), With<CodeEditor>>,
) {
    let Ok((mut state, mut tv)) = editor_query.single_mut() else {
        return;
    };

    let message = r#"Tree-sitter feature is not enabled!

To run this example with syntax highlighting, use:

    cargo run --example treesitter_highlighting --features tree-sitter

Or use the default features:

    cargo run --example treesitter_highlighting
"#;

    state.set_text(&mut tv, message);
}
