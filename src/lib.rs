//! An abstract execution machine that's a hybrid between a register-based and stack-based
//! architecture.
//!
//! The stack-based architecture with immutability of pushed values allows for easier reasoning
//! about information flow and data dependencies and for easier security analysis / verification.
//! Each push creates a new value that cannot be modified, ensuring that data flows
//! are explicit and traceable.
//!
//! Values can still be cleaned up by using the Pop instruction
//!
//! The actual machine is in the [`machine`] module and its submodules.
//!
//! The instruction set can be found in the [`instruction`] module, along with
//! the [`add_instr!`](crate::add_instr) and [`make_block!`](crate::make_block)
//! helpers for building programs.

pub mod information_flow;
pub mod instruction;
pub mod machine;
pub mod types;
