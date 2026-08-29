use log::debug;
use std::{fmt::Debug, rc::Rc};

use crate::{
    instruction::{
        BinaryOp,
        Instruction::{self},
        NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
    },
    types::{
        CellAmount, CellIndex, FdEntry, FunctionData, FunctionDataError, Immediate, Input, Output,
        ProgramData, ProgramDataError,
    },
};

pub mod executor;
pub mod verifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    FunctionDataError(FunctionDataError),
    ProgramDataError(ProgramDataError),
    /// When there isn't enough values on stack for Block to copy.
    NotEnoughArguments { required: usize, available: usize },
    /// When there are problems with the buffer
    IoReadError,
}

impl From<FunctionDataError> for CoreError {
    fn from(err: FunctionDataError) -> Self {
        CoreError::FunctionDataError(err)
    }
}

impl From<ProgramDataError> for CoreError {
    fn from(err: ProgramDataError) -> Self {
        CoreError::ProgramDataError(err)
    }
}

type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone)]
pub struct CoreMachine<Tag = ()> {
    function_data: FunctionData<Tag>,
    downgrader_data: FunctionData<Tag>,
    program_data: ProgramData<Tag>,
    output: Output,
    input: Input,
}

impl<Tag: Clone + Debug> CoreMachine<Tag> {
    pub fn new(program: impl Into<Rc<[Instruction<Tag>]>>) -> Self {
        Self {
            function_data: FunctionData::default(),
            downgrader_data: FunctionData::default(),
            program_data: ProgramData::new(program),
            output: Output::Stdout,
            input: Input::Stdin,
        }
    }

    pub fn function_get(&self, name: &str) -> CoreResult<&Instruction<Tag>> {
        self.function_data.get(name).map_err(Into::into)
    }

    pub fn function_insert(&mut self, name: String, entry: FdEntry<Tag>) -> CoreResult<()> {
        self.function_data.insert(name, entry)?;
        Ok(())
    }

    pub fn downgrader_get(&self, name: &str) -> CoreResult<&Instruction<Tag>> {
        self.downgrader_data.get(name).map_err(Into::into)
    }

    pub fn downgrader_insert(&mut self, name: String, entry: FdEntry<Tag>) -> CoreResult<()> {
        self.downgrader_data.insert(name, entry)?;
        Ok(())
    }

    pub fn common_function_logic(&mut self, function_name: &str) -> CoreResult<()> {
        self.common_definition_logic(function_name, false)
    }

    pub fn common_downgrader_logic(&mut self, function_name: &str) -> CoreResult<()> {
        self.common_definition_logic(function_name, true)
    }

    fn common_definition_logic(
        &mut self,
        function_name: &str,
        is_downgrader: bool,
    ) -> CoreResult<()> {
        let Some(current @ Instruction::Block(_, _)) = self.next() else {
            return Err(FunctionDataError::FunctionMissingBody(function_name.to_owned()).into());
        };

        let function_name = function_name.to_owned();
        debug!("Function '{}' will point to {:?}", function_name, current);
        if is_downgrader {
            self.downgrader_insert(function_name, FdEntry::Inst(current))?;
        } else {
            self.function_insert(function_name, FdEntry::Inst(current))?;
        }

        Ok(())
    }
}

impl<Tag: Clone + Debug> Iterator for CoreMachine<Tag> {
    type Item = Instruction<Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        self.program_data.next()
    }
}

/// Dispatches instructions
trait Evaluate<Tag: Debug = ()> {
    type Error;

    fn evaluate_instruction(&mut self, instr: &Instruction<Tag>) -> Result<(), Self::Error> {
        use Instruction::*;
        use log::debug;

        match instr {
            Nullary(instr) => {
                debug!("Evaling: {:?}", instr);
                self.evaluate_alu_nullary(instr)
            }
            UnaryImm(instr, imm) => {
                debug!("Evaling: {:?}, imm: {:?}", instr, imm);
                self.evaluate_alu_unary_imm(instr, *imm)
            }
            UnaryCell(instr, cell) => {
                debug!("Evaling: {:?}, cell: {:?}", instr, cell);
                self.evaluate_alu_unary_cell(instr, *cell)
            }
            UnaryCellAmnt(instr, amount) => {
                debug!("Evaling: {:?}, amount: {:?}", instr, amount);
                self.evaluate_alu_unary_cell_amnt(instr, *amount)
            }
            UnaryString(instr, name) => {
                debug!("Evaling: {:?}, name: '{}'", instr, name);
                self.evaluate_alu_unary_string(instr, name)
            }
            Binary(instr, arg1, arg2) => {
                debug!("Evaling: {:?}; args: {:?}, {:?}", instr, arg1, arg2);
                self.evaluate_alu_binary(instr, *arg1, *arg2)
            }
            Block(argument_count, instrs) => {
                debug!("Entering block with argument count {argument_count}...");
                self.evaluate_block(*argument_count, instrs.clone())
            }
            IfElse(cond_idx, when_true, when_false) => {
                debug!("Entering if-else block...");
                self.evaluate_ifelse(*cond_idx, when_true.clone(), when_false.clone()) // Cheap clone
            }
        }?;

        Ok(())
    }

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm<Tag>,
        arg: Immediate,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_cell_amnt(
        &mut self,
        instr: &UnaryOpCellAmnt,
        amount: CellAmount,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_unary_string(
        &mut self,
        instr: &UnaryOpString,
        name: &str,
    ) -> Result<(), Self::Error>;
    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> Result<(), Self::Error>;
    fn evaluate_block(
        &mut self,
        argument_count: CellAmount,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> Result<(), Self::Error>;
    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<[Instruction<Tag>]>,
        when_false: Rc<[Instruction<Tag>]>,
    ) -> Result<(), Self::Error>;
}

// =============================================================================
// Shared frame-stack mechanics used by both Executor and Verifier.
//
// Every frame owns the hidden caller stack while the body runs solely on cloned
// argument cells. On exit, locals are discarded and the caller is restored
// unchanged.
// =============================================================================

#[derive(Clone, Debug)]
pub struct Frame<V, T> {
    caller: Vec<Cell<V, T>>,
}

#[derive(Clone, Debug)]
pub struct Stack<V, T> {
    cells: Vec<Cell<V, T>>,
    frames: Vec<Frame<V, T>>,
}

impl<V: Clone, T: Clone> Default for Stack<V, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone, T: Clone> Stack<V, T> {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            frames: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, value: Cell<V, T>) {
        self.cells.push(value);
    }

