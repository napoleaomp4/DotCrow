# 0002. Hybrid post-quantum cryptography

- **Status:** accepted
- **Date:** 2026-10-19

## Context

We want DotCrow to resist quantum computers, including attackers recording traffic today to decrypt it in the future. The post-quantum algorithms standardized by NIST (ML-KEM, FIPS 203; ML-DSA, FIPS 204) are recent, and new implementations may have flaws.

## Decision

Always use **hybrid** schemes:

- Key agreement: ML-KEM-768 **and** X25519, combined with HKDF-SHA256 binding the KEM ciphertext and public keys (in the spirit of the X-Wing combiner).
- Encryption: XChaCha20-Poly1305.
- Signatures (planned): Ed25519 **and** ML-DSA-65, both required for verification.
- Transport (planned): TLS 1.3 with the X25519MLKEM768 hybrid group.

We only use implementations from established crates (RustCrypto, dalek).

## Alternatives considered

- **ML-KEM alone:** loses protection if a classical attack against the implementation or scheme is discovered.
- **X25519 alone:** does not resist quantum computers.
- **Standardized X-Wing:** this is the goal; we will adopt it once the standard and Rust implementation are stable, with a new envelope format version.

## Consequences

- Envelopes get larger (about 1.1 KB of headers because of ML-KEM-768). Acceptable for our use case.
- The format has a version byte, allowing scheme migrations without breaking old data.
- We need an external audit before 1.0.
