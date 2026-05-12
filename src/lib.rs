/*
 * A virtual machine that's a hybrid between a register-based and stack-based
 * architecture.
 *
 * Registers (referred to as "Cells") are temporarily represented as i64 value.
 * These "registers" are referred to as "cells" in the code.
 *
 * It has a push instruction - it places a value to the next available cell.
 * This is similar to Single Static Assignment (SSA) form in compilers.
 *
 * Pop doesn't have to exist for reading purposes, as we can read directly from
 * available cell.
 *
 * However, pop can be used to free up cells when needed.
 */

// mod verificator;
pub mod machine;
pub mod instruction;
pub mod types;
pub mod logging;

#[macro_use]
mod macros;
