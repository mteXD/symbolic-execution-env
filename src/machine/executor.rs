//! This module is the implementation of the executor. Function calls are executed.
//!
//! For the "entry point", see [`Executor::exec`].

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    rc::Rc,
};

use log::debug;

use crate::{
    information_flow::{FlowError, SecurityPolicy, TagTrait, validate_program_tags},
    instruction::{
        BinaryOp, Instruction, NullaryOp, UnaryOpCell, UnaryOpCellAmnt, UnaryOpImm, UnaryOpString,
    },
    machine::{Cell, CoreError, CoreMachine, Evaluate, Stack, reverse_index},
    types::{self, CellAmount, CellIndex, Immediate, ProgramData, Value},
};
use ExecutorError::*;
use Value::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError<Tag = ()>
where
    Tag: TagTrait,
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

impl<Tag: TagTrait> From<CoreError> for ExecutorError<Tag> {
    fn from(error: CoreError) -> Self {
        ExecutorError::Core(error)
    }
}

impl<Tag: TagTrait> From<FlowError<Tag>> for ExecutorError<Tag> {
    fn from(error: FlowError<Tag>) -> Self {
        ExecutorError::Flow(error)
    }
}

type ExecutorResult<T, Tag> = Result<T, ExecutorError<Tag>>;

const RECURSION_LIMIT: usize = 50;

pub struct Executor<Tag: TagTrait = ()> {
    machine: CoreMachine<Tag>,
    stack: Stack<Value, Tag>,
    policy: SecurityPolicy<Tag>,
    pc_tag: Tag,
    function_depth: usize,
    /// How many times each downgrader has been called during this run.
    downgrader_calls: HashMap<String, usize>,
}

impl<Tag: TagTrait> Display for Executor<Tag> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Executor {:?}]", self.stack.last_value())
    }
}

impl<Tag: TagTrait> Debug for Executor<Tag> {
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
impl Executor<()> {
    /// Creates an ordinary executor with information-flow monitoring disabled.
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy: SecurityPolicy::no_flow(),
            pc_tag: (),
            function_depth: 0,
            downgrader_calls: HashMap::new(),
        }
    }
}

impl<Tag: TagTrait> Executor<Tag> {
    /// Creates a monitored executor and validates every tag embedded in the program.
    pub fn with_policy(
        program: impl Into<Rc<[Instruction<Tag>]>>,
        policy: SecurityPolicy<Tag>,
    ) -> ExecutorResult<Self, Tag> {
        let program = program.into();
        validate_program_tags(&program, &policy)?;
        let pc_tag = policy.default_tag();
        Ok(Self {
            machine: CoreMachine::new(program),
            stack: Stack::new(),
            policy,
            pc_tag,
            function_depth: 0,
            downgrader_calls: HashMap::new(),
        })
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
    pub fn read(&self, index: CellIndex) -> ExecutorResult<Value, Tag> {
        self.stack
            .get(index.into())
            .map(|s| s.value)
            .ok_or(InvalidCell)
    }

    /// Reads the tag corresponding to a value cell.
    pub fn read_tag(&self, index: CellIndex) -> ExecutorResult<Tag, Tag> {
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
    pub fn tags(&self) -> Vec<Tag> {
        self.stack.tags()
    }

    /// Reads value and tag at the given index.
    fn read_entry(&self, index: CellIndex) -> ExecutorResult<(Value, Tag), Tag> {
        self.stack
            .get(usize::from(index))
            .map(|s| (s.value, s.tag))
            .ok_or(InvalidCell)
    }

    /// Calculates ccd(left, right)
    fn combine_tags(&self, left: Tag, right: Tag) -> ExecutorResult<Tag, Tag> {
        self.policy.ccd(left, right).map_err(Into::into)
    }

    /// Pushes a newly-created value, including the current control-flow tag.
    fn push_new_value(&mut self, value: Value, tag: Tag) -> ExecutorResult<(), Tag> {
        let effective_tag = self.combine_tags(tag, self.pc_tag)?;
        self.stack.push(Cell::new(value, effective_tag));
        Ok(())
    }

    /// Restores a result that already carries its final effective tag.
    fn push_existing_entry(&mut self, entry: (Value, Tag)) {
        self.stack.push(Cell::new(entry.0, entry.1));
    }

    fn run(&mut self) -> ExecutorResult<(), Tag> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
        }
        Ok(())
    }

