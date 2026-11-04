# Contributing

Thank you for helping improve Commercial Preflight. The project is in an early
v0.1 stage, so focused changes with tests are easiest to review.

## Development setup

The supported development environment is Windows with:

- Rust 1.88 or later;
- Windows PowerShell 5.1 or later;
- a current Node.js 22 release (or later) and npm;
- Microsoft C++ Build Tools and WebView2 from the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/); and
- Python 3.11 or later plus `requirements.txt` when changing the frozen Python
  reference.

Install the desktop dependencies with `npm ci` in `apps\desktop`. Rust
dependencies are locked in `Cargo.lock`.

## Workflow

1. Open an issue for substantial behavior or architecture changes before
   implementation.
2. Create a focused branch and keep unrelated formatting or generated output
   out of the change.
3. Add or update tests for observable behavior.
4. Use only synthetic fixtures. Never commit customer documents, credentials,
   real commercial values, local databases, or machine-specific paths.
5. Run the checks below and describe user-visible effects and limitations in
   the pull request.

## Validation

From the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

Push-Location apps\desktop
npm run typecheck
npm test
npm run build
Pop-Location
```

When changing `preflight/` or the reference behavior, also run:

```powershell
python -m unittest discover -s tests -p "test_*.py"
```

## Pull requests

Keep commits reviewable, explain privacy or compatibility implications, and
update public documentation when behavior changes. Do not commit `target/`,
`node_modules/`, `dist/`, logs, databases, or editor settings.

Potential vulnerabilities and sensitive reports belong in the private process
described in [SECURITY.md](SECURITY.md).
