//! IDE integration API for advanced LSP features
//!
//! This module provides events and helper functions for IDE-level features
//! that require external UI (panels, tree views, file navigation, etc.).
//!
//! The code editor handles inline features (completion, hover, diagnostics),
//! while this API allows a full IDE to integrate advanced features like:
//! - Document outline/symbols
//! - Workspace-wide symbol search
//! - Call hierarchy navigation
//! - Type hierarchy browsing
//! - Cross-file navigation
//!
//! # Architecture
//!
//! Features in this module follow a simple pattern:
//! 1. IDE calls a request helper function (e.g., `request_document_symbols`)
//! 2. LSP client sends the request to the language server
//! 3. When response arrives, an event is emitted (e.g., `DocumentSymbolsEvent`)
//! 4. IDE systems listen to these events and update their UI
//!
//! # Example: Document Outline Panel
//!
//! ```rust,ignore
//! use bevy::prelude::*;
//! use bevy_code_editor::lsp::ide_api::*;
//! use bevy_code_editor::lsp::prelude::*;
//!
//! // In your IDE setup
//! fn setup_ide(app: &mut App) {
//!     app.add_plugins(CodeEditorPlugin::default())
//!        .add_plugins(LspPlugin::default())
//!        .add_systems(Update, (
//!            request_symbols_on_file_open,
//!            update_outline_panel,
//!        ));
//! }
//!
//! // Request symbols when a file is opened
//! fn request_symbols_on_file_open(
//!     lsp_client: Res<LspClient>,
//!     lsp_sync: Res<LspSyncState>,
//!     // ... file open detection logic
//! ) {
//!     if let Some(uri) = &lsp_sync.document_uri {
//!         request_document_symbols(&lsp_client, uri);
//!     }
//! }
//!
//! // Update outline panel when symbols arrive
//! fn update_outline_panel(
//!     mut events: MessageReader<DocumentSymbolsEvent>,
//!     mut outline_panel: ResMut<OutlinePanel>,
//! ) {
//!     for event in events.read() {
//!         outline_panel.update_symbols(&event.symbols);
//!     }
//! }
//! ```
//!
//! # Example: Workspace Symbol Search
//!
//! ```rust,ignore
//! use bevy_code_editor::lsp::ide_api::*;
//!
//! // User types in search box
//! fn handle_search_input(
//!     lsp_client: Res<LspClient>,
//!     search_input: Res<SearchInput>,
//! ) {
//!     if search_input.is_changed() {
//!         request_workspace_symbols(&lsp_client, search_input.query.clone());
//!     }
//! }
//!
//! // Display results
//! fn show_search_results(
//!     mut events: MessageReader<WorkspaceSymbolsEvent>,
//!     mut results_panel: ResMut<SearchResultsPanel>,
//! ) {
//!     for event in events.read() {
//!         results_panel.show_symbols(&event.symbols);
//!     }
//! }
//! ```

use bevy::prelude::*;
use lsp_types::*;

use super::client::LspClient;

// ============================================================================
// Events for IDE Integration
// ============================================================================

/// Event emitted when document symbols are received (for outline/structure view)
///
/// Document symbols represent the structure of a file: functions, classes,
/// variables, etc. IDEs use this to show an outline/navigation panel.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct DocumentSymbolsEvent {
    /// URI of the document
    pub uri: Url,
    /// Symbols in the document (hierarchical structure)
    pub symbols: Vec<DocumentSymbol>,
}

/// Event emitted when workspace symbols search results are received
///
/// Workspace symbols allow searching for symbols across the entire project.
/// IDEs use this for "Go to Symbol" or "Quick Open" features.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct WorkspaceSymbolsEvent {
    /// The search query that was used
    pub query: String,
    /// Matching symbols from across the workspace
    pub symbols: Vec<SymbolInformation>,
}

/// Event emitted when type definition location is received
///
/// Type definition navigates to where a type is defined (not where a variable
/// is declared). Useful for navigating from a variable to its type definition.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct TypeDefinitionEvent {
    /// Locations where the type is defined (usually just one)
    pub locations: Vec<Location>,
}

/// Event emitted when implementation locations are received
///
/// Implementation locations show where an interface or abstract method is
/// implemented. Useful for navigating from interface to concrete implementations.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct ImplementationEvent {
    /// Locations of implementations
    pub locations: Vec<Location>,
}

