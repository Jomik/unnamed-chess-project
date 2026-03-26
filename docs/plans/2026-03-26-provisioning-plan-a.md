# Provisioning Plan A: BoardConfig + Token Migration

> **For agentic workers:** Use the `implementing` skill to execute this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `BoardConfig` struct with host-testable validation, and migrate Lichess token types from `&'static str` to `String`.

**Architecture:** `BoardConfig` lives in a new `src/provisioning.rs` module (platform-independent, not under `esp32/`). Validation logic is testable on the host. The ESP32-specific NVS load/save methods will be added in Plan B via a `cfg`-gated `impl BoardConfig` block in `src/esp32/provisioning.rs` — a standard Rust pattern for extending a type with platform-specific methods. This plan only covers the struct, validation, error type, and the Lichess token migration.

**Note:** The spec places everything in `src/esp32/provisioning.rs`. This plan deliberately splits the module: host-testable struct/validation in `src/provisioning.rs`, ESP-IDF methods in `src/esp32/provisioning.rs` (Plan B). This avoids `#[cfg]` guards within the struct definition and keeps tests runnable on the host.

**Tech Stack:** Rust, `thiserror`, `shakmaty` (existing)

**Spec:** `docs/specs/2026-03-26-softap-provisioning-design.md`

---

### Task 1: BoardConfig struct and validation

**Files:**
- Create: `src/provisioning.rs`
- Modify: `src/lib.rs:1-8` (add `pub mod provisioning;`)

#### Context for implementer

The spec defines a `BoardConfig` struct with four fields and validation rules:

| Field           | Type             | Validation                       |
|-----------------|------------------|----------------------------------|
| `wifi_ssid`     | `String`         | Non-empty, ≤ 32 bytes          |
| `wifi_pass`     | `String`         | Non-empty, ≤ 64 bytes          |
| `lichess_token` | `Option<String>` | None (always valid)             |
| `lichess_level` | `u8`             | 1–8 (default: 2)               |

The error type should use `thiserror` per project conventions. Validation is a method on `BoardConfig` so it can be called from both the form handler (Plan B) and tests.

- [ ] **Step 1: Write failing tests for BoardConfig validation**

