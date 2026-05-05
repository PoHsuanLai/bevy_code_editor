//! Edit history and undo/redo types

use std::time::Instant;

/// Type of edit for transaction grouping decisions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Inserting characters (typing)
    Insert,
    /// Deleting backward (backspace)
    DeleteBackward,
    /// Deleting forward (delete key)
    DeleteForward,
    /// Inserting a newline (breaks transaction)
    Newline,
    /// Pasting text (always its own transaction)
    Paste,
    /// Other operations that should be their own transaction
    Other,
}

/// A single edit operation that can be undone/redone
#[derive(Clone, Debug)]
pub struct EditOperation {
    /// The text that was removed (empty for insertions)
    pub removed_text: String,
    /// The text that was inserted (empty for deletions)
    pub inserted_text: String,
    /// The position where the edit occurred (char index)
    pub position: usize,
    /// Cursor position before the edit
    pub cursor_before: usize,
    /// Cursor position after the edit
    pub cursor_after: usize,
    /// The kind of edit (for grouping decisions)
    pub kind: EditKind,
}

/// A transaction groups multiple edits that should be undone/redone together
#[derive(Clone, Debug)]
pub struct EditTransaction {
    /// The operations in this transaction (in order of execution)
    pub operations: Vec<EditOperation>,
    /// When this transaction was created
    pub timestamp: Instant,
}

impl EditTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            timestamp: Instant::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl Default for EditTransaction {
    fn default() -> Self {
        Self::new()
    }
}

/// History manager for undo/redo operations
#[derive(Clone, Debug)]
pub struct EditHistory {
    /// Stack of undo transactions
    pub undo_stack: Vec<EditTransaction>,
    /// Stack of redo transactions
    pub redo_stack: Vec<EditTransaction>,
    /// Current transaction being built (groups rapid edits)
    pub current_transaction: Option<EditTransaction>,
    /// Time interval for grouping edits into transactions (in milliseconds)
    pub group_interval_ms: u64,
    /// Maximum number of transactions to keep
    pub max_history_size: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            current_transaction: None,
            group_interval_ms: 300, // Group edits within 300ms
            max_history_size: 1000,
        }
    }
}

impl EditHistory {
    /// Record an edit operation
    pub fn record(&mut self, operation: EditOperation) {
        let now = Instant::now();
        let op_kind = operation.kind;

        // Check if we should start a new transaction based on multiple criteria:
        // 1. Time elapsed since last edit
        // 2. Edit kind changed (e.g., typing -> deleting)
        // 3. Certain edit kinds always break transactions (newline, paste)
        // 4. Non-contiguous edits (cursor jumped)
        let should_start_new = match &self.current_transaction {
            Some(tx) => {
                let elapsed = now.duration_since(tx.timestamp).as_millis() as u64;

                // Time-based break
                if elapsed > self.group_interval_ms {
                    return self.start_new_transaction(operation, now);
                }

                // Certain operations always start a new transaction
                if matches!(
                    op_kind,
                    EditKind::Newline | EditKind::Paste | EditKind::Other
                ) {
                    return self.start_new_transaction(operation, now);
                }

                // Check if edit kind changed (typing vs deleting)
                if let Some(last_op) = tx.operations.last() {
                    let last_kind = last_op.kind;

                    // Different edit directions should break transaction
                    let kind_changed = match (last_kind, op_kind) {
                        (EditKind::Insert, EditKind::Insert) => false,
                        (EditKind::DeleteBackward, EditKind::DeleteBackward) => false,
                        (EditKind::DeleteForward, EditKind::DeleteForward) => false,
                        _ => true,
                    };

                    if kind_changed {
                        return self.start_new_transaction(operation, now);
                    }

                    // Check for non-contiguous edits
                    // For inserts: new position should be right after last cursor_after
                    // For deletes: position should be adjacent to last position
                    let is_contiguous = match op_kind {
                        EditKind::Insert => operation.position == last_op.cursor_after,
                        EditKind::DeleteBackward => operation.cursor_before == last_op.cursor_after,
                        EditKind::DeleteForward => operation.position == last_op.position,
                        _ => false,
                    };

                    if !is_contiguous {
                        return self.start_new_transaction(operation, now);
                    }
                }

                false
            }
            None => true,
        };

        if should_start_new {
            self.start_new_transaction(operation, now);
        } else {
            // Add to current transaction and update timestamp
            if let Some(tx) = &mut self.current_transaction {
                tx.operations.push(operation);
                tx.timestamp = now; // Update timestamp for continued grouping
            }
        }

        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Helper to start a new transaction
    fn start_new_transaction(&mut self, operation: EditOperation, timestamp: Instant) {
        // Finalize current transaction if exists
        self.finalize_transaction();
        // Start new transaction
        self.current_transaction = Some(EditTransaction {
            operations: vec![operation],
            timestamp,
        });
        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Finalize the current transaction and push to undo stack
    pub fn finalize_transaction(&mut self) {
        if let Some(tx) = self.current_transaction.take() {
            if !tx.is_empty() {
                self.undo_stack.push(tx);
                // Trim history if needed
                while self.undo_stack.len() > self.max_history_size {
                    self.undo_stack.remove(0);
                }
            }
        }
    }

    /// Pop a transaction from the undo stack for undoing
    pub fn pop_undo(&mut self) -> Option<EditTransaction> {
        // First finalize any pending transaction
        self.finalize_transaction();
        self.undo_stack.pop()
    }

    /// Push a transaction to the redo stack
    pub fn push_redo(&mut self, transaction: EditTransaction) {
        self.redo_stack.push(transaction);
    }

    /// Pop a transaction from the redo stack for redoing
    pub fn pop_redo(&mut self) -> Option<EditTransaction> {
        self.redo_stack.pop()
    }

    /// Push a transaction to the undo stack (used when redoing)
    pub fn push_undo(&mut self, transaction: EditTransaction) {
        self.undo_stack.push(transaction);
        // Trim history if needed
        while self.undo_stack.len() > self.max_history_size {
            self.undo_stack.remove(0);
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
            || self
                .current_transaction
                .as_ref()
                .is_some_and(|tx| !tx.is_empty())
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_transaction = None;
    }
}
