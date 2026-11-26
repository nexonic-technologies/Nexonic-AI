# Security policy

## Reporting a vulnerability

Please report security issues privately through GitHub's
[private vulnerability reporting](https://github.com/nexonic-technologies/Nexonic-AI/security/advisories/new)
or by email to info@nexonictechnologies.com. Do not open a public issue.

## Principles

- **Local first.** Camera, microphone and screen data are processed on the device by default.
  Raw recordings are never kept unless the user turns that on.
- **Nothing leaves without consent.** Cloud context sharing is a separate, revocable permission.
- **Isolation is enforced in code.** Sharee's private memory, Nexa's private memory and shared
  memory are separate storage domains; confidentiality does not depend on model instructions.
- **Least privilege.** Every capability (camera, files, automation, proactive voice, …) is an
  individually revocable permission with an emergency stop that revokes everything at once.
- **Untrusted content stays untrusted.** Web pages, documents, emails and screen text are data,
  never instructions that grant authority.
- **Credentials live in the macOS Keychain**, never in files or the repository.

## Repository protections

- Owner-only commit checks and attribution checks (`scripts/owner-policy.sh`, `Guard` workflow).
- Full-history secret scanning with gitleaks on every push and pull request.
- Personal data, credentials, databases and model adapters are ignored and blocked by CI.
- Third-party GitHub Actions are pinned to commit SHAs.
