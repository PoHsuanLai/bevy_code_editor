//! Tree-sitter syntax highlighting example
//!
//! Demonstrates how to use tree-sitter for syntax highlighting in the code editor.
//! This example highlights Rust code using the tree-sitter-rust grammar.

use bevy::prelude::*;
use bevy_code_editor::prelude::*;

#[cfg(feature = "tree-sitter")]
use tree_sitter_highlight::HighlightConfiguration;

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
        .add_systems(Startup, setup_editor_with_treesitter)
        .run();
}

#[cfg(feature = "tree-sitter")]
fn setup_editor_with_treesitter(mut state: ResMut<CodeEditorState>) {
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

    state.set_text(rust_code);

    // Set up tree-sitter highlighting configuration for Rust
    let language = tree_sitter_rust::LANGUAGE;

    // These are the actual capture names from tree-sitter-rust's highlights.scm
    let highlight_names = vec![
        "attribute",
        "comment",
        "comment.documentation",
        "constant",
        "constant.builtin",
        "constructor",
        "escape",
        "function",
        "function.macro",
        "function.method",
        "keyword",
        "label",
        "operator",
        "property",
        "punctuation.bracket",
        "punctuation.delimiter",
        "string",
        "type",
        "type.builtin",
        "variable.builtin",
        "variable.parameter",
    ];

    let mut config = HighlightConfiguration::new(
        language.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        "",
        "",
    )
    .expect("Failed to create highlight configuration");

    config.configure(&highlight_names);
    state.highlight_config = Some(config);

    // Trigger initial highlighting
    state.update_highlighting();
    state.needs_update = true;
}

#[cfg(not(feature = "tree-sitter"))]
fn setup_editor_with_treesitter(mut state: ResMut<CodeEditorState>) {
    let message = r#"Tree-sitter feature is not enabled!

To run this example with syntax highlighting, use:

    cargo run --example treesitter_highlighting --features tree-sitter

Or use the default features:

    cargo run --example treesitter_highlighting
"#;

    state.set_text(message);
}