/// Direction for call hierarchy requests
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDirection {
    /// Show incoming calls (who calls this function)
    Incoming,
    /// Show outgoing calls (what this function calls)
    Outgoing,
}

/// Event emitted when call hierarchy is received
///
/// Call hierarchy shows the call graph: which functions call this function
/// (incoming) or which functions this function calls (outgoing).
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct CallHierarchyEvent {
    /// The direction of the call hierarchy
    pub direction: CallDirection,
    /// The call hierarchy items
    pub items: Vec<CallHierarchyItem>,
}

/// Event emitted when type hierarchy is received
///
/// Type hierarchy shows the inheritance tree: supertypes (parents) or
/// subtypes (children) of a type.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct TypeHierarchyEvent {
    /// Whether these are supertypes or subtypes
    pub is_supertypes: bool,
    /// The type hierarchy items
    pub items: Vec<TypeHierarchyItem>,
}

/// Event emitted when folding ranges are received
///
/// Folding ranges define which code blocks can be collapsed/expanded.
/// IDEs use this to implement code folding in the gutter.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct FoldingRangesEvent {
    /// URI of the document
    pub uri: Url,
    /// Ranges that can be folded
    pub ranges: Vec<FoldingRange>,
}

/// Event emitted when selection ranges are received
///
/// Selection ranges implement "expand selection" and "shrink selection"
/// features, allowing users to progressively select larger/smaller syntactic units.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct SelectionRangesEvent {
    /// The selection ranges for the requested positions
    pub ranges: Vec<SelectionRange>,
}

// ============================================================================
// Helper Functions for Requesting IDE Features
// ============================================================================

/// Request document symbols (for outline/structure view)
///
/// This requests the hierarchical structure of a document. The response will
/// be emitted as a `DocumentSymbolsEvent`.
///
/// # Example
/// ```rust,ignore
/// fn on_file_open(lsp_client: Res<LspClient>, uri: &Url) {
///     request_document_symbols(&lsp_client, uri);
/// }
/// ```
pub fn request_document_symbols(lsp_client: &LspClient, uri: &Url) {
    if lsp_client.capabilities.supports_document_symbols() {
        lsp_client.send(super::messages::LspMessage::DocumentSymbols {
            uri: uri.clone(),
        });
    }
}

/// Request workspace-wide symbol search
///
/// This searches for symbols matching a query across the entire workspace.
/// The response will be emitted as a `WorkspaceSymbolsEvent`.
///
/// # Example
/// ```rust,ignore
/// fn on_search_input(lsp_client: Res<LspClient>, query: String) {
///     request_workspace_symbols(&lsp_client, query);
/// }
/// ```
pub fn request_workspace_symbols(lsp_client: &LspClient, query: String) {
    if lsp_client.capabilities.supports_workspace_symbols() {
        lsp_client.send(super::messages::LspMessage::WorkspaceSymbols { query });
    }
}

/// Request type definition location
///
/// This navigates to where a type is defined (not where a variable is declared).
/// The response will be emitted as a `TypeDefinitionEvent`.
///
/// # Example
/// ```rust,ignore
/// fn on_ctrl_click(lsp_client: Res<LspClient>, uri: &Url, position: Position) {
///     request_type_definition(&lsp_client, uri, position);
/// }
/// ```
pub fn request_type_definition(lsp_client: &LspClient, uri: &Url, position: Position) {
    if lsp_client.capabilities.supports_type_definition() {
        lsp_client.send(super::messages::LspMessage::TypeDefinition {
            uri: uri.clone(),
            position,
        });
    }
}

/// Request implementation locations
///
/// This finds where an interface or abstract method is implemented.
/// The response will be emitted as an `ImplementationEvent`.
///
/// # Example
/// ```rust,ignore
/// fn find_implementations(lsp_client: Res<LspClient>, uri: &Url, position: Position) {
///     request_implementation(&lsp_client, uri, position);
/// }
/// ```
pub fn request_implementation(lsp_client: &LspClient, uri: &Url, position: Position) {
    if lsp_client.capabilities.supports_implementation() {
        lsp_client.send(super::messages::LspMessage::Implementation {
            uri: uri.clone(),
            position,
        });
    }
}

