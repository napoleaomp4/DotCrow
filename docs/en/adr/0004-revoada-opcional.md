# 0004. Optional collective learning without personal data

- **Status:** accepted
- **Date:** 2026-10-19

## Context

DotCrow's learning mode lets knowledge from one installation serve all installations (Revoada, Portuguese for a flock of birds flying together). At the same time, people's sensitive data must not be collected or used to train anything.

## Decision

- Sharing is **off** by default and can only be enabled by the owner.
- Only **recipes** (how to perform a type of task) are shared, after a local privacy filter and the owner's approval.
- Received recipes are treated as untrusted external content: they go through Vigia (Portuguese for "lookout") and the policy, and carry a reputation.
- The owner's private data uses end-to-end encryption; recipes are readable by the server and must therefore be anonymous by construction.

## Alternatives considered

- **Share entire conversations:** inevitably leaks personal data.
- **End-to-end encryption for recipes too:** would prevent collective search, the purpose of Revoada.
- **Enabled by default:** contradicts the promise not to collect data without consent.

## Consequences

- Revoada's value grows more slowly because it depends on people opting in to sharing.
- We need to invest in good detectors for personal data in Portuguese (CPF and CNPJ tax IDs, Brazilian phone numbers).
