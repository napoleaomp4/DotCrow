# Revoada: collective memory

> *Revoada* is Portuguese for a flock of birds flying together.

The central idea of learning mode: **what one DotCrow learns benefits every one that comes after it.** If someone taught their DotCrow to issue a service invoice on a municipal portal, the next DotCrow that needs to do it already knows the steps.

This only works if it is safe. This document defines what is shared, what never is and how the flock protects itself from attempts to poison its memory. **Status:** planned for phase 4; the recipe format and privacy filter begin in phase 3.

## What is shared: recipes

A **recipe** describes *how* to perform a type of task, never *for whom* or *with which data*.

The example keeps the original field names and discriminator (`"tipo": "receita"`). Its descriptive values are translated; NFS-e is a Brazilian electronic service invoice, and ABRASF is the referenced model.

```json
{
  "tipo": "receita",
  "versao": 1,
  "objetivo": "Issue a service invoice on the municipal portal",
  "contexto": { "sistema": "NFS-e portal (ABRASF model)", "so": "any" },
  "passos": [
    "Open the portal and sign in with the company's digital certificate",
    "Go to Issuance > New NFS-e",
    "Fill in the service recipient from the customer record",
    "Check the tax rate and service code before issuing",
    "Download the PDF and XML"
  ],
  "cuidados": ["Issuance cannot be undone, only canceled within the allowed period"],
  "resultado": { "sucessos": 14, "falhas": 1 }
}
```

## What is **never** shared

- Files, documents, e-mails, messages, screens.
- Names, CPF/CNPJ (Brazilian individual/company tax IDs), e-mail addresses, phone numbers, addresses, monetary amounts, account numbers.
- File paths, operating system usernames, URLs containing tokens.
- API keys, passwords, certificates.
- Conversation history.

Your data is **not** used to train DotCrow or any LLM.

## A recipe's path to Revoada

1. **Muninn** ("memory", one of Odin's ravens) writes a recipe after a successful task and stores it **locally**.
2. The **privacy filter** runs on your machine and removes or rejects anything resembling personal data (detectors for e-mail addresses, CPF/CNPJ, phone numbers, paths, tokens, monetary amounts).
3. If sharing is **enabled** (it is off by default), you review and approve the final recipe, or enable automatic approval only for recipes the filter classified as clean.
4. The recipe is **signed** with your installation's identity and sent over hybrid post-quantum TLS.
5. Revoada stores the recipe without linking it to your public identity: the signature is used for reputation and blocking abuse, not to show who sent it.

## Defense against poisoning

Recipes from Revoada are **external content**: suggestions, never instructions.

- **Vigia** (Portuguese for "lookout") reviews each suggested step as if it came from a stranger. No recipe bypasses the permission policy or confirmations.
- Recipes contain descriptions of steps, not ready-to-run commands.
- **Reputation:** a recipe gains weight as independent installations report success with it. New recipes or those from installations without a track record appear as "unverified".
- Installations sending malicious recipes have their certificates revoked.
- Anyone can report a recipe.

## Why Revoada is not "end-to-end encrypted"

For the whole flock to search for a recipe, the server must be able to read its content. To be clear:

- **Recipes** are encrypted in transit and at rest, but **the Revoada server reads them**. That is why they must be anonymous by construction (the steps above).
- **Your private data** (personal memory, history, synchronization between your devices) uses **end-to-end encryption**: the server only stores blobs it cannot open.

## Open questions

Good topics for issues before phase 4:

- How detailed can a recipe be without enabling reidentification in small systems (a portal used by a single company)?
- Should we apply differential privacy to success and failure counters?
- How can we make Revoada self-hostable for companies that want a private flock?
