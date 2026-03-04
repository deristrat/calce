---
name: new-calculation
description: Add a new financial calculation to the Calce engine. Guides through methodology spec, TDD tests, implementation, and API/Python wiring. Use whenever a task requires adding or substantially extending a calculation.
argument-hint: <description of the calculation>
---

You are adding a new calculation to the Calce financial calculation engine.
The user's request is: $ARGUMENTS

Follow these phases strictly and in order. Do NOT skip ahead. Each phase requires explicit user approval before proceeding to the next.

---

## Phase 1: Understand the calculation

Goal: achieve a complete, unambiguous understanding of what the calculation does.

- If the request is well-described, references a standard financial formula, or points to a specification — confirm your understanding with the user in 2-3 sentences.
- If the request is vague or underspecified — interview the user. Ask about:
  - Inputs (what data does it need? which domain types?)
  - Output (what does the result look like? what currency? per-position or aggregate?)
  - Edge cases (what happens with zero positions, missing data, cross-currency?)
  - Relationship to existing calculations (does it build on `#CALC_MV`, `#CALC_POS_AGG`, etc.?)
- Keep asking until you could explain the calculation to someone else with no ambiguity.

**Do NOT proceed until the user confirms you have the right understanding.**

---

## Phase 2: Update methodology documentation

Read `docs/calculations/methodology.md` and `CLAUDE.md` (the calculation reference table).
See [methodology-template.md](methodology-template.md) for the section format.

Then:

1. Choose a `#CALC_*` tag for the new calculation (e.g. `#CALC_TWR`).
2. Write a new section in `docs/calculations/methodology.md` following the existing style:
   - Clear prose description of what the calculation does and why
   - Formulae in the indented pseudocode style used by existing sections
   - Explicit statement of edge cases and error conditions
   - Note any new assumptions that should be added to Section 1
   - Note any new conventions that should be added to Section 2
3. If this calculation introduces new core domain types, document them and note they need to be added to `domain/`.
4. Check whether existing sections need cross-references to the new calculation.
5. Update the calculation reference table in `CLAUDE.md` with the new tag and planned source file.

Present the full methodology diff to the user. **Do NOT proceed until the user approves the methodology.**

Iterate if the user has feedback — this is the specification, so it must be precise.

---

## Phase 3: Write tests (TDD red phase)

Design tests that a human can read and mentally verify: "if these pass, the calculation is correct."

Guidelines:
- Put tests in the appropriate module (usually `crates/calce-core/src/calc/<module>.rs` or a new file)
- Use the existing test patterns: `InMemoryMarketDataService`, `InMemoryUserDataService`, static dates and prices
- Keep the test count small and focused. Aim for 3-7 tests covering:
  - **Happy path**: basic case with known inputs and expected outputs
  - **Cross-currency**: if the calculation involves monetary values, test with FX conversion
  - **Edge cases**: zero positions, single position, boundary dates as appropriate
  - **Error cases**: missing price, missing FX rate, unauthorized access — but only those relevant to this calculation
- Each test should have a descriptive name that reads as a specification
- Use concrete, easy-to-verify numbers (round prices like 100.0, 200.0; round FX rates like 10.0)
- Write the function signature with a `todo!()` body so tests compile against real types

**Do NOT write the implementation yet.** The tests should compile but fail (red phase).

Present the tests to the user. **Do NOT proceed until the user approves the tests.**

---

## Phase 4: Implement the calculation

Now implement the calculation in `calce-core`:

1. Add any new domain types to `crates/calce-core/src/domain/` if needed (with `cfg_attr` serde derives).
2. Create the calculation function in `crates/calce-core/src/calc/` following existing patterns:
   - Pure function taking positions/trades, `CalculationContext`, and service trait references
   - No side effects, no async
   - Add the `#CALC_*` tag to the function's doc comment
   - Add `# Errors` and `# Panics` doc sections as required by clippy::pedantic
3. If this is a composed calculation, add it to `crates/calce-core/src/reports/`.
4. Wire it into `CalcEngine` in `crates/calce-core/src/engine.rs` if it should be accessible as a top-level engine operation.

Run tests and clippy:
```
cargo test -p calce-core
cargo clippy -p calce-core -- -D warnings
```

Fix until all tests pass (green phase). Report results to the user.

---

## Phase 5: Sanity check with seed data

Before wiring up endpoints, verify the calculation produces sensible results with the existing seed data.

1. Add any needed seed data to `crates/calce-api/src/seed.rs`
2. Write a quick `#[test]` in calce-api (or use an existing integration test) that runs the new calculation against seed data and prints the result
3. Share the output with the user — do the numbers make sense?

This catches issues that unit tests with synthetic data may miss (e.g. missing FX rate paths, unexpected interactions between positions).

---

## Phase 6: API endpoint and Python bindings

Once the core calculation works and sanity check passes:

1. **API endpoint** — Add a route in `crates/calce-api/src/routes.rs` and register it in `main.rs`. Follow the existing handler pattern (extract auth, parse query params, construct `CalcEngine`, call method, return JSON).

2. **Python bindings** — Add a method to the Python module in `crates/calce-python/src/lib.rs` following the existing pattern. Include a Python docstring explaining inputs and return value.

3. Run the full check:
```
cargo test
cargo clippy -- -D warnings
```

Report results to the user.

---

## Summary of checkpoints

| Phase | Gate |
|-------|------|
| 1. Understand | User confirms understanding |
| 2. Methodology | User approves doc changes |
| 3. Tests | User approves test design |
| 4. Implement | Tests pass, clippy clean |
| 5. Sanity check | Numbers look right to user |
| 6. API + Python | Full workspace builds and passes |
