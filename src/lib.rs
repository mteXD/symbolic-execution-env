// A virtual machine that's a hybrid between a register-based and stack-based architecture.
// Registers are temporarily represented as i64 value. These "registers" are referred to as "cells"
// in the code.
// It has a push instruction - it places a value to the next available cell.
// This is similar to Single Static Assignment (SSA) form in compilers.
// Pop doesn't have to exist for reading purposes, as we can read directly from available cell.
// However, pop can be used to free up cells when needed.

// pub struct Register(pub usize);
// pub struct Immediate(pub i64);
mod instructions;
mod machine;
mod types;

//TODO: figure out whether to keep code in separate modules or include some here
pub fn public_function_to_make_the_stupid_lsp_happy_and_fix_unused_warnings() {}

#[cfg(test)]
mod tests {
    use crate::{
        instructions::{
            BinaryOp,
            Instruction::{AluBinary, AluUnaryImm, AluUnaryReg, AluNullary},
            UnaryOpImm::Push,
            UnaryOpReg,
            NullaryOp::Nop,
        },
        machine::Machine,
        types::MachineError,
    };

    macro_rules! test_binop {
        ($name:ident, $a:expr, $b:expr, $op:ident => $expected:expr) => {
            #[test]
            fn $name() {
                let mut machine = Machine::default();
                let program = vec![
                    AluUnaryImm(Push, $a),
                    AluUnaryImm(Push, $b),
                    AluBinary(BinaryOp::$op, 0, 1),
                ];
                let last = machine.run(&program).unwrap();
                assert_eq!(last, Some(&$expected));
            }
        };
    }

    test_binop!(test_add, 10, 20, Add => 30);
    test_binop!(test_add_neg, 10, -30, Add => -20);
    test_binop!(test_mul, 10, 20, Mul => 200);
    test_binop!(test_div, 20, 5, Div => 4);

    #[test]
    fn test_div_bad() {
        let mut machine = Machine::default();
        let program = vec![
            AluUnaryImm(Push, 10),
            AluUnaryImm(Push, 0),
            AluBinary(BinaryOp::Div, 0, 1),
        ];
        let result = machine.run(&program);
        assert!(matches!(result, Err(MachineError::DivisionByZero)));
    }

    test_binop!(test_and, 0b1100, 0b1010, And => 0b1000);
    test_binop!(test_or, 0b1100, 0b1010, Or => 0b1110);
    test_binop!(test_xor, 0b1100, 0b1010, Xor => 0b0110);

    #[test]
    fn test_not() {
        let mut machine = Machine::default();
        let program = vec![AluUnaryImm(Push, 0b1100), AluUnaryReg(UnaryOpReg::Not, 0)];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&(!0b1100)));
    }

    test_binop!(test_slt, 10, 20, SetLessThan => 1);
    test_binop!(test_sgt, 20, 10, SetGreaterThan => 1);
    test_binop!(test_seq, 10, 10, SetEqual => 1);
    test_binop!(test_sne, 10, 20, SetNotEqual => 1);
    test_binop!(test_sle, 10, 10, SetLessThanOrEqual => 1);
    test_binop!(test_sge, 20, 10, SetGreaterThanOrEqual => 1);

    test_binop!(test_sll, 0b0001, 2, ShiftLeftLogical => 0b0100);
    test_binop!(test_srl, 0b0100, 2, ShiftRightLogical => 0b0001);
    test_binop!(test_sra, -8, 2, ShiftRightArithmetic => -2);

    #[test]
    fn nop() {
        let mut machine = Machine::default();
        let program = vec![AluNullary(Nop)];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, None);
    }

    #[test]
    fn math_with_read() {
        let mut machine = Machine::default();
        let program = vec![
            AluUnaryImm(Push, 50),
            AluUnaryImm(Push, 70),
            AluUnaryImm(Push, 10),
            AluBinary(BinaryOp::Add, 0, 1), // 50 + 70 = 120
            AluBinary(BinaryOp::Div, 3, 2), // 120 / 10 = 12
        ];
        let last = machine.run(&program).unwrap();
        assert_eq!(last, Some(&12));
    }

    #[test]
    fn test_run_until() {
        let mut machine = Machine::default();
        let program = vec![
            AluUnaryImm(Push, 10),
            AluUnaryImm(Push, 20),
            AluBinary(BinaryOp::Add, 0, 1),
            AluUnaryImm(Push, 5),
            AluBinary(BinaryOp::Mul, 2, 3),
        ];
        let last = machine.run_until(&program, 3).unwrap();
        assert_eq!(last, Some(&30)); // After first 3 instructions: 10 + 20 = 30

        let mut machine = Machine::default();
        let last = machine.run_until(&program, 5).unwrap();
        assert_eq!(last, Some(&150)); // After all instructions: 30 * 5 = 150
    }
}
