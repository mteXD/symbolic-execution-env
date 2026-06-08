use std::{ops::Deref, rc::Rc};

use super::*;
use crate::{
    information_flow::{FlowError, FlowTag, InformationFlowPolicy, NoFlow},
    instruction::{
        BinaryOp, FunctionOp, Instruction, IntrinsicArg, IntrinsicOp, NullaryOp, UnaryOpCell,
        UnaryOpImm,
    },
    machine::{CoreError, CoreMachine, PairedStack, StackFrames},
    types::{self, Cell, CellIndex, Immediate, ProgramData},
};
use Cell::*;
use ExecutorError::*;

#[derive(Debug, Clone)]
pub enum ExecutorError<Tag = ()>
where
    Tag: FlowTag,
{
    DivisionByZero,
    ArithmeticOverflow,
    StackUnderflow,
    NoSavedCells,
    NoRebasedCells,
    InvalidCell,
    TypeError { expected: Cell, found: Cell },
    BlockHasEmptyStack,
    Core(CoreError),
    Flow(FlowError<Tag>),
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

pub struct Executor<P: InformationFlowPolicy = NoFlow> {
    machine: CoreMachine<P::Tag>,
    stack: PairedStack<Cell, P::Tag>,
    policy: P,
    pc_tag: P::Tag,
    function_depth: usize,
}

// Convenience: lets `executor.cells`, `executor.base`, and the stack methods
// (`push`, `pop`, `get`) resolve directly through the inner `StackFrames`.
impl<P: InformationFlowPolicy> Deref for Executor<P> {
    type Target = StackFrames<Cell>;
    fn deref(&self) -> &StackFrames<Cell> {
        self.stack.values()
    }
}

impl Executor<NoFlow> {
    pub fn new(program: impl Into<Rc<[Instruction]>>) -> Self {
        Self {
            machine: CoreMachine::new(program),
            stack: PairedStack::new(),
            policy: NoFlow,
            pc_tag: (),
            function_depth: 0,
        }
    }
}

impl<P: InformationFlowPolicy> Executor<P> {
    pub fn with_policy(
        program: impl Into<Rc<[Instruction<P::Tag>]>>,
        policy: P,
    ) -> ExecutorResult<Self, P> {
        let program = program.into();
        Self::validate_program(&program, &policy)?;
        let pc_tag = policy.default_tag();
        Ok(Self {
            machine: CoreMachine::new(program),
            stack: PairedStack::new(),
            policy,
            pc_tag,
            function_depth: 0,
        })
    }

    fn validate_program(program: &[Instruction<P::Tag>], policy: &P) -> ExecutorResult<(), P> {
        for instruction in program {
            match instruction {
                Instruction::TaggedPush { tag, .. } => policy.validate_tag(*tag)?,
                Instruction::Block(inner) => Self::validate_program(inner, policy)?,
                Instruction::IfElse(_, when_true, when_false) => {
                    Self::validate_program(std::slice::from_ref(when_true.as_ref()), policy)?;
                    Self::validate_program(std::slice::from_ref(when_false.as_ref()), policy)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn redirect_input(&mut self, new_input: types::Input) {
        self.machine.input = new_input;
    }

    pub fn redirect_output(&mut self, new_output: types::Output) {
        self.machine.output = new_output;
    }

    #[inline]
    pub fn read(&self, reg: CellIndex) -> ExecutorResult<Cell, P> {
        self.get(reg.into()).copied().ok_or(InvalidCell)
    }

    pub fn read_tag(&self, reg: CellIndex) -> ExecutorResult<P::Tag, P> {
        self.stack.get_tag(reg.into()).copied().ok_or(InvalidCell)
    }

    pub fn tags(&self) -> &[P::Tag] {
        self.stack.tags()
    }

    pub fn last_tag(&self) -> Option<P::Tag> {
        self.tags().last().copied()
    }

    fn combine_tags(&self, left: P::Tag, right: P::Tag) -> ExecutorResult<P::Tag, P> {
        Ok(self.policy.closest_common_descendant(left, right)?)
    }

    fn push_with_tag(&mut self, value: Cell, tag: P::Tag) -> ExecutorResult<(), P> {
        let effective = self.combine_tags(tag, self.pc_tag)?;
        self.stack.push(value, effective);
        Ok(())
    }

    fn run(&mut self) -> ExecutorResult<(), P> {
        while let Some(instr) = self.machine.next() {
            self.evaluate_instruction(&instr)?;
        }
        Ok(())
    }

    pub fn exec(&mut self) -> ExecutorResult<Option<&Cell>, P> {
        self.run()?;
        Ok(self.cells.last())
    }

    /// Runs `instrs` as a nested context (block / function body / ifelse branch).
    ///
    /// Saves and restores `program_data` (so the inner program is scoped) and
    /// `function_data` (so inner `FunctionDefine`s don't leak to the parent).
    /// Cells are managed in-place via the [`StackFrames`] helper.
    fn run_nested(
        &mut self,
        instrs: Rc<[Instruction<P::Tag>]>,
    ) -> ExecutorResult<Option<(Cell, P::Tag)>, P> {
        let saved_base = self.stack.enter_block();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));

        let exec_result = self.run();

        self.machine.program_data = saved_pd;
        let (result, _) = self.stack.exit_block(saved_base);

        exec_result?;
        Ok(result)
    }

    /// Runs the body of an ifelse branch *inline* on the parent stack: cells
    /// are not isolated, so pops and pushes persist after the branch ends.
    /// `program_data` and `function_data` are still scoped. The marker frame
    /// pushed via [`StackFrames::enter_ifelse_branch`] makes `Rebase` an error
    /// inside the branch.
    fn run_ifelse_branch(
        &mut self,
        instrs: Rc<[Instruction<P::Tag>]>,
        condition_tag: P::Tag,
    ) -> ExecutorResult<(), P> {
        let branch_pc_tag = self.combine_tags(self.pc_tag, condition_tag)?;
        // Save program_data and function_data; add a ifelse frame
        self.stack.enter_ifelse_branch();
        let saved_pd = std::mem::replace(&mut self.machine.program_data, ProgramData::new(instrs));
        let saved_pc_tag = self.pc_tag;
        self.pc_tag = branch_pc_tag;

        let exec_result = self.run();

        // Restore program_data and function_data; pop the ifelse frame
        self.machine.program_data = saved_pd;
        self.pc_tag = saved_pc_tag;
        self.stack.exit_ifelse_branch();

        exec_result
    }
}

impl<P: InformationFlowPolicy> Evaluate<P::Tag> for Executor<P> {
    type Error = ExecutorError<P::Tag>;

    fn evaluate_alu_nullary(&mut self, instr: &NullaryOp) -> ExecutorResult<(), P> {
        use NullaryOp::*;

        match instr {
            Nop => (),
            Rebase => self.stack.rebase()?,
        }

        Ok(())
    }

    fn evaluate_alu_unary_imm(
        &mut self,
        instr: &UnaryOpImm,
        arg: Immediate,
    ) -> ExecutorResult<(), P> {
        use UnaryOpImm::*;

        match instr {
            Push => self.push_with_tag(Integer(arg), self.policy.default_tag())?,
        }

        Ok(())
    }

    fn evaluate_tagged_push(&mut self, value: Immediate, tag: &P::Tag) -> ExecutorResult<(), P> {
        self.push_with_tag(Integer(value), *tag)
    }

    fn evaluate_alu_unary_cell(
        &mut self,
        instr: &UnaryOpCell,
        arg: CellIndex,
    ) -> ExecutorResult<(), P> {
        use UnaryOpCell::*;

        match instr {
            Not => {
                let val = self.read(arg)?;
                let tag = self.read_tag(arg)?;
                if let Integer(v) = val {
                    self.push_with_tag(Integer(!v), tag)?;
                } else {
                    return Err(TypeError {
                        expected: Integer(0),
                        found: val,
                    });
                }
            }
            Read => {
                let val = self.read(arg)?;
                let tag = self.read_tag(arg)?;
                self.push_with_tag(val, tag)?;
            }
            ReadReverse => {
                // like python's negative indexing.
                let index = u16::try_from(self.cells.len())
                    .ok()
                    .and_then(|len| len.checked_sub(1))
                    .and_then(|len| len.checked_sub(arg))
                    .ok_or(InvalidCell)?;
                let val = self.read(index)?;
                let tag = self.read_tag(index)?;
                self.push_with_tag(val, tag)?;
            }
            Pop => {
                for _ in 0..arg {
                    self.stack.pop().ok_or(StackUnderflow)?; // Discard the popped value/tag pair
                }
            }
        }

        Ok(())
    }

    fn evaluate_alu_binary(
        &mut self,
        instr: &BinaryOp,
        arg1: CellIndex,
        arg2: CellIndex,
    ) -> ExecutorResult<(), P> {
        use BinaryOp::*;
        fn from_bool<T: From<bool>>(value: bool) -> T {
            value.into()
        }

        let a = self.read(arg1)?;
        let b = self.read(arg2)?;
        let tag = self.combine_tags(self.read_tag(arg1)?, self.read_tag(arg2)?)?;

        debug!("Evaluating binary: {:?} {:?} {:?}", a, instr, b);

        if let (Integer(a), Integer(b)) = (a, b) {
            let calculated_value = match instr {
                Add => a.checked_add(b).ok_or(ArithmeticOverflow)?,
                Mul => a.checked_mul(b).ok_or(ArithmeticOverflow)?,
                Div => a.checked_div(b).ok_or(DivisionByZero)?,
                And => a & b,
                Or => a | b,
                Xor => a ^ b,
                ShiftLeftLogical => a << b,
                ShiftRightLogical => ((a as u64) >> b) as i64,
                ShiftRightArithmetic => a >> b,
                SetEqual => from_bool(a == b),
                SetNotEqual => from_bool(a != b),
                SetLessThan => from_bool(a < b),
                SetLessThanOrEqual => from_bool(a <= b),
                SetGreaterThan => from_bool(a > b),
                SetGreaterThanOrEqual => from_bool(a >= b),
            };

            self.push_with_tag(Integer(calculated_value), tag)?;
        } else {
            return Err(TypeError {
                expected: Integer(0),
                found: self
                    .cells
                    .last()
                    .copied()
                    .expect("Stack should not be empty here"),
            });
        }

        Ok(())
    }

    fn evaluate_block(&mut self, instrs: Rc<[Instruction<P::Tag>]>) -> ExecutorResult<(), P> {
        // Each block must leave at least one value on its local stack so the parent can
        // observe a result. A block that ends with an empty local stack is a "void" error.
        match self.run_nested(instrs)? {
            Some((val, tag)) => self.stack.push(val, tag),
            None => return Err(BlockHasEmptyStack),
        }
        Ok(())
    }

    fn evaluate_function(&mut self, instr: &FunctionOp, fun: &String) -> ExecutorResult<(), P> {
        use FunctionOp::*;

        match instr {
            FunctionDefine => self.machine.common_function_logic(fun)?,
            FunctionCall => {
                let instr = self.machine.function_get(fun)?.clone();

                self.function_depth += 1;
                if self.function_depth > RECURSION_LIMIT {
                    panic!("Recursion limit of {RECURSION_LIMIT} exceeded in function '{fun}'");
                }

                let result = self.run_nested(Rc::<[Instruction<P::Tag>]>::from(vec![instr]));
                self.function_depth -= 1;

                if let Some(val) = result? {
                    self.stack.push(val.0, val.1);
                }
            }
        }

        Ok(())
    }

    fn evaluate_intrinsic(
        &mut self,
        instr: &IntrinsicOp,
        arg: &IntrinsicArg,
    ) -> ExecutorResult<(), P> {
        use IntrinsicArg::*;
        use IntrinsicOp::*;
        use types::{Input, Output};

        match (instr, arg) {
            (Print, Cell(cell_idx)) => {
                let val = self.read(*cell_idx)?;
                let val_tag = self.read_tag(*cell_idx)?;
                let effective_tag = self.combine_tags(val_tag, self.pc_tag)?;
                let output_tag = self.policy.output_tag();
                if !self.policy.can_flow(effective_tag, output_tag)? {
                    return Err(FlowError::InformationFlowViolation {
                        found: effective_tag,
                        guard: output_tag,
                    }
                    .into());
                }
                match &mut self.machine.output {
                    Output::Stdout => print!("{val}"),
                    Output::File(_) | Output::Buffer(_) => {
                        let imm = val.into_immediate().map_err(|_| InvalidCell)?;
                        self.machine.output.write(&[imm]);
                    }
                }
            }
            (Input, Cell(_)) => match &self.machine.input {
                Input::Stdin => {
                    let mut input: String = String::new();
                    std::io::stdin()
                        .read_line(&mut input)
                        .expect("Failed to read input");

                    match input.trim().parse::<i64>() {
                        Ok(val) => self.push_with_tag(Integer(val), self.policy.input_tag())?,
                        Err(e) => todo!("For now, invalid input is a fatal error: {e}"),
                    }
                }
                Input::File(_) => {
                    let mut data = self.machine.input.read_all();
                    if let Some(val) = data.pop() {
                        self.push_with_tag(Integer(val), self.policy.input_tag())?;
                        self.machine.input =
                            Input::Buffer(std::rc::Rc::new(std::cell::RefCell::new(data)));
                    } else {
                        return Err(Core(CoreError::IoReadError));
                    }
                }
                Input::Buffer(ref_cell) => {
                    let new_val = ref_cell
                        .borrow_mut()
                        .pop()
                        .ok_or(Core(CoreError::IoReadError))?;
                    self.push_with_tag(Integer(new_val), self.policy.input_tag())?;
                }
            },
            (FileRead, Str(path)) => {
                if path.is_empty() {
                    self.machine.input = Input::Stdin;
                } else {
                    self.machine.input = Input::File(path.clone());
                }
            }
            (FileWrite, Str(path)) => {
                if path.is_empty() {
                    self.machine.output = Output::Stdout;
                } else {
                    self.machine.output = Output::File(path.clone());
                }
            }
            _ => return Err(InvalidCell),
        }

        Ok(())
    }

    fn evaluate_ifelse(
        &mut self,
        cond_idx: CellIndex,
        when_true: Rc<Instruction<P::Tag>>,
        when_false: Rc<Instruction<P::Tag>>,
    ) -> ExecutorResult<(), P> {
        let condition = self.get(cond_idx.into()).copied().ok_or(InvalidCell)?;
        let condition_tag = self.read_tag(cond_idx)?;

        let branch = match condition {
            Integer(0) => when_false,
            Integer(_) => when_true,
            other => {
                return Err(TypeError {
                    expected: Integer(0),
                    found: other,
                });
            }
        };

        // The chosen branch runs inline on the parent's cells: its pops and
        // pushes are permanent, and `Rebase` is forbidden inside it.
        // PERF: clone
        self.run_ifelse_branch(
            Rc::<[Instruction<P::Tag>]>::from(vec![(*branch).clone()]),
            condition_tag,
        )
    }
}

impl From<Vec<Cell>> for Executor {
    fn from(value: Vec<Cell>) -> Self {
        let mut machine = Self::new(Vec::<Instruction>::new());
        machine.stack.set_values_for_unmonitored(value, ());
        machine
    }
}

#[cfg(test)]
pub mod tests;
