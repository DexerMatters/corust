# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0]

### Added
- Initial implementation of `type_enum!` macro for GADTs in Rust
- Type indexing support (variants can specify different type parameters)
- Phantom types for compile-time state tracking
- Trait object support with `Box<dyn Trait>`
- `match_t!` macro for runtime pattern matching on trait objects
- Method definitions with type-indexed return types
- Automatic type parameter filtering and PhantomData injection
- Trait bounds preservation in generated code

### [0.1.1]

### Added
- Support for variant-level generics, allowing each variant to have its own generic parameters with trait bounds