    /// Executes the program to completion, returning self.
    pub fn exec(mut self) -> ExecutorResult<Self, Tag> {
        self.run()?;
        Ok(self)
    }

    /// Runs `instrs` as a counted block context.
    ///
    /// The count clones the ordered caller suffix and isolates it before the
    /// first instruction. Program and stack state are restored even when body
    /// execution fails.
    fn run_nested(
        &mut self,
        argument_count: CellAmount,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> ExecutorResult<Option<(Value, Tag)>, Tag> {
        self.stack.enter_block(argument_count)?;
        let saved_program =
            std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let run_result = self.run();

        self.machine.program_data = saved_program;
        let (slot, _) = self.stack.exit_block();
        let result = slot.map(|s| (s.value, s.tag));

        run_result?;
        Ok(result)
    }

    /// Runs one ifelse branch inline on the current stack. A branch instruction
    /// that is itself a `Block` applies its declared isolated argument scope.
    fn run_ifelse_branch(
        &mut self,
        instrs: Rc<[Instruction<Tag>]>,
        condition_tag: Tag,
    ) -> ExecutorResult<(), Tag> {
        let branch_pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;
        let saved_program =
            std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));
        let saved_pc_tag = self.pc_tag;
        self.pc_tag = branch_pc_tag;

        let run_result = self.run();

        self.machine.program_data = saved_program;
        self.pc_tag = saved_pc_tag;

        run_result
    }

    /// Calls the ordinary function `function_name` and pushes its return value
    /// (if any).
    fn call_function(&mut self, function_name: &str) -> ExecutorResult<(), Tag> {
        if let Some(entry) = self.run_function_body(function_name, false)? {
            self.push_existing_entry(entry);
        }
        Ok(())
    }

    /// Invokes the downgrader `function_name`: runs its body like a function
    /// call, then applies the implicit retag — the return value must carry the
    /// connection `source` and is forced to `target`, bypassing the `pc_tag`
    /// join applied to ordinary values. Rejects names not registered as
    /// downgraders.
    fn call_downgrader(&mut self, function_name: &str) -> ExecutorResult<(), Tag> {
        // `Downgrader` is `Copy`, so the lookup releases the `&self.policy`
        // borrow at once.
        let Some(downgrader) = self.policy.downgrader(function_name) else {
            return Err(FlowError::DowngraderUndefined {
                name: function_name.to_owned(),
            }
            .into());
        };

        // Downgrades must happen at the top level of the program: a `Downgrade`
        // inside a function or downgrader body is rejected.
        if self.function_depth > 0 {
            return Err(FlowError::NestedDowngraderCall {
                downgrader: function_name.to_owned(),
            }
            .into());
        }

        // Charge the call against the downgrader's total call limit before the
        // body runs, so a call exceeding the limit fails without side effects.
        let calls = self
            .downgrader_calls
            .entry(function_name.to_owned())
            .or_insert(0);
        *calls += 1;
        if let Some(limit) = downgrader.max_calls
            && *calls > limit
        {
            return Err(FlowError::DowngraderCallLimitExceeded {
                downgrader: function_name.to_owned(),
                limit,
            }
            .into());
        }

        if let Some((value, tag)) = self.run_function_body(function_name, true)? {
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
        Ok(())
    }

    /// Shared call mechanics: resolves the body, guards the recursion depth,
    /// and runs it in a nested context. Returns the body's return value/tag.
    fn run_function_body(
        &mut self,
        function_name: &str,
        is_downgrader: bool,
    ) -> ExecutorResult<Option<(Value, Tag)>, Tag> {
        let body = if is_downgrader {
            self.machine.downgrader_get(function_name)?
        } else {
            self.machine.function_get(function_name)?
        }
        .clone();

        self.function_depth += 1;
        if self.function_depth > RECURSION_LIMIT {
            panic!("Recursion limit of {RECURSION_LIMIT} exceeded in function '{function_name}'");
        }

        let result = match body {
            Instruction::Block(_, instrs) if instrs.is_empty() => Err(EmptyBlock),
            Instruction::Block(argument_count, instrs) => self.run_nested(argument_count, instrs),
            _ => Err(CoreError::InvalidDefinitionBody {
                name: function_name.to_owned(),
            }
            .into()),
        };
        self.function_depth -= 1;
        result
    }

    fn ensure_output_allowed(&self, value_tag: Tag) -> ExecutorResult<(), Tag> {
        let effective_tag = self.combine_tags(value_tag, self.pc_tag)?;
        let output_guard = self.policy.output_tag();

        if self.policy.can_flow(effective_tag, output_guard)? {
            Ok(())
        } else {
            Err(FlowError::PGViolation {
                found: effective_tag,
                guard: output_guard,
            }
            .into())
        }
    }

    fn read_input_value(&mut self) -> ExecutorResult<Immediate, Tag> {
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
            Input::Buffer(buffer) => buffer
                .borrow_mut()
                .pop()
                .ok_or(Core(CoreError::IoReadError))?,
        };

        Ok(value)
    }
}

