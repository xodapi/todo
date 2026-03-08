---
description: Project health audit skill. Runs automated checks across Security, Architecture, Testing, Code Quality and DX layers.
---

# /doctor - Project Health Audit

$ARGUMENTS

---

## Purpose

Run comprehensive diagnostics on the project to ensure adherence to architectural, security, and code quality rules, as defined in `user_rules`. 

## Layers Checked

1. **Security**: Checks for secrets, `.env` file management, `zeroize` usage for sensitive data.
2. **Architecture**: Validates crates structure, shared types in `protocol`, event bus usage, and `ARCHITECTURE.md` presence.
3. **Testing**: Verifies test coverage, presence of tests in all crates, and automation scripts.
4. **Git / CI**: Checks `.github/workflows`, hooks, conventional commits enforcement, and `CHANGELOG.md` generation.
5. **Code Quality**: Enforces Rust 2024 edition, `x86_64-pc-windows-msvc` target, `cargo clippy` rules, absence of `unwrap()`, `lazy_static`, and `once_cell`.
6. **DX (Developer Experience)**: Validates `rustfmt`, long paths enablement, and observability (`tracing` usage vs `println!`).

---

## Sub-commands

```
/doctor        - Run a full health audit
/doctor scan   - Diagnose the project (same as full health audit)
/doctor quick  - Run a fast, 30-second priority check (Security & Code Quality)
/doctor fix    - Attempt to automatically fix issues found during the audit
```

---

## Diagnostics Commands

### Run Quick Checks (Code Quality & Formatting)
```powershell
// turbo
cargo fmt --check
cargo clippy --workspace --tests -- -D warnings
```

### Check Rust Edition and Forbidden Patterns
```powershell
// turbo
git grep "unwrap()" || echo "No unwraps found"
git grep "lazy_static" || echo "No lazy_static found"
git grep "once_cell" || echo "No once_cell found"
```

### Validate Testing Readiness
```powershell
// turbo
cargo test --workspace --no-run
```

---

## AI Agent Instructions (Active Doctor)

When invoked, the AI Agent must:
1. Identify missing configurations (e.g. MSRV in `Cargo.toml`, CI configs).
2. Report anti-patterns like `Arc<Mutex<Vec<T>>>`, missing `#[expect]`, or `unwrap()` usage.
3. Generate a report formatted as follows:

### Report Format

```markdown
# 🩺 Project Health Report

**Score**: 92/100 (Healthy)

## 🚨 Critical Issues (Security & Core Architecture)
- [ ] Issue 1 (e.g. `unwrap()` found in `src/main.rs`)
- [ ] Issue 2 

## ⚠️ Warnings (Code Quality & DX)
- [ ] Issue 1 (e.g. `cargo fmt` failed)

## ✅ Passing Checks
- Security: No raw `.env` leaks
- Architecture: `crates/protocol/` is correctly used
- Testing: Automated tests run successfully
...
```

To automatically fix code quality issues: Use `/doctor fix` which runs `cargo clippy --fix`, `cargo fmt`, and resolves specific rule violations mentioned in `<MEMORY[user_global]>`.