/// Request call hierarchy (prepare step)
///
/// Call hierarchy is a two-step process:
/// 1. First call this to prepare the call hierarchy
/// 2. Then request incoming or outgoing calls using the returned items
///
/// The response will be emitted as a `CallHierarchyEvent`.
///
/// # Example
/// ```rust,ignore
/// fn show_call_hierarchy(lsp_client: Res<LspClient>, uri: &Url, position: Position) {
///     request_call_hierarchy_prepare(&lsp_client, uri, position);
/// }
/// ```
pub fn request_call_hierarchy_prepare(lsp_client: &LspClient, uri: &Url, position: Position) {
    if lsp_client.capabilities.supports_call_hierarchy() {
        lsp_client.send(super::messages::LspMessage::CallHierarchyPrepare {
            uri: uri.clone(),
            position,
        });
    }
}

/// Request incoming or outgoing calls for a call hierarchy item
///
/// This is the second step of call hierarchy after prepare.
///
/// # Example
/// ```rust,ignore
/// fn expand_call_node(
///     lsp_client: Res<LspClient>,
///     item: CallHierarchyItem,
///     direction: CallDirection,
/// ) {
///     request_call_hierarchy_calls(&lsp_client, item, direction);
/// }
/// ```
pub fn request_call_hierarchy_calls(
    lsp_client: &LspClient,
    item: CallHierarchyItem,
    direction: CallDirection,
) {
    if !lsp_client.capabilities.supports_call_hierarchy() {
        return;
    }

    match direction {
        CallDirection::Incoming => {
            lsp_client.send(super::messages::LspMessage::CallHierarchyIncoming { item });
        }
        CallDirection::Outgoing => {
            lsp_client.send(super::messages::LspMessage::CallHierarchyOutgoing { item });
        }
    }
}

/// Request type hierarchy (prepare step)
///
/// Type hierarchy is similar to call hierarchy: prepare first, then
/// request supertypes or subtypes.
///
/// The response will be emitted as a `TypeHierarchyEvent`.
pub fn request_type_hierarchy_prepare(lsp_client: &LspClient, uri: &Url, position: Position) {
    if lsp_client.capabilities.supports_type_hierarchy() {
        lsp_client.send(super::messages::LspMessage::TypeHierarchyPrepare {
            uri: uri.clone(),
            position,
        });
    }
}

/// Request supertypes or subtypes for a type hierarchy item
///
/// This is the second step of type hierarchy after prepare.
pub fn request_type_hierarchy_types(
    lsp_client: &LspClient,
    item: TypeHierarchyItem,
    get_supertypes: bool,
) {
    if !lsp_client.capabilities.supports_type_hierarchy() {
        return;
    }

    if get_supertypes {
        lsp_client.send(super::messages::LspMessage::TypeHierarchySupertypes { item });
    } else {
        lsp_client.send(super::messages::LspMessage::TypeHierarchySubtypes { item });
    }
}

/// Request folding ranges for a document
///
/// Folding ranges define which code blocks can be collapsed/expanded.
/// The response will be emitted as a `FoldingRangesEvent`.
pub fn request_folding_ranges(lsp_client: &LspClient, uri: &Url) {
    if lsp_client.capabilities.supports_folding_range() {
        lsp_client.send(super::messages::LspMessage::FoldingRange {
            uri: uri.clone(),
        });
    }
}

/// Request selection ranges for positions
///
/// Selection ranges implement "expand selection" functionality.
/// The response will be emitted as a `SelectionRangesEvent`.
pub fn request_selection_ranges(lsp_client: &LspClient, uri: &Url, positions: Vec<Position>) {
    if lsp_client.capabilities.supports_selection_range() {
        lsp_client.send(super::messages::LspMessage::SelectionRange {
            uri: uri.clone(),
            positions,
        });
    }
}

// ============================================================================
// Note on Capability Checks
// ============================================================================
//
// Capability checking methods are defined directly on `ServerCapabilitiesCache`
// in the `capabilities` module. See:
// - supports_document_symbols()
// - supports_workspace_symbols()
// - supports_type_definition()
// - supports_implementation()
// - supports_call_hierarchy()
// - supports_type_hierarchy()
// - supports_folding_range()
// - supports_selection_range()
