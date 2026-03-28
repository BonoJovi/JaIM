//! IBus integration layer
//!
//! Implements the org.freedesktop.IBus.Engine D-Bus interface
//! using the zbus crate for pure-Rust D-Bus communication.
//!
//! Handles:
//! - Key event interception (ProcessKeyEvent)
//! - Preedit text display (UpdatePreeditText)
//! - Candidate list display (UpdateLookupTable)
//! - Text commit (CommitText)
//! - Focus/reset lifecycle

// TODO: Implement IBus D-Bus interface via zbus
