# Abstract Machine With Code Verification and Information Flow Tracking

Safe execution of programs requires strict code verification before the program
is executed at all.
Security can be further enhanced by information flow tracking, which allows
control over how values are transferred between different parts of the system,
especially standard input and output.
In this thesis, an experimental abstract machine is designed, whose
computational model combines properties of stack-based and register-based
architectures.
Stack values are immutable, and control flow is restricted to conditional
statements and function calls.
The machine is implemented as a library in the Rust language and includes an
executor for concrete program execution and a verifier that performs code
verification before execution using intervals that represent possible values
during execution.
Both parts of the machine also support information flow tracking.
The machine requires a security policy that defines the rules for tracking.
To avoid tag creep, it is possible to change data tags from strict to loose
using downgraders.
The developed machine successfully verifies programs before execution and
performs information flow tracking.

## Running, Testing, and Building

Make sure you have Rust and `cargo` installed.
All building and testing is done through `cargo` commands.

Optionally, you can also install `just` (Makefile alternative) for easier
building and testing.

Use `just --list` to see all available commands.
Checkout `justfile` for more details.
