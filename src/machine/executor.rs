//! This module is the implementation of the executor. Function calls are executed.
//!
//! For the "entry point", see [`Executor::exec`].

use std::{
    collections::HashSet,
    fmt::{Debug, Display},
    rc::Rc,
};

use log::debug;

use crate::{
    information_flow::{FlowError, FlowTag, InformationFlowPolicy, NoFlow},
    instruction::{
        BinaryOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
    },
    machine::{CoreError, CoreMachine, Evaluate, Cell, Stack},
    types::{self, Value, CellIndex, Immediate, IoBuffer, ProgramData},
};
use Value::*;
use ExecutorError::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError<Tag = ()>
where
    Tag: FlowTag,
{
    DivisionByZero,
    ArithmeticOverflow,
    StackUnderflow,
    InvalidCell,
    BlockHasEmptyStack,
    EmptyBlock,
    Core(CoreError),
    Flow(FlowError<Tag>),
    DebugError(&'static str),
}

impl<Tag: FlowTag> From<CoreError> for ExecutorError<Tag> {
    fn from(error: CoreError) -> Self {
        ExecutorError::Core(error)
    }
}

impl<Tag: FlowTag> From<FlowError<Tag>> for ExecutorError<Tag> {
    fn from(error: FlowError<Tag>) -> Self {
        ExecutorError::Flow(error)
    }
}

type ExecutorResult<T, P> = Result<T, ExecutorError<<P as InformationFlowPolicy>::Tag>>;

const RECURSION_LIMIT: usize = 50;

/// Per-call state for the downgrader currently executing. Downgraders are never
/// re-entrant, so at most one is active at a time. Drives the re-entrancy guard
/// and per-argument-cell budget counting; the implicit retag of the return
/// value is applied directly in `call_function` from the policy.
struct ActiveDowngrader {
    name: String,
    max_calls: Option<usize>,
    /// Caller stack length on entry: reads below this index touch arguments.
    base: usize,
    /// `function_depth` of the downgrader body; its own `Rebase` (same depth)
    /// closes the argument-counting window.
    depth: usize,
    /// Whether argument reads still count (true until the body rebases).
    counting: bool,
    /// Distinct caller cells already counted for this call.
    counted: HashSet<usize>,
}

pub struct Executor<P: InformationFlowPolicy = NoFlow> {
    machine: CoreMachine<P::Tag>,
    stack: Stack<Value, P::Tag>,
    policy: P,
    pc_tag: P::Tag,
    function_depth: usize,
    /// The downgrader whose body is currently executing, if any.
    active_downgrader: Option<ActiveDowngrader>,
}

impl Display for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Executor {:?}]", self.stack.last_value())
    }
}

impl Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor")
            .field("machine", &self.machine)
            .field("stack", &self.stack)
            .field("policy", &self.policy)
            .field("pc_tag", &self.pc_tag)
            .field("function_depth", &self.function_depth)
            .finish()
    }
}

/// An executor with no IFT
impl Executor<NoFlow> {
    /// Creates an ordinary executor with information-flow monitoring disabled.
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy: NoFlow,
            pc_tag: (),
            function_depth: 0,
            active_downgrader: None,
        }
    }
}

