//! Tree-sitter syntax highlighting example
//!
//! Demonstrates how to use tree-sitter for syntax highlighting in the code editor.
//! This example highlights Rust code using the tree-sitter-rust grammar.

use bevy::prelude::*;
use bevy_code_editor::prelude::*;
#[cfg(feature = "tree-sitter")]
use bevy_tree_sitter::Language;

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
        .add_plugins(CodeEditorPlugin::standalone())
        .add_systems(PostStartup, setup_editor_with_treesitter)
        .run();
}

fn setup_editor_with_treesitter(
    #[cfg(feature = "tree-sitter")] mut commands: Commands,
    editor_query: Query<Entity, With<CodeEditor>>,
    mut set_text_writer: MessageWriter<bevy_text_editor::SetTextRequested>,
) {
    let Ok(entity) = editor_query.single() else {
        return;
    };

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

    set_text_writer.write(bevy_text_editor::SetTextRequested {
        entity,
        text: text.to_string(),
    });

    #[cfg(feature = "tree-sitter")]
    commands.entity(entity).insert(Language::from_grammar(
        "rust",
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY,
    ));
}
