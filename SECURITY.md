# Security Policy

## Supported Versions

Security fixes are released for the latest published version only.

| Version | Supported |
| ------- | --------- |
| Latest release | ✅ |
| Older releases | ❌ |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public issues, discussions, or pull requests.**

Use GitHub's private vulnerability reporting instead:

1. Open the [Security tab](https://github.com/john3sobieski/radiant-audio-player/security) of this repository.
2. Click **Report a vulnerability**.
3. Describe the problem as precisely as you can.

If you cannot use this form, open a public issue titled "Security contact request" **without any technical details**, and I will arrange a private channel.

### What to include

- The affected version (see *Settings → Apps* on the device, or the release tag).
- Android version and device model.
- Steps to reproduce, or a proof of concept.
- The impact you expect (for example: unauthorized file access, code execution, data leak).
- Logs or screenshots, if they help.

## What to Expect

- **Acknowledgement** within 7 days.
- **Initial assessment** within 14 days, with a decision on severity and a rough timeline.
- **Fix and release** as soon as practical; critical issues are prioritized.
- **Credit** in the release notes, if you want it. Tell me how you would like to be named.

This is a personal open-source project maintained in spare time, so these timelines are best-effort rather than guarantees.

## Scope

In scope:

- The application code in this repository.
- Build and release configuration (signing, CI workflows).

Out of scope:

- Vulnerabilities in third-party libraries that are already fixed upstream (update the dependency instead; Dependabot tracks these).
- Issues that require a rooted device or physical access to an unlocked device.
- Social engineering and spam.

## Disclosure

Please allow time for a fix before disclosing publicly. Once a fix is released, a GitHub Security Advisory will be published describing the issue and the affected versions.
