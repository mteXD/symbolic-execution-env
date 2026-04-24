#[macro_export]
macro_rules! add_instr {
    ($op:ident) => {
        AluNullary(NullaryOp::$op)
    };
    ($op:ident, $a:expr) => {
        // for immediate
        AluUnaryImm(UnaryOpImm::$op, $a)
    };
    (R $op:ident, $a:expr) => {
        // for register
        AluUnaryCell(UnaryOpCell::$op, $a)
    };
    ($op:ident, $a:expr, $b:expr) => {
        AluBinary(BinaryOp::$op, $a, $b)
    };
    (fun $op:ident, $name:expr) => {
        AluFunction(FunctionOp::$op, $name)
    };
}

#[macro_export]
macro_rules! make_block {
    ($($instr:expr),+) => { // Variadic arguments, at least one
        Block(vec![ $( $instr ),* ])
    };
}

pub use add_instr;
pub use make_block;
