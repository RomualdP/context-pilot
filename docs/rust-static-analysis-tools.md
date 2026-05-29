# Static Source Code Analysis Tools for Rust

> *Modeled after [Gerard Holzmann's classic listing](https://web.archive.org/web/2017/https://spinroot.com/static/) of static analysis tools for C. Updated May 2025.*

---

**Preamble.**
The landscape of program analysis for Rust differs fundamentally from that of C. Rust's compiler (`rustc`) already enforces memory safety, data-race freedom, and lifetime correctness through its ownership and borrowing system — guarantees that, in C, require the most sophisticated commercial tools (Astrée, PolySpace, Coverity) and are never fully achieved even then. The remaining frontier for Rust analysis tools is therefore narrower but deeper: proving functional correctness, verifying `unsafe` code, catching logic errors, and auditing the supply chain. Where a C project might need five tools just to reach memory safety, a Rust project starts there and reaches upward toward full formal verification.

---

## 1. Built-in Compiler Analysis

### rustc

The Rust compiler itself is the most important static analysis tool in the ecosystem. The borrow checker enforces single-ownership and borrowing rules at compile time, eliminating use-after-free, double-free, data races, and dangling references without runtime overhead. The type system enforces pattern exhaustiveness, null-safety (via `Option<T>`), and error propagation (via `Result<T,E>`). Over 100 built-in compiler lints cover dead code, unreachable patterns, unused variables, and more. For a developer coming from C, this is roughly equivalent to running Coverity, a race detector, and a bounds checker simultaneously — except it's free, mandatory, and has a zero false-positive rate for the properties it checks.

### Clippy

A collection of over 800 lints organized into categories: correctness, suspicious, style, complexity, perf, pedantic, restriction, nursery, and cargo. Clippy is maintained under the Rust project umbrella and ships with `rustup`. It is the universal baseline — every Rust project should run it. The correctness and suspicious categories are carefully curated and nearly free of false positives; the pedantic and restriction categories are intentionally aggressive and should be adopted selectively. Clippy is to Rust what lint was to C, except vastly more capable and better maintained. Recommended unconditionally.

- Docs: [doc.rust-lang.org/clippy](https://doc.rust-lang.org/clippy/)
- Repository: [github.com/rust-lang/rust-clippy](https://github.com/rust-lang/rust-clippy)

---

## 2. Commercial and Industry Tools

### CodeQL (GitHub / Microsoft)

The successor to Semmle's Odasa, now integrated into GitHub Advanced Security. CodeQL constructs a relational database from the codebase and allows queries in a Datalog-inspired language. This is the same methodology that Holzmann recommended for C ("with some practice, the queries are not that hard to write"), and it translates well to Rust. CodeQL supports Rust with growing coverage, and is free on public repositories. For organizations on GitHub, this is the most practical commercial SAST option. The query language allows custom security rules beyond the built-in set. Recommended, especially for teams already on GitHub.

- Docs: [codeql.github.com](https://codeql.github.com/)

### Semgrep

A lightweight, pattern-matching SAST tool supporting 40+ languages including Rust. Rules are written in YAML and look like the code they match — no abstract syntax trees or regex wrestling required. Fast CI integration. The open-source version provides solid coverage; the commercial version (Semgrep Code) adds cross-file taint analysis. Custom rules are easy to write, making it attractive for enforcing project-specific coding standards. Less depth than CodeQL for complex data-flow analysis, but much faster to set up and run.

- Repository: [github.com/returntocorp/semgrep](https://github.com/returntocorp/semgrep)

### Coverity (by Synopsys)

The well-known tool based on Dawson Engler's methodology, now with Rust support. In the C world, Coverity is a default choice for finding memory errors and concurrency bugs. For Rust, its value is diminished because the compiler already handles memory safety in safe code. Its main utility for Rust is in analyzing `unsafe` blocks and FFI boundaries. Not evaluated in depth for Rust; the tool's Rust analysis is younger and less mature than its C/C++ analysis.

### Klocwork (by Perforce)

Perforce's SAST tool now supports Rust alongside C, C++, C#, Java, JavaScript, Python, and Kotlin. Marketed for enterprise-scale codebases in regulated industries. Includes SAST for security vulnerabilities. Perforce QAC, their compliance-focused analyzer, also added Rust support, targeting safety-critical industries that need MISRA-like compliance checking. Not evaluated independently.

- Website: [perforce.com/products/klocwork](https://www.perforce.com/products/klocwork)

### SonarQube

A code quality platform that covers bugs, code smells, technical debt, duplication, and security in a single dashboard. Rust support has been growing. SonarQube's strength is quality gates — blocking PRs that fail predefined standards. More of a code-quality tool than a deep security analyzer. The Community Edition is free and open-source. Useful as a complement to, not a replacement for, more targeted SAST tools.

- Website: [sonarqube.org](https://www.sonarqube.org/)

### TrustInSoft Analyzer

A formal verification toolchain that in November 2025 added comprehensive Rust analysis, developed in partnership with Ferrous Systems (makers of Ferrocene, the qualified Rust compiler). TrustInSoft claims mathematical proof guarantees — not probabilistic detection — of the absence of undefined behavior, panics, and memory vulnerabilities, including in mixed Rust/C/C++ codebases. Targets safety-critical environments (automotive, aerospace, defense). Expensive but thorough. The partnership with Ferrous Systems lends credibility for the Rust analysis capabilities.

- Website: [trustinsoft.com](https://www.trustinsoft.com/)

### PVS-Studio

A static analyzer historically focused on C/C++, with Rust support added. Provides CWE, OWASP, MISRA, and CERT coding standard compliance checking. Conditionally free for FOSS and individual developers. Not evaluated for Rust.

---

## 3. Academic and Research Tools

### 3a. Dynamic Analysis and Interpreters

#### Miri

An interpreter for Rust's Mid-level Intermediate Representation (MIR) that detects undefined behavior at runtime. Miri catches data races, memory leaks, use-after-free, invalid pointer arithmetic, violations of Stacked Borrows / Tree Borrows aliasing rules, and more — all issues that can arise in `unsafe` code. It has found bugs in the Rust standard library and compiler. Miri ships with rustup nightly (`rustup +nightly component add miri; cargo +nightly miri test`). Essential for anyone writing `unsafe` code. The gold standard for dynamic UB detection in Rust.

- Repository: [github.com/rust-lang/miri](https://github.com/rust-lang/miri)
- Developed by: Ralf Jung et al.

#### Sanitizers (ASan, TSan, MSan)

Rust supports LLVM's AddressSanitizer, ThreadSanitizer, and MemorySanitizer via compiler flags (`-Zsanitizer=address`). These provide runtime instrumentation for detecting memory errors, data races, and use of uninitialized memory. Primarily useful for `unsafe` code and FFI boundaries. Nightly only. Less Rust-specific than Miri but faster for large codebases.

### 3b. Static Bug Finders

#### Rudra

A static analyzer for detecting memory safety bugs in `unsafe` Rust at ecosystem scale (SOSP'21, Georgia Tech). Rudra performs hybrid HIR+MIR analysis and targets three bug classes: memory safety when panicked, higher-order invariant violations, and Send/Sync variance errors. It was run against all 43,000+ packages on crates.io and identified 264 previously unknown bugs, leading to **76 CVEs and 112 RustSec advisories** — representing 51.6% of all memory safety bugs reported to RustSec since 2016. This included bugs in the Rust standard library, the official futures library, and the Rust compiler itself. An impressive achievement. Development has slowed (last active ~2024), and it is best understood as a pioneering one-shot ecosystem scanner rather than a continuous CI tool.

- Paper: Bae et al., "Rudra: Finding Memory Safety Bugs in Rust at the Ecosystem Scale," SOSP 2021.
- Repository: [github.com/sslab-gatech/Rudra](https://github.com/sslab-gatech/Rudra)

#### MIRAI

An abstract interpreter for Rust's MIR by Herman Venter (formerly at Meta). MIRAI targets panics, security bugs, and general correctness issues using taint analysis and abstract interpretation. It was intended to become a widely-used static analysis tool for Rust. Development appears to have slowed as of 2025. The approach is sound and the tool has potential, but the lack of sustained investment limits its practical utility.

- Repository: [github.com/endorlabs/MIRAI](https://github.com/endorlabs/MIRAI)

#### MirChecker

A static analysis framework that combines numerical and symbolic analysis on Rust MIR to detect runtime crashes (including arithmetic errors) and memory safety violations (CCS'21). It detected 33 previously unknown bugs including 16 memory-safety issues across 12 crates. However, the tool suffers from a very high false positive rate (reported at 79–95% in subsequent studies), which limits its practical adoption.

- Paper: Li et al., "MirChecker: Detecting Bugs in Rust Programs via Static Analysis," CCS 2021.

#### lockbud

Detects common memory and concurrency bugs: double-lock, conflicting lock order, atomicity violations, use-after-free, invalid-free, and panic locations (TSE'24). MIR-based data-flow analysis. Actively maintained as of 2025. Fills an important niche — concurrency bug detection — that is underserved by other Rust tools.

- Paper: "Safety Issues in Rust," TSE 2024.
- Repository: [github.com/AidenPearce-ZYQ/lockbud](https://github.com/AidenPearce-ZYQ/lockbud)

#### Yuga

Detects lifetime annotation bugs in Rust (ICSE'24). Developed by researchers at Columbia University with Red Hat Research involvement. Targets a subtle class of bugs where incorrect lifetime annotations in `unsafe` code can lead to memory corruption. False positive rate reported at ~46%.

- Paper: Nitin et al., "Yuga: Automatically Detecting Lifetime Annotation Bugs in the Rust Language," ICSE 2024.

#### Other Notable Static Checkers

- **RAPx / SafeDrop** — Use-after-free and memory leakage detection (TOSEM'22, TSE'24). HIR+MIR analysis.
- **FFIChecker** — Memory bugs across Rust/C FFI boundaries (ESORICS'22). LLVM IR level.
- **TypePulse** — Type confusion detection (USENIX Security'25). MIR-based.
- **AtomVChecker** — Memory ordering misuse for atomics (ISSRE'24).
- **Cocoon** — Static information flow control / secrecy leak detection (OOPSLA'24).
- **cargo-pinch** — Pin contract violation detection.

### 3c. Formal Verification and Deductive Verifiers

This category has seen explosive growth in Rust, far outpacing what existed for C in the same timeframe. The Rust Formal Methods Interest Group ([rust-formal-methods.github.io](https://rust-formal-methods.github.io/)) hosts monthly talks and coordinates the community.

#### Kani (AWS)

A bit-precise bounded model checker for Rust (ICSE-SEIP'22). Kani is particularly useful for verifying `unsafe` code blocks where the compiler's guarantees do not hold. It proves memory safety, absence of panics, arithmetic overflow absence, and user-specified assertions. Backed by Amazon Web Services, Kani is at the center of the effort to [verify the safety of the Rust standard library](https://aws.amazon.com/blogs/opensource/verify-the-safety-of-the-rust-standard-library/). Of all the formal verification tools listed here, Kani has the most practical industry backing and the clearest path to production use. Its bounded model checking approach is slower than lightweight analysis but requires less annotation than deductive verifiers. Recommended for safety-critical Rust code.

- Repository: [github.com/model-checking/kani](https://github.com/model-checking/kani)

#### Prusti (ETH Zurich)

A deductive verifier based on the Viper verification infrastructure (NFM'22). Prusti exploits Rust's type system to greatly simplify formal verification — users can verify rich correctness properties with modest annotation overhead. Supports a VSCode extension for interactive verification. Among the most mature deductive verifiers for Rust, with the longest track record. The learning curve is real but manageable for developers willing to write specifications. Recommended for teams that want to go beyond bug-finding to proving functional correctness.

- Paper: Astrauskas et al., "The Prusti Project: Formal Verification for Rust," NFM 2022.
- Repository: [github.com/viperproject/prusti-dev](https://github.com/viperproject/prusti-dev)

#### Verus (Microsoft Research / CMU)

An SMT-based verification tool for Rust with a focus on low-level systems code (OOPSLA'23). Verus uses linear ghost types to handle ownership in proofs. It has been used to verify a concurrent memory allocator (mimalloc) and parts of a verified operating system kernel (Atmosphere, SOSP'25). Rising rapidly in both capability and adoption. Proofs and specifications are written in Rust syntax, lowering the barrier to entry. Among the most promising tools for verified systems programming.

- Paper: Lattuada et al., "Verus: Verifying Rust Programs Using Linear Ghost Types," OOPSLA 2023.
- Repository: [github.com/verus-lang/verus](https://github.com/verus-lang/verus)

#### Creusot (INRIA)

A deductive verifier that translates Rust to WhyML for verification (ICFEM'22). Creusot introduces the notion of prophecy values in specifications, allowing users to write specifications about reborrows that span function calls and loops. Actively maintained with recent additions including support for new language features (January 2026 talk at Rust Formal Methods group). Targets safe Rust; integration with Gillian-Rust (2024) extends coverage to unsafe Rust via separation logic.

- Paper: Denis, Jourdan, and Marché, "Creusot: A Foundry for the Deductive Verification of Rust Programs," ICFEM 2022.
- Repository: [github.com/creusot-rs/creusot](https://github.com/creusot-rs/creusot)

#### Flux

A refinement type checker for Rust (PLDI'23). Flux provides verification through liquid typing — a much lighter-weight approach than program logics. In benchmarks comparing Flux with Prusti, Flux slashed specification lines by a factor of two, verification time by an order of magnitude, and annotation overhead from up to 24% of code size to essentially nothing. Very compelling for the "lightweight verification" use case where you want stronger guarantees than testing but don't need full functional correctness proofs.

- Paper: "Flux: Liquid Types for Rust," PLDI 2023.
- Repository: [github.com/flux-rs/flux](https://github.com/flux-rs/flux)

#### Aeneas (INRIA)

A verification toolchain that translates Rust programs to Lean, Coq, F*, or HOL4 for interactive theorem proving (ICFP'22, ICFP'24). Aeneas eliminates memory reasoning for a large class of Rust programs by leveraging the type system, producing clean functional translations. For teams with expertise in proof assistants, this enables the deepest level of verification available. Recently extended with loop support (ICFP'24).

- Paper: Ho and Protzenko, "Aeneas: Rust Verification by Functional Translation," ICFP 2022.
- Repository: [github.com/AeneasVerif/aeneas](https://github.com/AeneasVerif/aeneas)

#### hax

A Rust verification tool that translates to F* or Rocq (Coq). Used for verified cryptographic implementations (HACL-Rust, Eurydice). Developed at INRIA. CCS'25 paper on formal security and functional verification of cryptographic protocol implementations in Rust. Targets the niche where cryptographic code must be both verified and performant.

- Repository: [github.com/hacspec/hax](https://github.com/hacspec/hax)

#### Other Verifiers

- **RefinedRust** — High-assurance verification combining refinement types with separation logic in Coq (PLDI'24, MPI-SWS). Targets both safe and unsafe Rust.
- **AutoVerus** — LLM-powered automated proof generation for Verus (OOPSLA'25). Interesting research direction.
- **crux-mir** — Static simulator for Rust using symbolic testing (Galois).
- **VeriFast** — Modular formal verification, extended to Rust (NFM'11, ongoing).
- **Soteria Rust** — Symbolic execution engine for Rust (2025, presented at Rust FM group).
- **mendel-verifier** — Capability-based verification for interior mutability.

### 3d. Foundational Theory

#### RustBelt

A formal, machine-checked safety proof for a language representing a realistic subset of Rust (POPL'18). Developed by Ralf Jung, Jacques-Henri Jourdan, Robbert Krebbers, and Derek Dreyer at MPI-SWS. RustBelt provides the semantic foundation that justifies Rust's safety claims and underpins the aliasing models used by Miri. This is to Rust what CompCert is to C — the gold standard for foundational assurance.

- Paper: Jung et al., "RustBelt: Securing the Foundations of the Rust Programming Language," POPL 2018.

#### Stacked Borrows / Tree Borrows

Aliasing models for Rust that define what constitutes undefined behavior for references and raw pointers (POPL'20 and ongoing). Implemented in Miri. Stacked Borrows was the first model; Tree Borrows is a more permissive successor that better accommodates common patterns. Understanding these models is essential for anyone writing `unsafe` Rust.

- Paper: Jung et al., "Stacked Borrows: An Aliasing Model for Rust," POPL 2020.

#### RustHornBelt

A semantic foundation for functional verification of Rust programs with unsafe code (PLDI'22). Combines RustBelt's semantic approach with automated CHC-based verification (RustHorn). Enables verification tools to handle both safe and unsafe code in a unified framework.

#### Polonius

The next-generation borrow checker for Rust, replacing the current NLL (Non-Lexical Lifetimes) analysis. Polonius uses a Datalog-based formulation that is more precise — it accepts more correct programs that the current borrow checker rejects. Available as an experimental flag. Not a standalone tool but an improvement to the compiler's built-in analysis.

---

## 4. Fuzzing and Property Testing

Where static analysis finds bugs by examining code, fuzzing finds them by running it. Rust has an unusually strong fuzzing ecosystem.

### cargo-fuzz

The official Rust fuzzing tool, using LLVM's libFuzzer as the backend. Coverage-guided: instruments the binary and mutates inputs to maximize code coverage. The standard choice for fuzzing Rust code. Requires nightly Rust.

- Repository: [github.com/rust-fuzz/cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
- Guide: [rust-fuzz.github.io/book](https://rust-fuzz.github.io/book/)

### afl.rs

A Rust wrapper around American Fuzzy Lop (AFL), one of the most effective general-purpose fuzzers. AFL is known for its efficiency in generating test cases that uncover deeply hidden bugs. Provides an alternative engine to libFuzzer.

- Repository: [github.com/rust-fuzz/afl.rs](https://github.com/rust-fuzz/afl.rs)

### bolero

A unified property-testing and fuzzing framework that supports libFuzzer, AFL, honggfuzz, and Kani (formal verification) from a single test harness. Write once, run with any engine. This is the state of the art for combining fuzzing with property testing in Rust. Also supports corpus replay during `cargo test`. Highly recommended.

- Repository: [github.com/camshaft/bolero](https://github.com/camshaft/bolero)

### proptest

A property-based testing library inspired by Python's Hypothesis framework. Generates structured inputs based on strategies, with automatic shrinking to minimal failing cases. More expressive than quickcheck for complex input types. Not a fuzzer (no coverage guidance), but complements fuzzing well.

- Repository: [github.com/proptest-rs/proptest](https://github.com/proptest-rs/proptest)

### loom

A concurrency testing tool for Rust that exhaustively explores thread interleavings (used by the Tokio project). Not a fuzzer in the traditional sense, but provides similar bug-finding power for concurrent code.

- Repository: [github.com/tokio-rs/loom](https://github.com/tokio-rs/loom)

---

## 5. Supply Chain and Dependency Analysis

Rust's crates.io ecosystem contains over 150,000 crates. Supply chain security is a first-class concern.

### cargo-audit

Scans `Cargo.lock` for crates with known vulnerabilities reported to the [RustSec Advisory Database](https://rustsec.org/). Built by the Rust Secure Code working group. The absolute minimum security tool every Rust project should run. Does not analyze source code — only checks dependency metadata against known advisories.

- Repository: [github.com/rustsec/rustsec](https://github.com/rustsec/rustsec)

### cargo-deny

Goes further than cargo-audit: enforces policies on licenses, duplicate dependencies, banned crates, and allowed registries/sources. Also includes advisory scanning. Highly configurable. The standard choice for policy enforcement in CI.

- Repository: [github.com/EmbarkStudios/cargo-deny](https://github.com/EmbarkStudios/cargo-deny)

### cargo-geiger

Quantifies `unsafe` code usage across the entire dependency tree. Outputs a tree with symbols: 🔒 (no unsafe, `#![forbid(unsafe_code)]`), ❓ (no unsafe found, but no forbid), ☢️ (unsafe usage found). Essential for understanding your project's attack surface. The name is apt — it measures the "radioactivity" of unsafe code in your dependencies.

- Repository: [github.com/geiger-rs/cargo-geiger](https://github.com/geiger-rs/cargo-geiger)

### cargo-vet (Mozilla)

A supply-chain audit certification system. Tracks which versions of which crates have been audited by your organization or by trusted third parties. Creates a `supply-chain/audits.toml` file with cryptographic audit records. More rigorous than cargo-audit but requires organizational investment.

- Repository: [github.com/mozilla/cargo-vet](https://github.com/mozilla/cargo-vet)

### cargo-crev

A cryptographically-signed, distributed peer review system for Rust crates. Community-driven trust network. Ambitious but adoption has been limited.

- Repository: [github.com/crev-dev/cargo-crev](https://github.com/crev-dev/cargo-crev)

---

## 6. Other Tools (Code Analysis, Visualization, Formatting)

### rust-analyzer

The IDE analysis server for Rust. Provides goto definition, type inference, symbol search, code completion, refactoring, and more. While not a static analysis tool in the traditional sense, it performs sophisticated type-level analysis in real time. The practical equivalent of running a lightweight verifier continuously in your editor.

- Repository: [github.com/rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer)

### rustfmt

The official code formatter. Enforces consistent style across a codebase. Analogous to `clang-format` for C. Configurable via `rustfmt.toml`.

### cargo-semver-checks

Checks that your public API changes are semver-compatible. Catches accidental breaking changes before publishing. Uses `rustdoc` JSON output. No equivalent exists for C.

### Dylint

A tool for running Rust lints from dynamic libraries. Allows developers to maintain personal or organization-specific lint collections without upstreaming to Clippy. Useful for enforcing project-specific coding rules.

- Repository: [github.com/trailofbits/dylint](https://github.com/trailofbits/dylint)

### RustViz / rustowl / Aquascope

Visualization tools for Rust's ownership and borrowing mechanics. Useful for learning and debugging, not for production analysis:
- **RustViz** — generates SVG visualizations of ownership flow.
- **rustowl** — visualizes ownership and lifetimes in MIR.
- **Aquascope** — uses Datalog and compiler internals to produce ownership diagrams (2024 Rust FM talk).

---

## 7. Recommended CI Pipeline

For most Rust projects, the following combination provides strong coverage with minimal effort:

```
cargo fmt --check        # Style consistency
cargo clippy -- -Dwarnings   # 800+ lints
cargo test               # Unit/integration tests
cargo audit              # Known vulnerability scanning
cargo deny check         # License + policy enforcement
```

For projects with `unsafe` code, add:

```
cargo +nightly miri test     # Undefined behavior detection
cargo geiger --forbid-only   # Unsafe code quantification
```

For safety-critical code, add one or more:

```
cargo kani verify            # Bounded model checking
# or Prusti/Verus/Creusot    # Deductive verification
```

For comprehensive testing, add:

```
cargo fuzz run <target>      # Coverage-guided fuzzing
# or cargo bolero test       # Unified fuzzing + property testing
```

---

## 8. Textbooks and Key References

- F. Nielson, H. R. Nielson, and C. Hankin, **Principles of Program Analysis**, Springer-Verlag, ISBN 3-540-65410-0. *(Foundational. Not Rust-specific, but covers the theory behind abstract interpretation, data-flow analysis, and type systems that underpin all tools listed here.)*

- Xavier Denis, **Vérification déductive de programmes Rust** (PhD thesis), Université Paris-Saclay, 2023. *(Comprehensive treatment of deductive verification for Rust, covering Creusot in depth.)*

- Son Ho, **Formal Verification of Rust Programs** (PhD thesis), INRIA Paris. *(Covers Aeneas and the functional translation approach.)*

- Ralf Jung, **Understanding and Evolving the Rust Programming Language** (PhD thesis), Saarland University, 2020. *(The definitive work on RustBelt, Stacked Borrows, and the semantic foundations of Rust's safety guarantees.)*

- Alex Le Blanc, **Surveying the Rust Verification Landscape**, University of Waterloo, 2024 ([arXiv:2410.01981](https://arxiv.org/abs/2410.01981)). *(The most recent comprehensive survey comparing Prusti, Creusot, Verus, Kani, Flux, and others.)*

- **The Rustonomicon** ([doc.rust-lang.org/nomicon](https://doc.rust-lang.org/nomicon/)). *(The official guide to unsafe Rust. Essential reading for understanding what the analysis tools are checking for.)*

---

## 9. Meta-Resources

- **Awesome Rust Checker** — [github.com/BurtonQin/Awesome-Rust-Checker](https://github.com/BurtonQin/Awesome-Rust-Checker). Comprehensive, actively maintained table of Rust checkers with papers, working IR levels, bug types, and last commit dates.

- **Awesome Rust Formalized Reasoning** — [github.com/newca12/awesome-rust-formalized-reasoning](https://github.com/newca12/awesome-rust-formalized-reasoning). Exhaustive list of resources for formalized reasoning in Rust.

- **Rust Formal Methods Interest Group** — [rust-formal-methods.github.io](https://rust-formal-methods.github.io/). Monthly talks since 2021 covering all major verification tools. The best way to stay current.

- **analysis-tools.dev/tag/rust** — [analysis-tools.dev/tag/rust](https://analysis-tools.dev/tag/rust). Ranks 67 Rust linters, analyzers, and formatters with community votes.

- **CMU SEI Blog: Rust Vulnerability Analysis and Maturity Challenges** (2023). Balanced assessment of the state of Rust tooling from a security perspective.

- **AWS Blog: Verify the Safety of the Rust Standard Library** (2024). Describes the multi-tool verification effort using Kani, Prusti, Verus, and Creusot.

---

*last update: 29 May 2025*