impl<P: InformationFlowPolicy> Executor<P> {
    /// Creates a monitored executor and validates every tag embedded in the program.
    pub fn with_policy(
        program: impl Into<Rc<[Instruction<P::Tag>]>>,
        policy: P,
    ) -> ExecutorResult<Self, P> {
        let program = program.into();
        Self::validate_program(&program, &policy)?;
        let pc_tag = policy.default_tag();
        Ok(Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy,
            pc_tag,
            function_depth: 0,
            active_downgrader: None,
        })
    }

    fn validate_program(program: &[Instruction<P::Tag>], policy: &P) -> ExecutorResult<(), P> {
        for instruction in program {
            Self::validate_instruction(instruction, policy)?;
        }
        Ok(())
    }

    fn validate_instruction(
        instruction: &Instruction<P::Tag>,
        policy: &P,
    ) -> ExecutorResult<(), P> {
        match instruction {
            Instruction::AluUnaryImm(UnaryOpImm::TaggedPush(tag), _) => {
                policy.validate_tag(*tag)?
            }
            Instruction::Block(body) => Self::validate_program(body, policy)?,
            Instruction::IfElse(_, when_true, when_false) => {
                Self::validate_instruction(when_true, policy)?;
                Self::validate_instruction(when_false, policy)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Redirects the source used by input instructions.
    pub fn redirect_input(mut self, new_input: types::Input) -> Self {
        self.machine.input = new_input;
        self
    }

    /// Redirects the destination used by print instructions.
    pub fn redirect_output(mut self, new_output: types::Output) -> Self {
        self.machine.output = new_output;
        self
    }

    /// Reads a value cell without exposing its tag. Does not affect downgrade
    /// accounting (intended for external inspection).
    pub fn read(&self, index: CellIndex) -> ExecutorResult<Value, P> {
        self.stack
            .get(index.into())
            .map(|s| s.value)
            .ok_or(InvalidCell)
    }

    /// Reads the tag corresponding to a value cell.
    pub fn read_tag(&self, index: CellIndex) -> ExecutorResult<P::Tag, P> {
        self.stack
            .get(index.into())
            .map(|s| s.tag)
            .ok_or(InvalidCell)
    }

    /// Returns the value cells, cloned out for inspection.
    pub fn values(&self) -> Vec<Value> {
        self.stack.values()
    }

    /// Returns the parallel tag stack.
    pub fn tags(&self) -> Vec<P::Tag> {
        self.stack.tags()
    }

    /// Returns the tag of the top value cell.
    pub fn last_tag(&self) -> Option<P::Tag> {
        self.stack.last_tag()
    }

    /// Reads value and tag at the given index. While a downgrader body runs, a
    /// read of one of its caller's argument cells is counted against that
    /// cell's per-downgrader budget.
    fn read_entry(&mut self, index: CellIndex) -> ExecutorResult<(Value, P::Tag), P> {
        let abs = usize::from(index);
        self.note_downgrade_arg(abs)?;
        self.stack
            .get(abs)
            .map(|s| (s.value, s.tag))
            .ok_or(InvalidCell)
    }

    /// Counts a downgrader argument read against the caller cell at `abs`, once
    /// per call per distinct cell, enforcing the per-value `max_calls` budget.
    fn note_downgrade_arg(&mut self, abs: usize) -> ExecutorResult<(), P> {
        let depth = self.function_depth;
        let Some(active) = self.active_downgrader.as_mut() else {
            return Ok(());
        };
        // Only count reads performed directly by the downgrader body (not by
        // functions it calls), of caller arguments, before the body rebases,
        // once per distinct cell.
        if depth != active.depth
            || !active.counting
            || abs >= active.base
            || !active.counted.insert(abs)
        {
            return Ok(());
        }
        let name = active.name.clone();
        let max_calls = active.max_calls;
        // let count = self.stack.bump_count(abs, &name); TODO:
        // if let Some(limit) = max_calls {
        //     if count > limit {
        //         return Err(FlowError::DowngraderCallLimitExceeded {
        //             downgrader: name,
        //             limit,
        //         }
        //         .into());
        //     }
        // }
        Ok(())
    }

    /// Calculates ccd(left, right)
    fn combine_tags(&self, left: P::Tag, right: P::Tag) -> ExecutorResult<P::Tag, P> {
        self.policy
            .closest_common_descendant(left, right)
            .map_err(Into::into)
    }

    /// Pushes a newly-created value, including the current control-flow tag.
    fn push_new_value(&mut self, value: Value, tag: P::Tag) -> ExecutorResult<(), P> {
        let effective_tag = self.combine_tags(tag, self.pc_tag)?;
        self.stack.push(Cell::new(value, effective_tag));
        Ok(())
    }

    /// Restores a result that already carries its final effective tag.
    fn push_existing_entry(&mut self, entry: (Value, P::Tag)) {
        self.stack.push(Cell::new(entry.0, entry.1));
    }

    fn run(&mut self) -> ExecutorResult<(), P> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
        }
        Ok(())
    }

    /// Executes the program to completion, returning self.
    pub fn exec(mut self) -> ExecutorResult<Self, P> {
        self.run()?;
        Ok(self)
    }

    /// Runs `instrs` as a nested context (block / function body / ifelse branch).
    ///
    /// Saves and restores `program_data` while [`StackFrames`] scopes the value
    /// and tag cells together.
    fn run_nested(
        &mut self,
        instrs: Rc<[Instruction<P::Tag>]>,
    ) -> ExecutorResult<Option<(Value, P::Tag)>, P> {
        let saved_bases = self.stack.enter_block();
        let saved_program =
            std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let run_result = self.run();

        self.machine.program_data = saved_program;
        let (slot, _) = self.stack.exit_block(saved_bases);
        let result = slot.map(|s| (s.value, s.tag));

        run_result?;
        Ok(result)
    }

    /// Runs the body of an ifelse branch *inline* on the parent stack: cells
    /// are not isolated, so pops and pushes persist after the branch ends.
    /// `program_data` is still scoped. The marker frame pushed via
    /// [`StackFrames::enter_ifelse_branch`] makes `Rebase` an error inside the
    /// branch.
    fn run_ifelse_branch(
        &mut self,
        instrs: Rc<[Instruction<P::Tag>]>,
        condition_tag: P::Tag,
    ) -> ExecutorResult<(), P> {
        let branch_pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;
        self.stack.enter_ifelse_branch();
        let saved_program =
            std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));
        let saved_pc_tag = self.pc_tag;
        self.pc_tag = branch_pc_tag;

        let run_result = self.run();

        self.machine.program_data = saved_program;
        self.pc_tag = saved_pc_tag;
        self.stack.exit_ifelse_branch();

        run_result
    }

    /// Calls `function_name`. `is_downgrade` distinguishes a `Downgrade`
    /// instruction (which applies the implicit retag and per-value budget) from
    /// an ordinary `FunctionCall`. The instruction and the policy registration
    /// must agree: a `Downgrade` of an unregistered name, or a `FunctionCall` of
    /// a registered downgrader, is rejected.
    fn call_function(&mut self, function_name: &str, is_downgrade: bool) -> ExecutorResult<(), P> {
        let body = self.machine.function_get(function_name)?.clone();

        // The instruction declares intent; the policy declares the connection.
        // Cross-check the two so downgrade sites are unambiguous. `Downgrader`
        // is `Copy`, so the lookup releases the `&self.policy` borrow at once.
        let downgrader = match (is_downgrade, self.policy.downgrader(function_name)) {
            (true, None) => {
                return Err(FlowError::NotADowngrader {
                    name: function_name.to_owned(),
                }
                .into());
            }
            (false, Some(_)) => {
                return Err(FlowError::DowngraderUsedAsFunction {
                    name: function_name.to_owned(),
                }
                .into());
            }
            (true, Some(downgrader)) => Some(downgrader),
            (false, None) => None,
        };

        // Downgraders are never re-entrant: a downgrader body may not invoke any
        // downgrader while one is already running.
        if downgrader.is_some() && self.active_downgrader.is_some() {
            return Err(FlowError::RecursiveDowngrader {
                downgrader: function_name.to_owned(),
            }
            .into());
        }

        self.function_depth += 1;
        if self.function_depth > RECURSION_LIMIT {
            panic!("Recursion limit of {RECURSION_LIMIT} exceeded in function '{function_name}'");
        }

        // Install the downgrader context (if any) for the duration of the body.
        // `base` is the caller stack height: reads below it touch arguments.
        if let Some(downgrader) = downgrader {
            self.active_downgrader = Some(ActiveDowngrader {
                name: function_name.to_owned(),
                max_calls: downgrader.max_calls,
                base: self.stack.len(),
                depth: self.function_depth,
                counting: true,
                counted: HashSet::new(),
            });
        }

        let result = self.run_nested(Rc::from(vec![body]));

        if downgrader.is_some() {
            self.active_downgrader = None;
        }
        self.function_depth -= 1;

        if let Some((value, tag)) = result? {
            match downgrader {
                // Implicit retag: the body's return value must already carry the
                // connection `source`; it is then forced to `target`, bypassing
                // the `pc_tag` join applied to ordinary values.
                Some(downgrader) => {
                    let connection = downgrader.connection;
                    if tag != connection.source {
                        return Err(FlowError::DowngraderReturnTagMismatch {
                            found: tag,
                            expected: connection.source,
                        }
                        .into());
                    }
                    self.stack.push(Cell::new(value, connection.target));
                }
                None => self.push_existing_entry((value, tag)),
            }
        }
        Ok(())
    }

    fn print_cell(&mut self, index: CellIndex) -> ExecutorResult<(), P> {
        let (value, value_tag) = self.read_entry(index)?;
        self.ensure_output_allowed(value_tag)?;

        match &mut self.machine.output {
            types::Output::Stdout => print!("{value}"),
            types::Output::File(_) | types::Output::Buffer(_) => {
                let immediate = value.into_immediate().map_err(|_| InvalidCell)?;
                self.machine.output.write(&[immediate]);
            }
        }
        Ok(())
    }

    fn ensure_output_allowed(&self, value_tag: P::Tag) -> ExecutorResult<(), P> {
        let effective_tag = self.combine_tags(value_tag, self.pc_tag)?;
        let output_guard = self.policy.output_tag();

        if self.policy.can_flow(effective_tag, output_guard)? {
            Ok(())
        } else {
            Err(FlowError::InformationFlowViolation {
                found: effective_tag,
                guard: output_guard,
            }
            .into())
        }
    }

    fn read_input_value(&mut self) -> ExecutorResult<Immediate, P> {
        use types::Input;

        let value = match &self.machine.input {
            Input::Stdin => {
                let mut input = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .expect("Failed to read input");
                match input.trim().parse() {
                    Ok(value) => value,
                    Err(error) => todo!("For now, invalid input is a fatal error: {error}"),
                }
            }
            Input::File(_) => {
                let mut data = self.machine.input.read_all();
                let value = data.pop().ok_or(Core(CoreError::IoReadError))?;
                self.machine.input = IoBuffer::new(data).into();
                value
            }
            Input::Buffer(buffer) => buffer
                .borrow_mut()
                .pop()
                .ok_or(Core(CoreError::IoReadError))?,
        };

        Ok(value)
    }
}

