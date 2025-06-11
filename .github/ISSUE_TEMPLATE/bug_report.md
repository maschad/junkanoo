---
name: Bug Report
about: Create a report to help us improve
title: '[BUG] '
labels: bug
assignees: ''
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to fill out this bug report!
  - type: textarea
    id: bug-description
    attributes:
      label: Bug Description
      description: Provide a clear and concise description of what the bug is
      placeholder: What's the bug?
    validations:
      required: true
  - type: textarea
    id: steps-to-reproduce
    attributes:
      label: Steps to Reproduce
      description: List the steps to reproduce the bug
      placeholder: |
        1.
        2.
        3.
    validations:
      required: true
  - type: textarea
    id: expected-behavior
    attributes:
      label: Expected Behavior
      description: What you expected to happen
    validations:
      required: true
  - type: textarea
    id: actual-behavior
    attributes:
      label: Actual Behavior
      description: What actually happened
    validations:
      required: true
  - type: dropdown
    id: platform
    attributes:
      label: Platform
      description: Select your operating system
      options:
        - macOS
        - Linux
        - Windows
        - Other
    validations:
      required: true
  - type: textarea
    id: os-version
    attributes:
      label: OS Version
      description: Your operating system version (e.g., macOS 13.1, Ubuntu 22.04, Windows 11)
      placeholder: e.g., macOS 13.1, Ubuntu 22.04, Windows 11
    validations:
      required: true
  - type: textarea
    id: rust-version
    attributes:
      label: Rust Version
      description: Output of `rustc --version`
      placeholder: rustc 1.87.0
    validations:
      required: true
  - type: textarea
    id: junkanoo-version
    attributes:
      label: junkanoo Version
      description: Output of `junkanoo --version` or the version you're using
    validations:
      required: true
  - type: textarea
    id: logs
    attributes:
      label: Logs and Error Messages
      description: Please provide any relevant logs or error messages
      placeholder: |
        ```bash
        # Terminal output or logs here
        ```
    validations:
      required: true
  - type: textarea
    id: additional-context
    attributes:
      label: Additional Context
      description: Add any other context about the problem here
  - type: textarea
    id: possible-solution
    attributes:
      label: Possible Solution
      description: If you have suggestions on how to fix the bug
  - type: textarea
    id: screenshots
    attributes:
      label: Screenshots
      description: If applicable, add screenshots to help explain your problem
---

## Bug Description
<!-- Provide a clear and concise description of what the bug is -->

## Steps to Reproduce
1.
2.
3.

## Expected Behavior
<!-- What you expected to happen -->

## Actual Behavior
<!-- What actually happened -->

## Environment
<!-- Please fill out the following information -->

### Platform
- [ ] macOS
- [ ] Linux
- [ ] Windows
- [ ] Other (please specify):

### OS Version
<!-- e.g., macOS 13.1, Ubuntu 22.04, Windows 11 -->

### Rust Version
<!-- Output of `rustc --version` -->

### junkanoo Version
<!-- Output of `junkanoo --version` or the version you're using -->

## Logs and Error Messages
<!-- Please provide any relevant logs or error messages -->
```bash
# Terminal output or logs here
```

## Additional Context
<!-- Add any other context about the problem here -->

## Possible Solution
<!-- If you have suggestions on how to fix the bug -->

## Screenshots
<!-- If applicable, add screenshots to help explain your problem -->