    /// Pops the top cell from the current stack context.
    pub fn pop(&mut self) -> Option<Cell<V, T>> {
        self.cells.pop()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<&Cell<V, T>> {
        self.cells.get(idx)
    }

    /// Enters a block with the declared argument count.
    pub fn enter_block(&mut self, count: CellAmount) -> CoreResult<()> {
        let required = usize::from(count);
        let available = self.cells.len();
        if required > available {
            return Err(CoreError::NotEnoughArguments {
                required,
                available,
            });
        }

        let arguments = self.cells[available - required..].to_vec();
        let caller = std::mem::replace(&mut self.cells, arguments);
        self.frames.push(Frame { caller });
        Ok(())
    }

    /// Enters an isolated block with explicit cells instead of cloning them
    /// from the caller. Used by definition-time abstract analysis, where the
    /// declared parameters are represented by synthetic cells.
    pub fn enter_block_with_arguments(&mut self, arguments: Vec<Cell<V, T>>) {
        let caller = std::mem::replace(&mut self.cells, arguments);
        self.frames.push(Frame { caller });
    }

    /// Ends the current block, restores its caller environment, and returns
    /// `(last_cell_at_end_of_body, body_stack_size)`.
    pub fn exit_block(&mut self) -> (Option<Cell<V, T>>, usize) {
        let Frame { caller } = self.frames.pop().expect("exit_block: no frame");
        let body_stack_size = self.cells.len();
        let result = self.cells.last().cloned();
        self.cells = caller;
        (result, body_stack_size)
    }

    /// A "getter" method for cells' length
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// A "getter" method for cells' is_empty method
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

impl<V: Copy, T: Copy> Stack<V, T> {
    /// Values only, cloned out for inspection and error reporting.
    pub fn values(&self) -> Vec<V> {
        self.cells.iter().map(|slot| slot.value).collect()
    }

    /// Value of the top cell, if any.
    pub fn last_value(&self) -> Option<V> {
        self.cells.last().map(|slot| slot.value)
    }

    /// Tags only, cloned out for inspection and error reporting.
    pub fn tags(&self) -> Vec<T> {
        self.cells.iter().map(|slot| slot.tag).collect()
    }

    /// Tag of the top cell, if any.
    pub fn last_tag(&self) -> Option<T> {
        self.cells.last().map(|slot| slot.tag)
    }

    /// Value at `index`, if it exists.
    pub fn value_at(&self, index: usize) -> Option<V> {
        self.cells.get(index).map(|slot| slot.value)
    }

    /// Tag at `index`, if it exists.
    pub fn tag_at(&self, index: usize) -> Option<T> {
        self.cells.get(index).map(|slot| slot.tag)
    }

    /// Read-only view of the underlying slots.
    pub fn slots(&self) -> &[Cell<V, T>] {
        &self.cells
    }

    /// Replaces the underlying slots, returning the previous contents.
    pub fn replace_slots(&mut self, slots: Vec<Cell<V, T>>) -> Vec<Cell<V, T>> {
        std::mem::replace(&mut self.cells, slots)
    }

    /// Takes the underlying slots, leaving an empty stack behind.
    pub fn take_slots(&mut self) -> Vec<Cell<V, T>> {
        std::mem::take(&mut self.cells)
    }

    /// Overwrites the underlying slots.
    pub fn set_slots(&mut self, slots: Vec<Cell<V, T>>) {
        self.cells = slots;
    }
}

/// Converts a `ReadReverse` offset into a normal cell index, like Python's
/// negative indexing: offset 0 is the top cell. Returns `None` if the offset
/// reaches below the bottom of the stack. Used by both runners.
fn reverse_index(stack_len: usize, reverse_offset: CellIndex) -> Option<CellIndex> {
    let last_index = CellIndex::try_from(stack_len).ok()?.checked_sub(1)?;
    last_index.checked_sub(reverse_offset)
}

/// A single stack cell, containing a value and a tag.
#[derive(Clone, Debug)]
pub struct Cell<V, T> {
    value: V,
    tag: T,
}

impl<V, T> Cell<V, T> {
    pub fn new(value: V, tag: T) -> Self {
        Self { value, tag }
    }
}
