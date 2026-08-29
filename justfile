export RUST_LOG := "trace"

[group('building')]
build:
    @echo "Building the project..."
    cargo build
    @echo "Build completed successfully."

[group('building')]
release: clean
    @echo "Building the project in release mode..."
    cargo build --release
    @echo "Release build completed successfully."
    
[group('dev')]
check:
    cargo check --tests

[group('dev')]
test arg="":
    cargo test {{arg}} --

[group('dev')]
test-list:
    cargo test -- --list

[group('destructive')]
refactor:
    cargo clippy --fix --lib -p virtual_machine
    cargo fmt --all -- --check

[group('cleaning')]
clean:
    cargo clean
    rm flamegraph.svg
    rm perf.data
    rm perf.data.old