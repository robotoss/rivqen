# Documentation style guide

This guide tells you how to write Rivqen documentation. Use it for every page, ADR, issue and pull request description.

[[toc]]

## 1. Language: ASD-STE100 rules

Rivqen documentation uses **Simplified Technical English** (ASD-STE100) as its base. Many readers do not speak English as a first language. Short, clear sentences prevent errors.

<Badge type="tip" text="FACT" /> ASD-STE100 is a controlled-language specification from ASD (AeroSpace and Defence Industries Association of Europe). It has writing rules and a dictionary of approved words. See [Standards](/research/standards#asd-ste100).

We apply the rules at two levels:

| Text type | Where | Rule level |
|---|---|---|
| **Procedures** | Install steps, integration steps, runbooks, test procedures | **Strict** |
| **Safety text** | Warnings, cautions, security notes | **Strict** |
| **Descriptive text** | Concepts, architecture, research, rationale | **Relaxed** |

### 1.1 Strict rules (procedures and warnings)

1. Write **one instruction** in each sentence.
2. Keep each sentence to **20 words or fewer**.
3. Use the **imperative** (command) form: "Add the dependency." Not: "The dependency should be added."
4. Use the **active voice**.
5. Put a **condition before the action**: "If the build fails, do step 4."
6. Write **one step** in each numbered item. Number the steps.
7. Use **one word for one meaning**. Use the terms in the [Glossary](/guide/glossary). Do not use synonyms.
8. Start a warning with the **danger**, then give the **instruction**: "Risk of data leak. Do not cache authenticated pages without a partition."
9. Do not use "should", "may", or "might" in a procedure. Use "do" or "do not".
10. Use "must", "must not", "should", "may" only in **normative specifications** (see 1.3).

**Example — correct:**

1. Open the `build.gradle.kts` file of your app module.
2. Add the Rivqen dependency to the `dependencies` block.
3. Synchronize the project.

**Example — incorrect:**

> You should probably add the dependency, after which the project might need to be synchronized so that Gradle can pick it up.

### 1.2 Relaxed rules (descriptive text)

1. Keep sentences to **25 words or fewer** when possible.
2. Write **one topic** in each paragraph. Keep paragraphs to **six sentences or fewer**.
3. Use the active voice when possible.
4. Use tables and lists instead of long paragraphs.
5. Explain each abbreviation the first time you use it on a page.

### 1.3 Normative language (specifications)

Protocol and conformance pages use the requirement keywords of [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174): **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, **MAY**. Write them in capitals only when you mean the normative meaning.

### 1.4 Technical names

Technical names are not translated or simplified. You can use names of APIs, classes, HTTP headers, RFCs, crates, and packages as they are. Put them in `code` format.

## 2. Status labels

Mark every important statement with one label. Use the VitePress `Badge` component.

| Label | Markup | Use when |
|---|---|---|
| <Badge type="tip" text="FACT" /> | `<Badge type="tip" text="FACT" />` | A source confirms it: upstream code at the pinned commit, a standard, official platform docs, or a test result. Give the source. |
| <Badge type="info" text="DESIGN" /> | `<Badge type="info" text="DESIGN" />` | It is a Rivqen proposal. It is not built. |
| <Badge type="warning" text="RESEARCH" /> | `<Badge type="warning" text="RESEARCH" />` | It needs a spike, a test, or an audit to confirm. |
| <Badge type="danger" text="GATE" /> | `<Badge type="danger" text="GATE" />` | It is a blocking release or phase condition. |

::: danger Do not present a design as a fact
A design statement that has no label is a defect. A reviewer must reject the change.
:::

## 3. Evidence references

Upstream evidence uses this format: `upstream:<path>:<line>` at the pinned commit.

- Pinned upstream commit: `Tencent/VasSonic@59936beff656d4b5718ff6444d6c5e001a2c5231` (2019-04-15).
- Example: `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicSession.java:120`.

Follow the [clean-room rules](/legal/provenance). Do not copy upstream code into the documentation. You can quote header names, constants, marker strings and regular expressions, because they are part of the wire protocol.

## 4. Requirement identifiers

| Prefix | Area | Example |
|---|---|---|
| `INV-` | Architecture invariants | `INV-05` predictable fallback |
| `R-CORE-` | Rust core requirements | `R-CORE-05` |
| `R-CACHE-` | Cache requirements | `R-CACHE-01` |
| `A-` / `I-` / `W-` | Android / iOS / Web requirements | `A-03` |
| `S-` | Server SDK requirements | `S-001` |
| `SEC-` | Security controls | `SEC-09` |
| `HARD-` | Release hardening items | `HARD-05` |
| `PAR-` | Parity items (VasSonic feature → Rivqen) | `PAR-06` |
| `WP-` | Work packages | `WP-04` |
| `G` | Gates | `G2` |
| `Q-` | Open questions | `Q-04` |
| `ADR-` | Architecture decision records | `ADR-003` |
| `FX-` | Golden fixtures | `FX-LEG-304` |

An identifier never changes its meaning. If you remove a requirement, mark it **Withdrawn**. Do not reuse its number.

## 5. Page structure

Each engineering page has this order:

1. **Title** — a noun phrase.
2. **Purpose** — one or two sentences: what the page defines and for whom.
3. **Status** — the label(s) that apply to the page.
4. **Content** — tables first, then text.
5. **Verification** — how we test or prove the content.
6. **Related** — links to ADRs, work packages and requirements.

## 6. Callouts

| Callout | Use for |
|---|---|
| `::: tip` | A useful practice. |
| `::: info` | Context or background. |
| `::: warning` | A risk of wrong behavior or wasted work. |
| `::: danger` | A risk of security failure, data loss or data leak. |
| `::: details` | Long optional content, such as full examples. |

## 7. Diagrams

Use [Mermaid](https://mermaid.js.org/) code blocks. The site renders them and puts wide diagrams in a scroll box.

1. Keep each label to **five words or fewer**. Put long text in the page, not in the diagram.
2. Use `flowchart TD` (top-down) for diagrams with more than six nodes. Top-down diagrams fit narrow screens better.
3. Keep each diagram to **15 nodes or fewer**. Split larger diagrams.
4. Do not use HTML or links inside diagram labels.
5. Give each diagram a sentence before it that says what it shows.
6. Check the diagram in light mode, dark mode, and at 375 px width before you merge.

## 8. Code samples

1. Mark planned APIs with a comment on the first line: `// Planned API — not released`.
2. Give the language for each code block.
3. Do not put secrets, real tokens, or real user data in samples. Use `example.com` hosts.
4. Use `code-group` blocks to show the same step in more than one language.

## 9. Terms

- Write **Rivqen** for the product. Do not write "Sonic Next" or "Sonic" for the product.
- Write **legacy mode** or **legacy protocol** for the temporary VasSonic compatibility. Write "Sonic" only in attribution or in literal legacy wire tokens (`sonicdiff`, header names).
- Write **Rivqen protocol** or **RQP** for the native protocol.
- Write **Rivqen protocol** (short: **RQP**) for the new protocol.
- Write **VasSonic** only for the upstream Tencent project.
- See the full [Glossary](/guide/glossary).

## Related

- [Clean-room provenance](/legal/provenance)
- [Name and brand](/legal/brand)
- [Standards: ASD-STE100](/research/standards#asd-ste100)