impl<Tag: TagTrait> Evaluate<Tag> for Executor<Tag> {
    type Error = ExecutorError<Tag>;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> ExecutorResult<(), Tag> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Input => {
                let value = self.read_input_value()?;
                self.push_new_value(Integer(value), self.policy.input_tag())?;
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm<Tag>,
        arg: Immediate,
    ) -> ExecutorResult<(), Tag> {
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
    ) -> ExecutorResult<(), Tag> {
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
            Print => {
                let (value, value_tag) = self.read_entry(arg)?;
                self.ensure_output_allowed(value_tag)?;

                match &mut self.machine.output {
                    types::Output::Stdout => print!("{value}"),
                    types::Output::Buffer(_) => {
                        let immediate = value.into_immediate().map_err(|_| InvalidCell)?;
                        self.machine.output.write(&[immediate]);
                    }
                }
            }
        }

        Ok(())
    }

    fn evaluate_alu_unary_cell_amnt(
        &mut self,
        instr: &UnaryOpCellAmnt,
        amount: CellAmount,
    ) -> ExecutorResult<(), Tag> {
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
    ) -> ExecutorResult<(), Tag> {
        use UnaryOpString::*;

        match instr {
            FunctionDefine => self.machine.common_function_logic(name)?,
            Downgrader => {
                if self.policy.downgrader(name).is_none() {
                    return Err(FlowError::DowngraderUndefined {
                        name: name.to_owned(),
                    }
                    .into());
                }
                self.machine.common_downgrader_logic(name)?
            }
            FunctionCall => self.call_function(name)?,
            Downgrade => self.call_downgrader(name)?,
        }

        Ok(())
    }

    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> ExecutorResult<(), Tag> {
        let (left, left_tag) = self.read_entry(arg1)?;
        let (right, right_tag) = self.read_entry(arg2)?;
        let result_tag = self.combine_tags(left_tag, right_tag)?;

        debug!("Evaluating binary: {:?} {:?} {:?}", left, instr, right);

        let expect_integer = |cell: Value| -> ExecutorResult<i64, Tag> {
            match cell {
                Integer(value) => Ok(value),
                _found => {
                    todo!("This will eventually be a TypeError, in case types get implemented.")
                }
            }
        };

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
                CmpEqual => Ok(from_bool(left == right)),
                CmpNotEqual => Ok(from_bool(left != right)),
                CmpLessThan => Ok(from_bool(left < right)),
                CmpLessThanOrEqual => Ok(from_bool(left <= right)),
                CmpGreaterThan => Ok(from_bool(left > right)),
                CmpGreaterThanOrEqual => Ok(from_bool(left >= right)),
            }
        }?;
        self.push_new_value(Integer(result), result_tag)?;

        Ok(())
    }

    fn evaluate_block(
        &mut self,
        argument_count: CellAmount,
        instrs: Rc<[Instruction<Tag>]>,
    ) -> ExecutorResult<(), Tag> {
        // Each block must leave at least one value on its local stack so the parent can
        // observe a result. A block that ends with an empty local stack is a "void" error.
        if instrs.is_empty() {
            return Err(EmptyBlock);
        }
        match self.run_nested(argument_count, instrs)? {
            Some(entry) => self.push_existing_entry(entry),
            None => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction<Tag>>,
        when_false: Rc<Instruction<Tag>>,
    ) -> ExecutorResult<(), Tag> {
        let (condition, condition_tag) = self.read_entry(cond_idx)?;

        let branch = match condition {
            Integer(0) => when_false,
            Integer(_) => when_true,
            _other => todo!("This will eventually be a TypeError, in case types get implemented."),
        };

        let branch_program = Rc::from(vec![branch.as_ref().clone()]);
        self.run_ifelse_branch(branch_program, condition_tag)
    }
}
