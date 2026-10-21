# 0003. Write-only key vault

- **Status:** accepted
- **Date:** 2026-10-19

## Context

The computer's owner requested that no API key be exposed: only the owner adds keys, and not even the owner can view them afterward, only delete and create another. API keys are worth money, and any read path becomes a target for scripts, extensions and prompt injection.

## Decision

The vault exposes only `add`, `list` (metadata and fingerprint) and `delete`. The only access to the secret is `lease`, used by the provider adapter to build the request. In phase 1 the vault moves inside Ninho (Portuguese for "nest", the local service), and the local CLI/app-to-Ninho protocol will have no key-reading verb.

## Alternatives considered

- **Display a masked key (last 4 characters):** leaks part of the secret and creates a habit of "checking by looking". The fingerprint allows verification without leaking anything.
- **Password-protected export command:** reopens exactly the path we want to close.

## Consequences

- To replace a key, the owner deletes it and adds another.
- If the owner loses access to the provider account, the stored key does not help recover it. This is intentional.
