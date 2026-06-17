//! Navigation: definition / references responses, navigate + multi-location events.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use bevy_instanced_text_editor::RopeBuffer;
use lsp_types::*;

use crate::text_view::InstancedText;
use crate::types::{CodeEditor, CursorState};

use bevy_lsp::{LspDefinitionResponse, LspDocument, LspReferencesResponse};

/// Message emitted when navigation to a different file is requested
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct NavigateToFileEvent {
    /// URI of the file to open
    pub uri: Url,
    /// Line number (0-indexed)
    pub line: usize,
    /// Character position in line (0-indexed)
    pub character: usize,
}

/// Message emitted when there are multiple definition/reference locations
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct MultipleLocationsEvent {
    /// All available locations
    pub locations: Vec<Location>,
    /// Type of locations (definition, references, etc.)
    pub location_type: LocationType,
}

/// Type of location event
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationType {
    Definition,
    References,
}

/// Editor state mutated by [`on_lsp_definition`]: mutable cursor (moved to
/// the definition site for same-file jumps) plus the buffer and document.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct DefinitionResponseRow {
    cursor_state: &'static mut CursorState,
    buffer: &'static InstancedText<RopeBuffer>,
    lsp_document: Option<&'static LspDocument>,
}

pub fn on_lsp_definition(
    mut events: MessageReader<LspDefinitionResponse>,
    mut q: Query<DefinitionResponseRow, With<CodeEditor>>,
    mut navigate_events: MessageWriter<NavigateToFileEvent>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
) {
    for ev in events.read() {
        let Ok(row) = q.get_mut(ev.entity) else {
            continue;
        };
        let DefinitionResponseRowItem {
            mut cursor_state,
            buffer,
            lsp_document,
        } = row;
        if ev.locations.is_empty() {
            continue;
        }

        #[cfg(debug_assertions)]
        debug!("[LSP] Definition: {} location(s)", ev.locations.len());

        if ev.locations.len() > 1 {
            multi_location_events.write(MultipleLocationsEvent {
                locations: ev.locations.clone(),
                location_type: LocationType::Definition,
            });
        }

        let location = &ev.locations[0];
        let current_uri = lsp_document.map(|d| &d.uri);
        let is_same_file = current_uri.is_some_and(|uri| uri == &location.uri);

        if is_same_file {
            let line_num = location.range.start.line as usize;
            let char_in_line = location.range.start.character as usize;
            if line_num < buffer.len_lines() {
                let line_start_char = buffer.line_to_char(line_num);
                let target_char_pos = line_start_char + char_in_line;
                cursor_state.cursor_pos = target_char_pos.min(buffer.len_chars());
            }
        } else {
            navigate_events.write(NavigateToFileEvent {
                uri: location.uri.clone(),
                line: location.range.start.line as usize,
                character: location.range.start.character as usize,
            });
        }
    }
}

pub fn on_lsp_references(
    mut events: MessageReader<LspReferencesResponse>,
    editors: Query<Entity, With<CodeEditor>>,
    mut multi_location_events: MessageWriter<MultipleLocationsEvent>,
) {
    for ev in events.read() {
        if editors.get(ev.entity).is_err() {
            continue;
        }
        #[cfg(debug_assertions)]
        debug!("[LSP] References: {} location(s)", ev.locations.len());
        if !ev.locations.is_empty() {
            multi_location_events.write(MultipleLocationsEvent {
                locations: ev.locations.clone(),
                location_type: LocationType::References,
            });
        }
    }
}