impl<P: InformationFlowPolicy> Evaluate<P::Tag> for Executor<P> {
    type Error = ExecutorError<P::Tag>;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> ExecutorResult<(), P> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => {
                self.stack.rebase()?;
                // A downgrader's own rebase closes its argument-counting window:
                // afterwards the caller cells are gone and low indices are
                // body-local locals that must not count as downgrades.
                let depth = self.function_depth;
                if let Some(active) = self.active_downgrader.as_mut() {
                    if depth == active.depth {
                        active.counting = false;
                    }
                }
            }
            Input => {
                let value = self.read_input_value()?;
                self.push_new_value(Integer(value), self.policy.input_tag())?;
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm<P::Tag>,
        arg: Immediate,
    ) -> ExecutorResult<(), P> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push_new_value(Integer(arg), self.policy.default_tag())?,
            TaggedPush(tag) => self.push_new_value(Integer(arg), *tag)?,
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> ExecutorResult<(), P> {
        use UnaryOpCell::*;

        match instr {
            Not => {
                let (val, tag) = self.read_entry(arg)?;
                if let Integer(val) = val {
                    self.push_new_value(Integer(!val), tag)?;
                } else {
                    todo!("This will eventually be a TypeError, in case types get implemented.")
                }
            }
            Read => {
                let (val, tag) = self.read_entry(arg)?;
                self.push_new_value(val, tag)?;
            }
            ReadReverse => {
                let index = reverse_index(self.stack.len(), arg).ok_or(InvalidCell)?;
                let (val, tag) = self.read_entry(index)?;
                self.push_new_value(val, tag)?;
            }
            Print => self.print_cell(arg)?,
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell_amnt(
        &mut self,
        instr: &UnaryOpCellAmnt,
        amount: CellIndex,
    ) -> ExecutorResult<(), P> {
        use UnaryOpCellAmnt::*;

        match instr {
            Pop => {
                for _ in 0..amount {
                    self.stack.pop().ok_or(StackUnderflow)?;
                }
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_string(
        &mut self,
        instr: &UnaryOpString,
        name: &str,
    ) -> ExecutorResult<(), P> {
        use UnaryOpString::*;
        use types::{Input, Output};

        match instr {
            // The body is registered identically for both; the downgrade
            // semantics live entirely at the call site (implicit retag, budget).
            // The instruction's intent must still agree with the policy so a
            // downgrade gate is never defined as an ordinary function.
            FunctionDefine => {
                if self.policy.downgrader(name).is_some() {
                    return Err(FlowError::DowngraderUsedAsFunction {
                        name: name.to_owned(),
                    }
                    .into());
                }
                _ = self.machine.common_function_logic(name)?
            }
            Downgrader => {
                if self.policy.downgrader(name).is_none() {
                    return Err(FlowError::NotADowngrader {
                        name: name.to_owned(),
                    }
                    .into());
                }
                _ = self.machine.common_function_logic(name)?
            }
            FunctionCall => self.call_function(name, false)?,
            Downgrade => self.call_function(name, true)?,
            FileRead => self.machine.input = Input::from_path(name),
            FileWrite => self.machine.output = Output::from_path(name),
        }

        Ok(())
    }

    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> ExecutorResult<(), P> {
        let (left, left_tag) = self.read_entry(arg1)?;
        let (right, right_tag) = self.read_entry(arg2)?;
        let result_tag = self.combine_tags(left_tag, right_tag)?;

        debug!("Evaluating binary: {:?} {:?} {:?}", left, instr, right);

        let expect_integer = |cell: Value| -> ExecutorResult<i64, P> {match cell {
            Integer(value) => Ok(value),
            _found => todo!("This will eventually be a TypeError, in case types get implemented.")

        }};

        let left = expect_integer(left)?;
        let right = expect_integer(right)?;
        let result = {
            use BinaryOp::*;

            let from_bool = |value: bool| Immediate::from(value);

            match instr {
                Add => left.checked_add(right).ok_or(ArithmeticOverflow),
                Mul => left.checked_mul(right).ok_or(ArithmeticOverflow),
                Div => left.checked_div(right).ok_or(DivisionByZero),
                And => Ok(left & right),
                Or => Ok(left | right),
                Xor => Ok(left ^ right),
                ShiftLeftLogical => Ok(left << right),
                ShiftRightLogical => Ok(((left as u64) >> right) as i64),
                ShiftRightArithmetic => Ok(left >> right),
                SetEqual => Ok(from_bool(left == right)),
                SetNotEqual => Ok(from_bool(left != right)),
                SetLessThan => Ok(from_bool(left < right)),
                SetLessThanOrEqual => Ok(from_bool(left <= right)),
                SetGreaterThan => Ok(from_bool(left > right)),
                SetGreaterThanOrEqual => Ok(from_bool(left >= right)),
            }
        }?;
        self.push_new_value(Integer(result), result_tag)?;

        Ok(())
    }

    fn evaluate_block(&mut self, instrs: Rc<[Instruction<P::Tag>]>) -> ExecutorResult<(), P> {
        // Each block must leave at least one value on its local stack so the parent can
        // observe a result. A block that ends with an empty local stack is a "void" error.
        if instrs.is_empty() {
            return Err(EmptyBlock);
        }
        match self.run_nested(instrs)? {
            Some(entry) => self.push_existing_entry(entry),
            None => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction<P::Tag>>,
        when_false: Rc<Instruction<P::Tag>>,
    ) -> ExecutorResult<(), P> {
        let (condition, condition_tag) = self.read_entry(cond_idx)?;

        let branch = match condition {
            Integer(0) => when_false,
            Integer(_) => when_true,
            _other => todo!("This will eventually be a TypeError, in case types get implemented.")
        };

        let branch_program = Rc::from(vec![branch.as_ref().clone()]);
        self.run_ifelse_branch(branch_program, condition_tag)
    }
}

fn reverse_index(stack_len: usize, reverse_offset: CellIndex) -> Option<CellIndex> {
    let last_index = CellIndex::try_from(stack_len).ok()?.checked_sub(1)?;
    last_index.checked_sub(reverse_offset)
}

impl From<Vec<Value>> for Executor {
    fn from(value: Vec<Value>) -> Self {
        let mut machine = Self::new(Vec::<Instruction>::new());
        machine.stack.set_values_for_unmonitored(value, ());
        machine
    }
}
