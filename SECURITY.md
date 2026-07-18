# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in DevFlow, please report it privately.

**Do not open a public issue.**

Email: security@dennis.dev (or contact the maintainer directly via GitHub).

We will respond within 48 hours and work with you to understand and address the issue.

## Supported Versions

Only the latest release receives security updates.

| Version | Supported |
|---|---|
| v1.0.0+ | ✅ |
| < v1.0.0 | ❌ |

## Scope

Security issues may include:

- Command injection via crafted prompts or config
- Arbitrary file access through worktree or path manipulation
- Shell metacharacter escaping issues
- Sensitive data exposure in logs or state files
- Denial of service via resource exhaustion

## Best Practices

When using DevFlow:

- Review agent prompts before running in production
- Do not expose `.devflow/state-NN.json` or `.devflow/events.jsonl` to untrusted contexts
- Use `devflow doctor` to verify your environment
- Use the latest release
