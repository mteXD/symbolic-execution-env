use std::rc::Rc;

use criterion::{Criterion, criterion_group, criterion_main};
use virtual_machine::{add_instr, instruction::Instruction, machine::executor::Executor};

fn bench1(c: &mut Criterion) {
    let pushes = (0..10000).map(|i| add_instr!(Push, i));
    let additions = (0..9999).zip(1..10000).map(|(i, j)| add_instr!(Add, i, j));
    let program: Rc<[Instruction]> = std::hint::black_box(pushes.chain(additions).collect());

    c.bench_function("simple addition", |b| {
        b.iter(|| {
            // `exec` consumes the executor, so build a fresh one per
            // iteration; cloning the `Rc` program is cheap.
            let executor = Executor::new(program.clone());
            let _ = executor.exec().expect("Failed to run the program");
        })
    });
}

criterion_group!(benches, bench1);
criterion_main!(benches);
