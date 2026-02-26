---
description: "Rust programming language coding conventions and best practices"
applyTo: "**/*.rs"
---

# Rust Coding Conventions and Best Practices

Follow idiomatic Rust practices and community standards when writing Rust code.

These instructions are based on [The Rust Book](https://doc.rust-lang.org/book/), [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), [RFC 430 naming conventions](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md), and the broader Rust community at [users.rust-lang.org](https://users.rust-lang.org).

## Project Context

**This is an application, not a library.** Contributors are expected to:

- Understand the chess board hardware and sensor architecture
- Read code to understand usage patterns
- Have basic Embedded Rust knowledge for ESP32 work

Documentation should be **concise and focused on "what" and "why"**, not tutorials on "how to use" basic APIs.

## General Instructions

- Always prioritize readability, safety, and maintainability.
- Use strong typing and leverage Rust's ownership system for memory safety.
- Break down complex functions into smaller, more manageable functions.
- For algorithm-related code, include explanations of the approach used.
- Write code with good maintainability practices, including comments on **why** certain design decisions were made.
- Handle errors gracefully using `Result<T, E>` and provide meaningful error messages.
- For external dependencies, mention their usage and purpose in documentation.
- Use consistent naming conventions following [RFC 430](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md).
- Write idiomatic, safe, and efficient Rust code that follows the borrow checker's rules.
- Ensure code compiles without warnings.

## Patterns to Follow

- Use modules (`mod`) and public interfaces (`pub`) to encapsulate logic.
- Handle errors properly using `?`, `match`, or `if let`.
- Use `thiserror` for custom error types (the only error library in this project).
- Implement traits to abstract services or external dependencies.
- Prefer enums over flags and states for type safety.
- Use builders for complex object creation.
- Split binary and library code (`main.rs` vs `lib.rs`) for testability and reuse.
- Use iterators instead of index-based loops as they're often faster and safer.
- Use `&str` instead of `String` for function parameters when you don't need ownership.
- Prefer borrowing and zero-copy operations to avoid unnecessary allocations.

## Patterns to Avoid

- No `unwrap()` in production code — use `?` or `expect()` only where logically impossible to fail, with a clear message.
- Don't rely on global mutable state—use dependency injection or thread-safe containers.
- Avoid deeply nested logic—refactor with functions or combinators.
- Don't ignore warnings—treat them as errors during CI.
- Avoid `unsafe` unless required and fully documented.
- Don't overuse `clone()`, use borrowing instead of cloning unless ownership transfer is needed.
- Avoid premature `collect()`, keep iterators lazy until you actually need the collection.
- Avoid unnecessary allocations—prefer borrowing and zero-copy operations.

## Code Style and Formatting

- Follow the Rust Style Guide and use `rustfmt` for automatic formatting.
- Keep lines under 100 characters when possible.
- Place function and struct documentation immediately before the item using `///`.
- Use `cargo clippy` to catch common mistakes and enforce best practices.

## Error Handling

- Use `Result<T, E>` for recoverable errors and `panic!` only for unrecoverable errors.
- No `unwrap()` in production code — use `?` or `expect()` only where logically impossible to fail, with a clear message.
- Create custom error types using `thiserror`.
- Use `Option<T>` for values that may or may not exist.
- Provide meaningful error messages and context.
- Error types should be meaningful and well-behaved (implement standard traits like `Debug`, `Display`, `Error`).
- Validate function arguments and return appropriate errors for invalid input.
- **Never use `()` as an error type**—always create a proper error enum or struct.

## API Design Guidelines

### Common Traits Implementation

Eagerly implement common traits where appropriate:

- `Copy`, `Clone`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, `Hash`, `Debug`, `Display`, `Default`
- Use standard conversion traits: `From`, `AsRef`, `AsMut`

### Type Safety and Predictability

- Use newtypes to provide static distinctions
- Arguments should convey meaning through types; prefer specific types over generic `bool` parameters
- Use `Option<T>` appropriately for truly optional values
- Functions with a clear receiver should be methods
- Only smart pointers should implement `Deref` and `DerefMut`

### Future Proofing

- Structs should have private fields
- Functions should validate their arguments
- All public types must implement `Debug`

## Testing and Documentation

### Testing

- Write comprehensive unit tests using `#[cfg(test)]` modules and `#[test]` annotations.
- Use test modules alongside the code they test (`mod tests { ... }`).
- Write integration tests in `tests/` directory with descriptive filenames.
- Test edge cases, error conditions, and hardware boundary cases.

### Documentation Guidelines

**Documentation Level**: Concise technical documentation for contributors, not library users.

#### What to Document

- **Brief summaries** of what public types and functions do
- **Hardware specifics** (pin assignments, timing requirements, sensor behavior)
- **"Why" over "how"**: Document design decisions and rationale
- **Non-obvious behavior**: Inversion of sensor logic, bitboard layouts, etc.
- **TODOs**: Mark incomplete implementations clearly

#### What NOT to Document

- ❌ Basic usage examples for simple traits/functions
- ❌ Tutorials on how to implement standard traits
- ❌ Hand-holding examples showing obvious usage patterns
- ❌ Extensive API examples (contributors can read the code)

#### Format

```rust
/// Brief description of what this does.
///
/// Additional context about design decisions or non-obvious behavior.
/// Hardware-specific details like timing constraints or sensor characteristics.
pub struct MyType { ... }
```

**Good Example:**

```rust
/// DRV5032FB outputs LOW when south pole magnet detected (piece present).
/// The 74HC165 shifts this data out serially when clocked.
pub struct Esp32PieceSensor { ... }
```

**Avoid:**

````rust
/// A trait for reading piece positions from physical sensors.
///
/// # Examples
/// ```
/// let mut sensor = MockPieceSensor::new();
/// let positions = sensor.read_positions();
/// ```
///
/// # Implementation Notes
/// - Hardware implementations should handle errors gracefully
/// - The method is mutable to allow state updates
/// ...lengthy explanation of basic Rust concepts...
````

#### Documentation Completeness

- All public items should have at least a brief `///` comment
- Link related types using backticks and square brackets: `[`Square`]`
- Complex hardware interactions deserve detailed comments
- Simple getters/setters need minimal documentation

## Project Organization

- Use feature flags for optional functionality.
- Organize code into modules using `mod.rs` or named files.
- Keep `main.rs` or `lib.rs` minimal — move logic to modules.
- **This is not published to crates.io** — omit `license`, `repository`, `keywords`, `categories` from `Cargo.toml`.

## Quality Checklist

Before submitting code for review, ensure:

### Core Requirements

- [ ] **Naming**: Follows RFC 430 naming conventions
- [ ] **Traits**: Implements `Debug` (at minimum) and `Clone`, `PartialEq` where appropriate
- [ ] **Error Handling**: Uses `Result<T, E>` with proper error types (not `()`)
- [ ] **Documentation**: All public items have brief rustdoc comments explaining what and why
- [ ] **Testing**: Test coverage for new functionality and edge cases

### Safety and Quality

- [ ] **Safety**: No unnecessary `unsafe` code, proper error handling (no `unwrap()` in production code)
- [ ] **Performance**: Efficient use of iterators, minimal allocations
- [ ] **API Design**: Functions are predictable, flexible, and type-safe
- [ ] **Future Proofing**: Private fields in structs, sealed traits where appropriate
- [ ] **Tooling**: Code passes `cargo fmt`, `cargo clippy`, and `cargo test`
- [ ] **Hardware**: Comments explain hardware-specific behavior and constraints