Create `src/provisioning.rs` with the test module. Tests should cover:
- Valid config passes validation
- Empty SSID rejected
- SSID > 32 bytes rejected
- Empty password rejected
- Password > 64 bytes rejected
- Level 0 rejected
- Level 9 rejected
- Level 1 and 8 accepted (boundary)
- Missing lichess_token (None) is valid
- Default lichess_level is 2

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> BoardConfig {
        BoardConfig {
            wifi_ssid: "MyNetwork".into(),
            wifi_pass: "secret123".into(),
            lichess_token: None,
            lichess_level: 2,
        }
    }

    #[test]
    fn valid_config_passes() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn empty_ssid_rejected() {
        let mut c = valid_config();
        c.wifi_ssid = String::new();
        assert!(matches!(c.validate(), Err(ValidationError::SsidEmpty)));
    }

    #[test]
    fn ssid_over_32_bytes_rejected() {
        let mut c = valid_config();
        c.wifi_ssid = "a".repeat(33);
        assert!(matches!(c.validate(), Err(ValidationError::SsidTooLong)));
    }

    #[test]
    fn ssid_exactly_32_bytes_accepted() {
        let mut c = valid_config();
        c.wifi_ssid = "a".repeat(32);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn empty_password_rejected() {
        let mut c = valid_config();
        c.wifi_pass = String::new();
        assert!(matches!(c.validate(), Err(ValidationError::PasswordEmpty)));
    }

    #[test]
    fn password_over_64_bytes_rejected() {
        let mut c = valid_config();
        c.wifi_pass = "a".repeat(65);
        assert!(matches!(
            c.validate(),
            Err(ValidationError::PasswordTooLong)
        ));
    }

    #[test]
    fn password_exactly_64_bytes_accepted() {
        let mut c = valid_config();
        c.wifi_pass = "a".repeat(64);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn level_zero_rejected() {
        let mut c = valid_config();
        c.lichess_level = 0;
        assert!(matches!(
            c.validate(),
            Err(ValidationError::LevelOutOfRange)
        ));
    }

    #[test]
    fn level_nine_rejected() {
        let mut c = valid_config();
        c.lichess_level = 9;
        assert!(matches!(
            c.validate(),
            Err(ValidationError::LevelOutOfRange)
        ));
    }

    #[test]
    fn level_boundaries_accepted() {
        let mut c = valid_config();
        c.lichess_level = 1;
        assert!(c.validate().is_ok());
        c.lichess_level = 8;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn none_lichess_token_valid() {
        let c = valid_config();
        assert!(c.lichess_token.is_none());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn some_lichess_token_valid() {
        let mut c = valid_config();
        c.lichess_token = Some("lip_abc123".into());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn default_level_is_two() {
        assert_eq!(BoardConfig::DEFAULT_LEVEL, 2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `just test -- provisioning`
Expected: compilation errors (struct and types don't exist yet)

- [ ] **Step 3: Implement BoardConfig, ValidationError, and validate()**

Add to `src/provisioning.rs` (above the test module):

```rust
/// Configuration loaded from NVS at boot.
///
/// Validated before saving to NVS. See [`ValidationError`] for constraints.
#[derive(Debug, Clone)]
pub struct BoardConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub lichess_token: Option<String>,
    pub lichess_level: u8,
}

/// Validation errors for [`BoardConfig`] fields.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("WiFi SSID must not be empty")]
    SsidEmpty,
    #[error("WiFi SSID exceeds 32 bytes")]
    SsidTooLong,
    #[error("WiFi password must not be empty")]
    PasswordEmpty,
    #[error("WiFi password exceeds 64 bytes")]
    PasswordTooLong,
    #[error("Lichess AI level must be 1–8")]
    LevelOutOfRange,
}

impl BoardConfig {
    pub const DEFAULT_LEVEL: u8 = 2;

    /// Validate all fields. Returns the first error found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.wifi_ssid.is_empty() {
            return Err(ValidationError::SsidEmpty);
        }
        if self.wifi_ssid.len() > 32 {
            return Err(ValidationError::SsidTooLong);
        }
        if self.wifi_pass.is_empty() {
            return Err(ValidationError::PasswordEmpty);
        }
        if self.wifi_pass.len() > 64 {
            return Err(ValidationError::PasswordTooLong);
        }
        if self.lichess_level < 1 || self.lichess_level > 8 {
            return Err(ValidationError::LevelOutOfRange);
        }
        Ok(())
    }
}
```

Then add the module to `src/lib.rs`:

```rust
pub mod provisioning;
```

Add it after the existing `pub mod setup;` line.

- [ ] **Step 4: Run tests to verify they pass**

Run: `just test -- provisioning`
Expected: all 13 tests pass

- [ ] **Step 5: Run full check**

Run: `just check`
Expected: fmt, clippy, and all tests pass

- [ ] **Step 6: Commit**

Message: `feat: add BoardConfig struct with validation`

---

### Task 2: Migrate Lichess token from `&'static str` to `String`

**Files:**
- Modify: `src/esp32/lichess.rs:29-37` (Esp32LichessClient)
- Modify: `src/esp32/lichess.rs:39-42` (Esp32LichessGame)
- Modify: `src/esp32/lichess.rs:137-150` (Esp32LichessStreamImpl)

#### Context for implementer

Three structs in `src/esp32/lichess.rs` hold the Lichess API token as `&'static str`. This worked when the token came from `env!()` (compile-time `&'static str`), but now the token comes from NVS at runtime as an owned `String`.

The token flows by ownership transfer through the chain:
1. `Esp32LichessClient` owns the token
2. `challenge_ai(self, ...)` moves `self.token` into `Esp32LichessGame` (line 117)
3. `into_stream(self)` passes `self.token` to `Esp32LichessStreamImpl::connect` (line 132)
4. `connect` stores it as `post_token` (line 184)

All three structs change `&'static str` → `String`. The `connect` function parameter changes too.

**Important:** This code is `#[cfg(target_os = "espidf")]` — it cannot be tested on the host. Verify with `just build` (requires ESP toolchain).

- [ ] **Step 1: Change `Esp32LichessClient` token to `String`**

In `src/esp32/lichess.rs`, change:

```rust
// Before (line 29-36):
pub struct Esp32LichessClient {
    token: &'static str,
}

impl Esp32LichessClient {
    pub fn new(token: &'static str) -> Self {
        Self { token }
    }
}

// After:
pub struct Esp32LichessClient {
    token: String,
}

impl Esp32LichessClient {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}
```

- [ ] **Step 2: Change `Esp32LichessGame` token to `String`**

```rust
// Before (line 39-42):
pub struct Esp32LichessGame {
    game_id: String,
    token: &'static str,
}

// After:
pub struct Esp32LichessGame {
    game_id: String,
    token: String,
}
```

No changes needed in `challenge_ai` — `self.token` is already moved into the struct (line 117).

- [ ] **Step 3: Change `Esp32LichessStreamImpl` and `connect` parameter**

```rust
// Before (line 137-150):
struct Esp32LichessStreamImpl {
    stream_conn: EspHttpConnection,
    post_token: &'static str,
    game_id: String,
    line_buf: Vec<u8>,
}

impl Esp32LichessStreamImpl {
    fn connect(token: &'static str, game_id: &str) -> Result<Self, Esp32LichessError> {

// After:
struct Esp32LichessStreamImpl {
    stream_conn: EspHttpConnection,
    post_token: String,
    game_id: String,
    line_buf: Vec<u8>,
}

impl Esp32LichessStreamImpl {
    fn connect(token: String, game_id: &str) -> Result<Self, Esp32LichessError> {
```

No other changes needed — `format!("Bearer {token}")` on line 164 and `format!("Bearer {}", self.post_token)` on line 256 work identically with `String`.

- [ ] **Step 4: Update `main.rs` call site**

In `src/main.rs` line 126, change:

```rust
// Before:
let client = Esp32LichessClient::new(token);

// After:
let client = Esp32LichessClient::new(token.to_string());
```

This is a temporary bridge — `token` is still `&'static str` from `option_env!()` here. Plan B will replace the entire `option_env!()` block with NVS-sourced config.

- [ ] **Step 5: Verify ESP32 build compiles**

Run: `just build`
Expected: builds successfully (requires `cargo +esp` toolchain)

If the ESP toolchain is not available, at minimum run `just check` to verify host-side code still passes.

- [ ] **Step 6: Commit**

Message: `refactor: change Lichess token from &'static str to String`

---

### Task 3: Update `LichessConfig` doc comment

**Files:**
- Modify: `src/lichess.rs:14-16`

#### Context for implementer

`LichessConfig` has a doc comment saying "constructed from compile-time env vars". This is no longer true after provisioning — it will be constructed from `BoardConfig` fields. Update the doc comment proactively since we're already touching this area.

- [ ] **Step 1: Update doc comment**

In `src/lichess.rs` line 14-16, change:

```rust
// Before:
/// Configuration for Lichess integration, constructed from compile-time env vars.
/// The token is not included here — it is passed directly to the LichessClient
/// constructor, keeping the secret out of a general config struct.

// After:
/// Configuration for Lichess integration.
/// The token is not included here — it is passed directly to the LichessClient
/// constructor, keeping the secret out of a general config struct.
```

- [ ] **Step 2: Run `just check`**

Expected: all checks pass

- [ ] **Step 3: Commit**

Message: `docs: update LichessConfig comment for provisioning`
